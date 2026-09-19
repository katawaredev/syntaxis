package dev.syntaxis.android;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.net.Uri;
import android.net.http.SslError;
import android.os.Build;
import android.os.Bundle;
import android.text.InputType;
import android.view.View;
import android.view.WindowInsets;
import android.webkit.CookieManager;
import android.webkit.RenderProcessGoneDetail;
import android.webkit.SslErrorHandler;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceError;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;
import androidx.webkit.JavaScriptReplyProxy;
import androidx.webkit.WebViewCompat;
import androidx.webkit.WebViewFeature;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.security.SecureRandom;
import java.util.HashSet;
import java.util.Set;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Native connection coordinator; project UI belongs to the shared app. */
public final class MainActivity extends Activity {
    private static final String TERMUX_PERMISSION = "com.termux.permission.RUN_COMMAND";
    private static final int TERMUX_REQUEST = 2, FILE_REQUEST = 3;
    private final ExecutorService network = Executors.newFixedThreadPool(3);
    private SharedPreferences preferences;
    private LinearLayout root, content;
    private ProgressBar progress;
    private WebView web;
    private String remote = "", origin = Endpoint.LOCAL, appToken;
    private boolean ready, checking, awaitingTermux, resetHistory, configuringRemote;
    private int connectionGeneration;
    private ValueCallback<Uri[]> fileCallback;

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        preferences = getSharedPreferences("connections", MODE_PRIVATE);
        remote = preferences.getString("remote", "");
        appToken = preferences.getString("local_token", null);
        if (appToken == null) {
            byte[] bytes = new byte[32];
            new SecureRandom().nextBytes(bytes);
            StringBuilder token = new StringBuilder();
            for (byte value : bytes) token.append(String.format("%02x", value & 0xff));
            appToken = token.toString();
            preferences.edit().putString("local_token", appToken).apply();
        }
        root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(Color.rgb(31, 32, 33));
        root.setOnApplyWindowInsetsListener((view, insets) -> {
            if (Build.VERSION.SDK_INT >= 30) {
                android.graphics.Insets padding = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout() | WindowInsets.Type.ime());
                view.setPadding(padding.left, padding.top, padding.right, padding.bottom);
            } else view.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(), insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            return Build.VERSION.SDK_INT >= 30 ? WindowInsets.CONSUMED : insets.consumeSystemWindowInsets();
        });
        progress = new ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal);
        root.addView(progress, new LinearLayout.LayoutParams(-1, dp(2)));
        content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        root.addView(content, new LinearLayout.LayoutParams(-1, 0, 1));
        setContentView(root);
        if (Build.VERSION.SDK_INT >= 33) getOnBackInvokedDispatcher().registerOnBackInvokedCallback(0, this::back);
        showMessage("Starting Syntaxis", "Checking the local Termux runtime…");
        checkLocal();
    }

    private int dp(int value) { return Math.round(value * getResources().getDisplayMetrics().density); }
    private void toast(String text) { Toast.makeText(this, text, Toast.LENGTH_LONG).show(); }
    private Button button(String title, Runnable action) {
        Button button = new Button(this);
        button.setText(title);
        button.setAllCaps(false);
        button.setOnClickListener(view -> action.run());
        return button;
    }
    private TextView text(String value, int size) {
        TextView text = new TextView(this);
        text.setText(value);
        text.setTextColor(Color.rgb(225, 225, 225));
        text.setTextSize(size);
        text.setPadding(0, dp(8), 0, dp(12));
        return text;
    }
    private LinearLayout showMessage(String title, String message) {
        configuringRemote = false;
        connectionGeneration++;
        destroyWeb();
        content.removeAllViews();
        progress.setVisibility(View.GONE);
        ScrollView scroll = new ScrollView(this);
        LinearLayout panel = new LinearLayout(this);
        panel.setOrientation(LinearLayout.VERTICAL);
        panel.setPadding(dp(24), dp(48), dp(24), dp(24));
        panel.addView(text(title, 28));
        panel.addView(text(message, 16));
        scroll.addView(panel);
        content.addView(scroll, new LinearLayout.LayoutParams(-1, -1));
        return panel;
    }

    private void checkLocal() {
        if (checking || isFinishing()) return;
        checking = true;
        network.execute(() -> {
            String error = null;
            try {
                BackendClient.Response response = BackendClient.request(Endpoint.LOCAL, "/auth/android-session", "", appToken);
                if (response.status != 204) error = "The backend needs to be updated or paired with this app. Stop it with Ctrl+C in Termux, then tap Start and pair.";
            } catch (Exception exception) { error = "The local backend is not running. Complete the Termux setup, then start it below."; }
            final String failure = error;
            runOnUiThread(() -> {
                checking = false;
                if (isFinishing() || isDestroyed()) return;
                ready = failure == null;
                if (!ready) showSetup(failure);
                else if (!preferences.getBoolean("onboarded", false)) showWelcome(true);
                else if ((web == null && !configuringRemote) || awaitingTermux) navigate(false, "/");
                awaitingTermux = false;
            });
        });
    }

    private void showSetup(String detail) {
        LinearLayout panel = showMessage("Set up Termux to continue", detail + "\n\n1. Install Termux from F-Droid or its official GitHub releases.\n2. In Termux, run pkg update and termux-setup-storage.\n3. Copy the Syntaxis installer, matching backend archive, and checksum to Downloads.\n4. Run bash ~/storage/downloads/install-syntaxis.sh --no-start\n5. Return here and tap Start and pair.\n\nTermux is required even when working with remote projects.");
        panel.addView(button("Start and pair", this::startTermux));
        panel.addView(button("Check again", this::checkLocal));
        panel.addView(button("Open Termux", this::openTermux));
    }

    private void showWelcome(boolean firstRun) {
        if (!ready) { checkLocal(); return; }
        LinearLayout panel = showMessage(firstRun ? "Welcome to Syntaxis" : "Remote connection", "Your local projects are ready. Add a remote Syntaxis server to see its projects alongside them, or continue locally.");
        configuringRemote = true;
        final int generation = connectionGeneration;
        panel.addView(text("Remote server URL", 14));
        EditText address = new EditText(this);
        address.setSingleLine(true);
        address.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI);
        address.setHint("https://code.example.com");
        address.setText(remote);
        panel.addView(address);
        panel.addView(text("Server password", 14));
        EditText password = new EditText(this);
        password.setSingleLine(true);
        password.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD);
        panel.addView(password);
        TextView feedback = text("", 14);
        panel.addView(feedback);
        Button connect = button("Connect", () -> {});
        connect.setOnClickListener(view -> {
            final String candidate;
            try { candidate = Endpoint.remote(address.getText().toString()); }
            catch (IllegalArgumentException error) { feedback.setText(error.getMessage()); return; }
            String value = password.getText().toString();
            if (value.isEmpty()) { feedback.setText("Enter the server password."); return; }
            connect.setEnabled(false);
            feedback.setText("Connecting…");
            network.execute(() -> {
                String error = null;
                try {
                    BackendClient.Response response = BackendClient.request(candidate, "/login", "password=" + URLEncoder.encode(value, StandardCharsets.UTF_8.name()), null);
                    if (response.status == 401) throw new IllegalArgumentException("Incorrect password.");
                    if (response.status == 429) throw new IllegalArgumentException("Too many attempts. Try again in a few minutes.");
                    if (response.status != 303) throw new IllegalArgumentException("The server did not accept the login (HTTP " + response.status + ").");
                    BackendClient.Response projects = BackendClient.request(candidate, "/api/workspaces", null, null);
                    if (projects.status != 200) throw new IllegalArgumentException("Could not load this server's projects.");
                    new JSONArray(projects.body);
                } catch (Exception exception) { error = exception.getMessage(); }
                final String failure = error;
                runOnUiThread(() -> {
                    if (isFinishing() || isDestroyed() || generation != connectionGeneration) return;
                    connect.setEnabled(true);
                    if (failure != null) { feedback.setText(failure); return; }
                    password.setText("");
                    remote = candidate;
                    preferences.edit().putString("remote", remote).putBoolean("onboarded", true).apply();
                    CookieManager.getInstance().flush();
                    navigate(false, "/");
                });
            });
        });
        panel.addView(connect);
        panel.addView(button(firstRun ? "Skip — use local projects" : "Back to projects", () -> {
            preferences.edit().putBoolean("onboarded", true).apply();
            navigate(false, "/");
        }));
        if (!firstRun && !remote.isEmpty()) panel.addView(button("Remove remote", () -> {
            // Removing a connection never removes that server's projects.
            remote = "";
            preferences.edit().remove("remote").apply();
            navigate(false, "/");
        }));
    }

    private void navigate(boolean useRemote, String path) {
        if (!ready) { checkLocal(); return; }
        if (useRemote && remote.isEmpty()) { showWelcome(false); return; }
        configuringRemote = false;
        connectionGeneration++;
        origin = useRemote ? remote : Endpoint.LOCAL;
        if (web == null) createWeb();
        if (web == null) return;
        web.stopLoading();
        resetHistory = true;
        web.loadUrl(origin + path);
    }

    @SuppressLint("SetJavaScriptEnabled")
    private void createWeb() {
        content.removeAllViews();
        web = new WebView(this);
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            showMessage("Update Android System WebView", "Syntaxis needs a WebView with secure messaging support. Update Android System WebView before continuing.");
            return;
        }
        WebSettings settings = web.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setDomStorageEnabled(true);
        settings.setAllowFileAccess(false);
        settings.setAllowContentAccess(false);
        settings.setMixedContentMode(WebSettings.MIXED_CONTENT_NEVER_ALLOW);
        CookieManager.getInstance().setAcceptCookie(true);
        CookieManager.getInstance().setAcceptThirdPartyCookies(web, false);
        Set<String> origins = new HashSet<>();
        origins.add(Endpoint.LOCAL);
        if (!remote.isEmpty()) origins.add(remote);
        WebViewCompat.addWebMessageListener(web, "SyntaxisAndroid", origins, (view, message, source, mainFrame, reply) -> {
            if (!mainFrame || view != web || !Endpoint.contains(origin, source.toString())) return;
            String path = Uri.parse(view.getUrl() == null ? "" : view.getUrl()).getPath();
            if (!("/".equals(path) || "/new-project".equals(path) || "/clone-project".equals(path))) return;
            try { bridge(new JSONObject(message.getData()), reply, path); }
            catch (Exception error) { toast("Could not read the app connection request."); }
        });
        web.setWebViewClient(new WebViewClient() {
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                if (!request.isForMainFrame()) return false;
                String url = request.getUrl().toString();
                if (Endpoint.contains(origin, url)) {
                    if (!origin.equals(Endpoint.LOCAL) && "/".equals(request.getUrl().getPath())) { navigate(false, "/"); return true; }
                    return false;
                }
                if (request.hasGesture()) openExternal(request.getUrl());
                return true;
            }
            @Override public void onPageFinished(WebView view, String url) {
                if (view != web || !Endpoint.contains(origin, url)) return;
                if (resetHistory) { view.clearHistory(); resetHistory = false; }
                CookieManager.getInstance().flush();
                if ("/login".equals(Uri.parse(url).getPath())) {
                    if (origin.equals(Endpoint.LOCAL)) { ready = false; checkLocal(); }
                    else showWelcome(false);
                }
            }
            @Override public void onReceivedError(WebView view, WebResourceRequest request, WebResourceError error) {
                if (view == web && request.isForMainFrame() && Endpoint.contains(origin, request.getUrl().toString())) failure("Connection interrupted", error.getDescription().toString());
            }
            @Override public void onReceivedHttpError(WebView view, WebResourceRequest request, WebResourceResponse response) {
                if (view == web && request.isForMainFrame() && Endpoint.contains(origin, request.getUrl().toString())) failure("Could not open project", "The server returned HTTP " + response.getStatusCode() + ".");
            }
            @Override public void onReceivedSslError(WebView view, SslErrorHandler handler, SslError error) {
                handler.cancel();
                if (view == web) failure("Connection rejected", "The server's HTTPS certificate could not be verified.");
            }
            @Override public boolean onRenderProcessGone(WebView view, RenderProcessGoneDetail detail) {
                if (view == web) failure("Page closed by Android", "Return to your projects to reopen it. The backend may still be running.");
                return true;
            }
        });
        web.setWebChromeClient(new WebChromeClient() {
            @Override public void onProgressChanged(WebView view, int value) {
                if (view != web) return;
                progress.setProgress(value);
                progress.setVisibility(value == 100 ? View.GONE : View.VISIBLE);
            }
            @Override public boolean onShowFileChooser(WebView view, ValueCallback<Uri[]> callback, FileChooserParams params) {
                cancelFileChooser();
                fileCallback = callback;
                try { startActivityForResult(params.createIntent(), FILE_REQUEST); }
                catch (ActivityNotFoundException error) { cancelFileChooser(); toast("No file picker is installed."); }
                return true;
            }
        });
        web.setDownloadListener((url, userAgent, disposition, mime, length) -> {
            if (url.startsWith("https://") || url.startsWith("http://")) openExternal(Uri.parse(url));
            else toast("Open this project in a browser to download exports.");
        });
        content.addView(web, new LinearLayout.LayoutParams(-1, -1));
    }

    private void bridge(JSONObject request, JavaScriptReplyProxy reply, String page) throws JSONException {
        String id = request.getString("id");
        switch (request.getString("action")) {
            case "configure":
                respond(reply, id, JSONObject.NULL, null);
                showWelcome(false);
                break;
            case "open":
                String path = request.getString("path");
                if (!Endpoint.isAppPath(path)) { respond(reply, id, null, "Invalid project path."); return; }
                boolean useRemote = request.getBoolean("remote");
                respond(reply, id, JSONObject.NULL, null);
                navigate(useRemote, path);
                break;
            case "state":
                boolean currentRemote = !origin.equals(Endpoint.LOCAL);
                if (currentRemote && "/".equals(page)) {
                    respond(reply, id, JSONObject.NULL, null);
                    navigate(false, "/");
                    return;
                }
                final String server = remote;
                final boolean projects = request.optBoolean("projects", false);
                network.execute(() -> {
                    JSONObject result = new JSONObject();
                    try {
                        result.put("remoteConfigured", !server.isEmpty());
                        result.put("currentRemote", currentRemote);
                        result.put("projects", new JSONArray());
                        result.put("error", JSONObject.NULL);
                        if (projects && !server.isEmpty()) {
                            try {
                                BackendClient.Response response = BackendClient.request(server, "/api/workspaces", null, null);
                                if (response.status != 200) throw new IllegalArgumentException(response.status == 401 || response.status == 303 ? "Remote login expired. Reconnect in Remote settings." : "Remote projects are unavailable. Local projects are ready.");
                                result.put("projects", new JSONArray(response.body));
                            } catch (Exception error) { result.put("error", "Remote projects are unavailable. Use Remote settings to reconnect; local projects are ready."); }
                        }
                        runOnUiThread(() -> respond(reply, id, result, null));
                    } catch (JSONException error) { runOnUiThread(() -> respond(reply, id, null, "Could not load connections.")); }
                });
                break;
            default: respond(reply, id, null, "Unsupported app request.");
        }
    }
    private void respond(JavaScriptReplyProxy reply, String id, Object result, String error) {
        if (isFinishing() || isDestroyed()) return;
        try {
            JSONObject response = new JSONObject();
            response.put("id", id);
            response.put("result", result == null ? JSONObject.NULL : result);
            response.put("error", error == null ? JSONObject.NULL : error);
            reply.postMessage(response.toString());
        } catch (JSONException | IllegalStateException ignored) { /* The requesting page may have closed. */ }
    }
    private void failure(String title, String detail) {
        LinearLayout panel = showMessage(title, detail);
        panel.addView(button("Back to projects", this::checkLocal));
    }
    private void startTermux() {
        if (getPackageManager().getLaunchIntentForPackage("com.termux") == null) { toast("Install Termux first."); return; }
        if (checkSelfPermission(TERMUX_PERMISSION) != PackageManager.PERMISSION_GRANTED) { requestPermissions(new String[]{TERMUX_PERMISSION}, TERMUX_REQUEST); return; }
        Intent intent = new Intent("com.termux.RUN_COMMAND");
        intent.setClassName("com.termux", "com.termux.app.RunCommandService");
        intent.putExtra("com.termux.RUN_COMMAND_PATH", "/data/data/com.termux/files/usr/bin/bash");
        intent.putExtra("com.termux.RUN_COMMAND_ARGUMENTS", new String[]{"/data/data/com.termux/files/home/.local/share/syntaxis/start.sh", "--app-token", appToken});
        intent.putExtra("com.termux.RUN_COMMAND_WORKDIR", "/data/data/com.termux/files/home");
        intent.putExtra("com.termux.RUN_COMMAND_BACKGROUND", false);
        intent.putExtra("com.termux.RUN_COMMAND_SESSION_ACTION", "0");
        try { startService(intent); awaitingTermux = true; toast("Return to Syntaxis once the backend is running."); }
        catch (RuntimeException error) { toast("Enable allow-external-apps=true in Termux settings, then retry."); }
    }
    private void openTermux() {
        Intent intent = getPackageManager().getLaunchIntentForPackage("com.termux");
        if (intent == null) toast("Install Termux first."); else startActivity(intent);
    }
    private void openExternal(Uri uri) {
        if (!"https".equals(uri.getScheme()) && !"http".equals(uri.getScheme())) return;
        try { startActivity(new Intent(Intent.ACTION_VIEW, uri)); }
        catch (ActivityNotFoundException error) { toast("No browser is installed."); }
    }
    private void cancelFileChooser() { if (fileCallback != null) { fileCallback.onReceiveValue(null); fileCallback = null; } }
    private void destroyWeb() {
        cancelFileChooser();
        if (web != null) { WebView old = web; web = null; content.removeView(old); old.stopLoading(); old.destroy(); }
    }
    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == FILE_REQUEST && fileCallback != null) {
            fileCallback.onReceiveValue(WebChromeClient.FileChooserParams.parseResult(result, data));
            fileCallback = null;
        }
    }
    @Override public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(request, permissions, results);
        if (request == TERMUX_REQUEST && results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED) startTermux();
        else if (request == TERMUX_REQUEST) toast("Grant Syntaxis the Run commands permission to pair with Termux.");
    }
    private void back() {
        if (web != null && web.canGoBack()) web.goBack();
        else if (web != null && !Endpoint.LOCAL.concat("/").equals(web.getUrl())) navigate(false, "/");
        else finish();
    }
    @SuppressLint("GestureBackNavigation")
    @Override public void onBackPressed() { back(); }
    @Override protected void onResume() { super.onResume(); if (preferences != null && (ready || awaitingTermux)) checkLocal(); }
    @Override protected void onPause() { CookieManager.getInstance().flush(); super.onPause(); }
    @Override protected void onDestroy() { destroyWeb(); network.shutdownNow(); super.onDestroy(); }
}
