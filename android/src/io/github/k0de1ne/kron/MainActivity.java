package io.github.k0de1ne.kron;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceError;

public class MainActivity extends Activity {
    static final int PORT = 7790;
    static final String UI = "http://127.0.0.1:" + PORT + "/";
    private int retries;
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        if (android.os.Build.VERSION.SDK_INT >= 33)
            requestPermissions(new String[]{android.Manifest.permission.POST_NOTIFICATIONS}, 1);
        startForegroundService(new Intent(this, KronService.class));
        WebView web = new WebView(this);
        web.getSettings().setJavaScriptEnabled(true);
        web.setWebViewClient(new WebViewClient() {
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                return !UI.equals(request.getUrl().toString());
            }
            @Override public void onReceivedError(WebView view, WebResourceRequest request, WebResourceError error) {
                if (!request.isForMainFrame()) return;
                if (++retries < 60) view.postDelayed(() -> view.loadUrl(UI), 1000);
                else {
                    String log;
                    try { log = new String(java.nio.file.Files.readAllBytes(KronService.logFile(MainActivity.this).toPath()), java.nio.charset.StandardCharsets.UTF_8); }
                    catch (Exception e) { log = e.toString(); }
                    view.loadData("<meta name=viewport content='width=device-width'><pre>" + android.text.Html.escapeHtml(log) + "</pre>", "text/html", "UTF-8");
                }
            }
        });
        setContentView(web);
        web.loadUrl(UI);
    }
}
