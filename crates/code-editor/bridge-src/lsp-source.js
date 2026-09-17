import {
  findReferences,
  formatDocument,
  jumpToDefinition,
  LSPClient,
  LSPPlugin,
  serverCompletion,
  serverDiagnostics,
} from "@codemirror/lsp-client";

const REGISTRY_KEY = Symbol.for("syntaxis.codeMirror.languageServices");
const IDLE_DISCONNECT_MS = 2 * 60 * 1000;
const MAX_DIAGNOSTICS = 200;

const registry = () => {
  if (!(globalThis[REGISTRY_KEY] instanceof Map)) {
    globalThis[REGISTRY_KEY] = new Map();
  }
  return globalThis[REGISTRY_KEY];
};

const sanitizeHTML = (html) => {
  const template = document.createElement("template");
  template.innerHTML = html;
  for (const element of template.content.querySelectorAll(
    "script,style,iframe,object,embed,link,meta,form,input,button",
  )) {
    element.remove();
  }
  for (const element of template.content.querySelectorAll("*")) {
    for (const attribute of Array.from(element.attributes)) {
      const name = attribute.name.toLowerCase();
      if (name.startsWith("on") || name === "style" || name === "srcdoc") {
        element.removeAttribute(attribute.name);
      } else if (name === "href" || name === "src") {
        try {
          const url = new URL(attribute.value, globalThis.location.href);
          if (!["http:", "https:", "mailto:"].includes(url.protocol)) {
            element.removeAttribute(attribute.name);
          }
        } catch {
          element.removeAttribute(attribute.name);
        }
      }
    }
  }
  return template.innerHTML;
};

const socketUrl = (endpoint) => {
  const url = new URL(endpoint, globalThis.location.href);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url.href;
};

const openTransport = (endpoint) =>
  new Promise((resolve, reject) => {
    const handlers = new Set();
    const socket = new WebSocket(socketUrl(endpoint));
    socket.addEventListener(
      "open",
      () => {
        resolve({
          socket,
          transport: {
            send(message) {
              if (socket.readyState !== WebSocket.OPEN) {
                throw new Error("The language-service connection is closed");
              }
              socket.send(message);
            },
            subscribe(handler) {
              handlers.add(handler);
            },
            unsubscribe(handler) {
              handlers.delete(handler);
            },
          },
          handlers,
        });
      },
      { once: true },
    );
    socket.addEventListener("message", (event) => {
      if (typeof event.data !== "string") return;
      for (const handler of handlers) handler(event.data);
    });
    socket.addEventListener(
      "error",
      () => reject(new Error("Could not connect to the language server")),
      { once: true },
    );
  });

const createSession = async (config) => {
  const connection = await openTransport(config.endpoint);
  const session = {
    client: null,
    socket: connection.socket,
    refs: 0,
    idleTimer: null,
    closed: false,
    status: "starting",
    message: "",
    listeners: new Set(),
  };
  const capabilities = () => {
    const available = client.serverCapabilities ?? {};
    return {
      completion: Boolean(available.completionProvider),
      definition: Boolean(available.definitionProvider),
      references: Boolean(available.referencesProvider),
      formatting: Boolean(available.documentFormattingProvider),
    };
  };
  const updateStatus = (status, message = "") => {
    session.status = status;
    session.message = message;
    for (const listener of session.listeners) listener(status, message, capabilities());
  };
  const client = new LSPClient({
    rootUri: config.rootUri,
    timeout: 3000,
    sanitizeHTML,
    notificationHandlers: {
      "textDocument/publishDiagnostics": (_client, params) => {
        if (Array.isArray(params?.diagnostics) && params.diagnostics.length > MAX_DIAGNOSTICS) {
          params.diagnostics = params.diagnostics.slice(0, MAX_DIAGNOSTICS);
        }
        return false;
      },
    },
    extensions: [serverCompletion(), serverDiagnostics()],
  });
  session.client = client;
  connection.socket.addEventListener("close", () => {
    session.closed = true;
    client.disconnect();
    if (registry().get(config.sessionKey) === session) {
      registry().delete(config.sessionKey);
    }
    updateStatus("unavailable", "Language-service connection closed");
  });
  client.connect(connection.transport);
  client.initializing.then(
    () => updateStatus("ready"),
    (error) => {
      updateStatus("unavailable", error instanceof Error ? error.message : String(error));
      connection.socket.close();
    },
  );
  return session;
};

const acquireSession = async (config) => {
  let session = registry().get(config.sessionKey);
  if (!session || session.closed) {
    session = await createSession(config);
    if (session.closed) throw new Error(session.message || "Language-service connection closed");
    registry().set(config.sessionKey, session);
  }
  if (session.idleTimer != null) {
    clearTimeout(session.idleTimer);
    session.idleTimer = null;
  }
  session.refs += 1;
  session.listeners.add(config.onStatus);
  config.onStatus(session.status, session.message, {
    completion: Boolean(session.client.serverCapabilities?.completionProvider),
    definition: Boolean(session.client.serverCapabilities?.definitionProvider),
    references: Boolean(session.client.serverCapabilities?.referencesProvider),
    formatting: Boolean(session.client.serverCapabilities?.documentFormattingProvider),
  });
  return session;
};

const releaseSession = (sessionKey, session, listener) => {
  session.listeners.delete(listener);
  session.refs = Math.max(0, session.refs - 1);
  if (session.refs !== 0 || session.closed || session.idleTimer != null) return;
  session.idleTimer = setTimeout(() => {
    session.idleTimer = null;
    if (session.refs !== 0 || session.closed) return;
    session.closed = true;
    session.client.disconnect();
    session.socket.close(1000, "Idle");
    if (registry().get(sessionKey) === session) registry().delete(sessionKey);
  }, IDLE_DISCONNECT_MS);
};

const documentUri = (rootUri, filename) => {
  const relative = filename
    .split("/")
    .filter((segment) => segment && segment !== ".")
    .map(encodeURIComponent)
    .join("/");
  return new URL(relative, rootUri).href;
};

export const connectLanguageService = async (config) => {
  const session = await acquireSession(config);
  return {
    extension: session.client.plugin(
      documentUri(config.rootUri, config.filename),
      config.languageId,
    ),
    release() {
      releaseSession(config.sessionKey, session, config.onStatus);
    },
  };
};

export const runLanguageServiceAction = (action, view) => {
  switch (action) {
    case "go_to_definition":
      return jumpToDefinition(view);
    case "find_references":
      return findReferences(view);
    case "format_document":
      return LSPPlugin.get(view)?.client.hasCapability("documentFormattingProvider") === false
        ? false
        : formatDocument(view);
    default:
      return false;
  }
};
