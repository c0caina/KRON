#!/system/bin/sh
set -eu
umask 022
export KRON_RUNTIME=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export KRON_CONFIG="${KRON_CONFIG:-$KRON_RUNTIME/../kron.json}"
export KRON_LISTEN="${KRON_LISTEN:-127.0.0.1:7791}"
export LD_LIBRARY_PATH="$KRON_RUNTIME/llama${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
printf '%s\n' "$$" > "$KRON_RUNTIME/pid"
cd "$KRON_RUNTIME/.."
model_pid= agent_pid=
trap 'trap - EXIT; kill ${agent_pid:-} ${model_pid:-} 2>/dev/null || true' EXIT
trap 'exit 0' TERM INT
"$KRON_RUNTIME/llama/llama-server" -m "$KRON_RUNTIME/ignition.gguf" --alias ignition --host 127.0.0.1 --port 7792 --jinja -c 8192 -t 4 --parallel 1 --temp 0.1 --top-k 50 --repeat-penalty 1.1 --no-webui > "$KRON_RUNTIME/model.log" 2>&1 &
model_pid=$!
"$KRON_RUNTIME/kron" &
agent_pid=$!
wait "$agent_pid"
