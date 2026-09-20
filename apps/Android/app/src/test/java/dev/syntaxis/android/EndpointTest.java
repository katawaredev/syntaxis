package dev.syntaxis.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class EndpointTest {
    @Test public void shellNavigationOnlyAcceptsAppRoutes() {
        assertTrue(Endpoint.isAppPath("/"));
        assertTrue(Endpoint.isAppPath("/new-project"));
        assertTrue(Endpoint.isAppPath("/clone-project"));
        assertTrue(Endpoint.isAppPath("/workspaces/my-project/files"));
        assertTrue(Endpoint.isAppPath("/workspaces/my-project/terminal?session=abc"));
        for (String path : new String[]{"//evil.example", "/login", "/auth/logout", "/workspaces/../files", "/workspaces/%2f/files", "https://evil.example/", "/assets/page.html"}) {
            assertFalse(path, Endpoint.isAppPath(path));
        }
    }

    @Test public void normalizesRemoteOrigins() {
        assertEquals("https://code.example.com", Endpoint.remote(" https://CODE.example.com:443/ "));
        assertEquals("https://code.example.com:8443", Endpoint.remote("https://code.example.com:8443"));
    }

    @Test public void rejectsUntrustedOrAmbiguousConnectionInputs() {
        for (String input : new String[]{"http://example.com", "javascript:alert(1)",
                "https://user:secret@example.com", "https://example.com/path", "https://example.com?token=secret",
                "https://example.com#fragment", "https://localhost", "https://127.0.0.1:8787",
                "https://[::1]", "https://example.com:0", "https://example.com:65536", "not a URL"}) {
            assertThrows(input, IllegalArgumentException.class, () -> Endpoint.remote(input));
        }
    }

    @Test public void navigationCannotCrossOriginsOrPorts() {
        assertTrue(Endpoint.contains(Endpoint.LOCAL, "http://127.0.0.1:8787/workspaces/project/files"));
        assertFalse(Endpoint.contains(Endpoint.LOCAL, "http://127.0.0.1:9999/"));
        assertFalse(Endpoint.contains(Endpoint.LOCAL, "http://127.0.0.1.evil.example:8787/"));
        assertFalse(Endpoint.contains("https://example.com", "https://user@example.com/"));
        assertFalse(Endpoint.contains("https://example.com", "http://example.com/"));
        assertFalse(Endpoint.contains("https://example.com", "intent://example.com"));
        assertTrue(Endpoint.contains("https://example.com", "https://example.com:443/login"));
    }
}
