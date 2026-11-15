use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::PathBuf, process::Stdio, sync::Arc};
use tokio::{io::AsyncReadExt, process::Command, sync::Mutex};

#[derive(Deserialize, Serialize)]
struct Config {
    base_url: String,
    model: String,
    #[serde(default)]
    api_key: String,
}

struct App {
    config_path: PathBuf,
    prompt_path: PathBuf,
    client: reqwest::Client,
    history: Mutex<Vec<Value>>,
    origin: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let path = std::env::var_os("KRON_CONFIG").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("kron.json"));
    if !path.exists() {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&Config {
                base_url: "https://api.openai.com/v1".into(),
                model: "gpt-4.1".into(),
                api_key: String::new(),
            })?,
        )?;
        eprintln!("Created {}. Set base_url, model and api_key before sending a message.", path.display());
    }
    let runtime = std::env::var_os("KRON_RUNTIME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("runtime"));
    let ui = runtime.join("ui");
    let prompt_path = runtime.join("system.md");
    std::fs::read_to_string(&prompt_path).context("Cannot read system.md in KRON_RUNTIME")?;
    if cfg!(target_os = "android") {
        let output =
            Command::new("su").args(["-c", "id -u"]).output().await.context("Android requires su root access")?;
        if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != "0" {
            bail!("Grant KRON root access in your root manager");
        }
    }
    let listen = std::env::var("KRON_LISTEN").unwrap_or_else(|_| "127.0.0.1:7790".into());
    let address: std::net::SocketAddr = listen.parse()?;
    if !address.ip().is_loopback() {
        bail!("KRON_LISTEN must be a loopback address");
    }
    let listener = tokio::net::TcpListener::bind(address).await?;
    let app = Arc::new(App {
        config_path: path,
        prompt_path,
        client: reqwest::Client::new(),
        history: Mutex::new(Vec::new()),
        origin: format!("http://{}", listener.local_addr()?),
    });
    eprintln!("{}", app.origin);
    axum::serve(
        listener,
        Router::new()
            .fallback_service(tower_http::services::ServeDir::new(ui))
            .route("/message", post(message))
            .with_state(app),
    )
    .await?;
    Ok(())
}

#[derive(Deserialize)]
struct Input {
    text: String,
}

type ApiError = (StatusCode, String);

async fn message(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(input): Json<Input>,
) -> Result<Json<Value>, ApiError> {
    // A root console is local only. Require the page's origin and custom header
    // so arbitrary websites cannot send commands through this endpoint.
    if headers.get("origin").and_then(|v| v.to_str().ok()) != Some(app.origin.as_str())
        || headers.get("x-kron").and_then(|v| v.to_str().ok()) != Some("1")
    {
        return Err((StatusCode::FORBIDDEN, "Open KRON on its local address".into()));
    }
    if input.text.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Empty message".into()));
    }
    let mut history = app.history.try_lock().map_err(|_| (StatusCode::CONFLICT, "Agent is busy".into()))?;
    // A failed request must not leave an incomplete tool exchange in history.
    let prompt = std::fs::read_to_string(&app.prompt_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Read system.md: {e}")))?;
    let mut next = history.clone();
    let system = json!({"role":"system", "content":prompt});
    if next.is_empty() {
        next.push(system);
    } else {
        next[0] = system;
    }
    next.push(json!({"role":"user", "content":input.text}));
    let answer = run(&app, &mut next).await.map_err(|e| (StatusCode::BAD_GATEWAY, format!("{e:#}")))?;
    *history = next;
    Ok(Json(json!({"text":answer})))
}

async fn run(app: &App, messages: &mut Vec<Value>) -> Result<String> {
    let config: Config = serde_json::from_slice(&std::fs::read(&app.config_path)?)?;
    loop {
        let body = json!({"model":config.model, "messages":messages, "tools":[{
            "type":"function", "function":{
                "name":"console", "description":"Execute a shell command on this device with full permissions. Returns exit code, stdout and stderr. Each invocation uses a fresh shell.",
                "parameters":{"type":"object", "properties":{"command":{"type":"string"}}, "required":["command"], "additionalProperties":false}
            }
        }]});
        let mut request =
            app.client.post(format!("{}/chat/completions", config.base_url.trim_end_matches('/'))).json(&body);
        if !config.api_key.is_empty() {
            request = request.bearer_auth(&config.api_key);
        }
        let response = request.send().await?;
        let status = response.status();
        if !status.is_success() {
            bail!("Model API returned HTTP {status}");
        }
        let value: Value = response.json().await?;
        let reply = value["choices"][0]["message"].clone();
        if reply["role"] != "assistant" {
            bail!("Model API returned no assistant message");
        }
        let calls = reply["tool_calls"].as_array().cloned().unwrap_or_default();
        let text = reply["content"].as_str().unwrap_or_default().to_owned();
        messages.push(reply);
        if calls.is_empty() {
            return Ok(text);
        }
        for call in calls {
            let result = async {
                if call["function"]["name"] != "console" {
                    bail!("Unknown tool; use console");
                }
                let args: Value =
                    serde_json::from_str(call["function"]["arguments"].as_str().context("Missing arguments")?)?;
                console(args["command"].as_str().context("Missing command")?).await
            }
            .await;
            messages.push(json!({"role":"tool", "tool_call_id":call["id"],
                "content":match result { Ok(output)=>output, Err(e)=>format!("Console error: {e:#}") }}));
        }
    }
}

async fn capture(mut reader: impl tokio::io::AsyncRead + Unpin) -> Result<String> {
    let mut kept = Vec::new();
    let mut buf = [0; 4096];
    let mut omitted = false;
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        let take = n.min(32_768 - kept.len());
        kept.extend_from_slice(&buf[..take]);
        omitted |= take < n;
    }
    let mut text = String::from_utf8_lossy(&kept).into_owned();
    if omitted {
        text.push_str("\n[output truncated]");
    }
    Ok(text)
}

async fn console(command: &str) -> Result<String> {
    let mut shell = if cfg!(target_os = "android") {
        let mut c = Command::new("su");
        c.args(["-c", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };
    let cwd = std::env::current_dir()?;
    shell
        .env(
            "KRON_RUNTIME",
            cwd.join(std::env::var_os("KRON_RUNTIME").map(PathBuf::from).unwrap_or_else(|| "runtime".into())),
        )
        .env(
            "KRON_CONFIG",
            cwd.join(std::env::var_os("KRON_CONFIG").map(PathBuf::from).unwrap_or_else(|| "kron.json".into())),
        );
    let mut child =
        shell.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn()?;
    let (out, err, status) =
        tokio::join!(capture(child.stdout.take().unwrap()), capture(child.stderr.take().unwrap()), child.wait());
    Ok(format!("exit: {}\nstdout:\n{}\nstderr:\n{}", status?.code().unwrap_or(-1), out?, err?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn console_reports_output_and_exit() {
        let result = console("printf hello; printf problem >&2; exit 7").await.unwrap();
        assert!(result.contains("exit: 7"));
        assert!(result.contains("stdout:\nhello"));
        assert!(result.contains("stderr:\nproblem"));
    }
    #[tokio::test]
    async fn capture_drains_large_output() {
        let bytes = vec![b'x'; 100_000];
        let result = capture(bytes.as_slice()).await.unwrap();
        assert!(result.starts_with(&"x".repeat(32_768)));
        assert!(result.ends_with("[output truncated]"));
    }
}
