(() => {
  const NativeWebSocket = window.WebSocket;

  function CompatibleWebSocket(url, protocols) {
    if (!new.target) {
      throw new TypeError(
        "Failed to construct 'WebSocket': Please use the 'new' operator.",
      );
    }

    if (typeof url === "string" && url.startsWith("/")) {
      const scheme = window.location.protocol === "https:" ? "wss:" : "ws:";

      url = `${scheme}//${window.location.host}${url}`;
    }

    return protocols === undefined
      ? new NativeWebSocket(url)
      : new NativeWebSocket(url, protocols);
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
