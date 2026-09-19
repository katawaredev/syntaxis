package dev.syntaxis.android;

import android.webkit.CookieManager;
import android.os.Handler;
import android.os.Looper;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Map;

/** Fixed-origin native requests. Never follows redirects with credentials. */
final class BackendClient {
    static final class Response {
        final int status;
        final String body;
        Response(int status, String body) { this.status = status; this.body = body; }
    }

    static Response request(String origin, String path, String body, String bearer) throws IOException {
        HttpURLConnection connection = (HttpURLConnection) URI.create(origin + path).toURL().openConnection();
        try {
            connection.setInstanceFollowRedirects(false);
            connection.setConnectTimeout(5000);
            connection.setReadTimeout(10000);
            connection.setRequestProperty("Accept", "application/json");
            String cookie = CookieManager.getInstance().getCookie(origin);
            if (cookie != null) connection.setRequestProperty("Cookie", cookie);
            if (bearer != null) connection.setRequestProperty("Authorization", "Bearer " + bearer);
            if (body != null) {
                connection.setRequestMethod("POST");
                connection.setRequestProperty("Origin", origin);
                connection.setRequestProperty("Content-Type", "application/x-www-form-urlencoded");
                connection.setDoOutput(true);
                try (java.io.OutputStream output = connection.getOutputStream()) {
                    output.write(body.getBytes(StandardCharsets.UTF_8));
                }
            }
            int status = connection.getResponseCode();
            for (Map.Entry<String, List<String>> header : connection.getHeaderFields().entrySet()) {
                if ("Set-Cookie".equalsIgnoreCase(header.getKey())) {
                    for (String value : header.getValue()) {
                        CountDownLatch stored = new CountDownLatch(1);
                        new Handler(Looper.getMainLooper()).post(() -> CookieManager.getInstance().setCookie(origin, value, accepted -> stored.countDown()));
                        try { if (!stored.await(5, TimeUnit.SECONDS)) throw new IOException("Could not store the login session."); }
                        catch (InterruptedException interrupted) { Thread.currentThread().interrupt(); throw new IOException("Login cancelled.", interrupted); }
                    }
                }
            }
            InputStream stream = status >= 400 ? connection.getErrorStream() : connection.getInputStream();
            if (stream == null) return new Response(status, "");
            try (InputStream input = stream; ByteArrayOutputStream output = new ByteArrayOutputStream()) {
                byte[] buffer = new byte[8192];
                int length;
                while ((length = input.read(buffer)) != -1) {
                    if (output.size() + length > 2 * 1024 * 1024) throw new IOException("Server response is too large.");
                    output.write(buffer, 0, length);
                }
                return new Response(status, output.toString(StandardCharsets.UTF_8.name()));
            }
        } finally { connection.disconnect(); }
    }

    private BackendClient() {}
}
