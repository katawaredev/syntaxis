(() => {
  const NativeWebSocket = window.WebSocket;

  const openSocket = (url, protocols) =>
    protocols === undefined ? new NativeWebSocket(url) : new NativeWebSocket(url, protocols);

  function CompatibleWebSocket(url, protocols) {
    if (!new.target) {
      throw new TypeError("Failed to construct 'WebSocket': Please use the 'new' operator.");
    }

    try {
      return openSocket(url, protocols);
    } catch (error) {
      const isUnsupportedRelativeUrl =
        error instanceof DOMException &&
        error.name === "SyntaxError" &&
        typeof url === "string" &&
        url.startsWith("/");

      if (!isUnsupportedRelativeUrl) throw error;

      const scheme = window.location.protocol === "https:" ? "wss:" : "ws:";
      return openSocket(`${scheme}//${window.location.host}${url}`, protocols);
    }
  }

  CompatibleWebSocket.prototype = NativeWebSocket.prototype;
  for (const state of ["CONNECTING", "OPEN", "CLOSING", "CLOSED"]) {
    Object.defineProperty(CompatibleWebSocket, state, {
      value: NativeWebSocket[state],
      enumerable: true,
    });
  }

  window.WebSocket = CompatibleWebSocket;
})();
