package dev.syntaxis.android;

import java.net.URI;
import java.net.URISyntaxException;
import java.util.Locale;

/** Connections are origins, never paths, credentials, or arbitrary intent URLs. */
final class Endpoint {
    static final String LOCAL = "http://127.0.0.1:8787";

    static String remote(String input) {
        try {
            URI uri = new URI(input.trim());
            String host = uri.getHost();
            if (!"https".equalsIgnoreCase(uri.getScheme()) || host == null
                    || uri.getRawUserInfo() != null || uri.getRawQuery() != null
                    || uri.getRawFragment() != null
                    || !(uri.getRawPath().isEmpty() || "/".equals(uri.getRawPath()))
                    || uri.getPort() == 0 || uri.getPort() > 65535
                    || host.equalsIgnoreCase("localhost") || host.startsWith("127.")
                    || host.equals("[::1]")) {
                throw new IllegalArgumentException("Use an HTTPS server origin, e.g. https://code.example.com (no path or credentials).");
            }
            return new URI("https", null, host.toLowerCase(Locale.ROOT),
                    uri.getPort() == 443 ? -1 : uri.getPort(), null, null, null).toString();
        } catch (URISyntaxException error) {
            throw new IllegalArgumentException("Enter a valid HTTPS server address.");
        }
    }

    static boolean contains(String origin, String address) {
        try {
            URI expected = new URI(origin);
            URI actual = new URI(address);
            return expected.getScheme().equalsIgnoreCase(actual.getScheme())
                    && expected.getHost().equalsIgnoreCase(actual.getHost())
                    && port(expected) == port(actual) && actual.getRawUserInfo() == null;
        } catch (URISyntaxException | NullPointerException error) {
            return false;
        }
    }

    static boolean isAppPath(String path) {
        if (path == null || path.contains("\\") || path.contains("..") || path.contains("%") || path.contains("#")) return false;
        return path.equals("/") || path.equals("/new-project") || path.equals("/clone-project")
                || path.matches("/workspaces/[A-Za-z0-9_-]+/(files|terminal|git|preview|ai)(\\?.*)?");
    }

    private static int port(URI uri) {
        return uri.getPort() == -1 ? ("https".equalsIgnoreCase(uri.getScheme()) ? 443 : 80) : uri.getPort();
    }

    private Endpoint() {}
}
