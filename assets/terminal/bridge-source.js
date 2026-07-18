import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import { parseSourceLocations } from "./source-links.js";

const instances = new Map();

function themeColor(container, property, fallback) {
  return getComputedStyle(container).getPropertyValue(property).trim() || fallback;
}
function emit(kind, id, detail = {}) {
  window.dispatchEvent(
    new CustomEvent("syntaxis-terminal", {
      detail: { kind, id, ...detail },
    }),
  );
}

function sourceLinks(term, id, row) {
  const line = term.buffer.active.getLine(row - 1);
  if (!line) return [];
  const text = line.translateToString(true);
  const links = [];
  for (const location of parseSourceLocations(text)) {
    links.push({
      range: {
        start: { x: location.start + 1, y: row },
        end: { x: location.start + location.text.length, y: row },
      },
      text: location.text,
      decorations: {
        pointerCursor: true,
        underline: true,
      },
      activate(event) {
        event.preventDefault();
        event.stopPropagation();
        emit("source_link", id, {
          path: location.path,
          line: location.line,
          column: location.column,
          end_line: location.endLine,
          end_column: location.endColumn,
        });
      },
    });
  }
  return links;
}

async function mount(id) {
  dispose(id);

  const container = document.getElementById(id);
  if (!container) throw new Error(`Terminal container ${id} was not found`);

  for (const [instanceId, instance] of instances) {
    if (instance.container === container || !instance.container.isConnected) {
      dispose(instanceId);
    }
  }
  container.replaceChildren();

  const background = themeColor(container, "--card", "#272727");
  const foreground = themeColor(container, "--foreground", "#d4d4d4");
  const term = new Terminal({
    allowTransparency: false,
    cursorBlink: true,
    fontFamily: "Menlo, Consolas, 'DejaVu Sans Mono', 'Courier New', monospace",
    fontSize: 14,
    lineHeight: 1.2,
    scrollback: 5000,
    theme: {
      // Keep the terminal canvas identical to the editor surface. These resolve
      // to the same --card/--foreground tokens used by dioxus-code-editor.css.
      background,
      foreground,
      cursor: foreground,
      selectionBackground: "#569cd64d",
    },
  });
  const fitAddon = new FitAddon();
  term.loadAddon(fitAddon);
  term.open(container);
  const dataSubscription = term.onData(data => emit("input", id, { data }));
  const resizeSubscription = term.onResize(({ cols, rows }) => {
    const rect = container.getBoundingClientRect();
    emit("resize", id, {
      columns: cols,
      rows,
      pixelWidth: Math.min(65535, Math.round(rect.width)),
      pixelHeight: Math.min(65535, Math.round(rect.height)),
    });
  });
  const linkSubscription = term.registerLinkProvider({
    provideLinks(row, callback) {
      callback(sourceLinks(term, id, row));
    },
  });
  let fitFrame = null;
  const fit = () => {
    if (fitFrame !== null) cancelAnimationFrame(fitFrame);
    fitFrame = requestAnimationFrame(() => {
      fitFrame = null;
      if (instances.get(id)?.term === term) fitAddon.fit();
    });
  };
  const resizeObserver = new ResizeObserver(fit);
  resizeObserver.observe(container);
  instances.set(id, {
    term,
    fitAddon,
    container,
    resizeObserver,
    subscriptions: [dataSubscription, resizeSubscription, linkSubscription],
    cancelFit: () => {
      if (fitFrame !== null) cancelAnimationFrame(fitFrame);
    },
  });
  fit();
  requestAnimationFrame(() => {
    if (instances.get(id)?.term !== term) return;
    emit("ready", id);
  });
}

function write(id, data) {
  instances.get(id)?.term.write(new Uint8Array(data));
}

async function action(id, name) {
  const instance = instances.get(id);
  if (!instance) {
    emit("action_result", id, { action: name, ok: false, message: "Terminal is not ready" });
    return;
  }
  const { term, fitAddon } = instance;
  try {
    switch (name) {
      case "clear":
        term.clear();
        break;
      case "copy": {
        const selection = term.getSelection();
        if (!selection) throw new Error("Select terminal text before copying");
        if (!navigator.clipboard?.writeText) throw new Error("Clipboard write access is unavailable");
        await navigator.clipboard.writeText(selection);
        emit("action_result", id, { action: name, ok: true, message: "Selection copied" });
        break;
      }
      case "paste": {
        if (!navigator.clipboard?.readText) throw new Error("Clipboard read access is unavailable");
        const text = await navigator.clipboard.readText();
        if (!text) throw new Error("Clipboard is empty");
        term.paste(text);
        emit("action_result", id, { action: name, ok: true, message: "Clipboard pasted" });
        break;
      }
      case "focus":
        term.focus();
        break;
      case "fit":
        fitAddon.fit();
        break;
    }
  } catch (error) {
    emit("action_result", id, {
      action: name,
      ok: false,
      message: String(error?.message ?? error),
    });
  }
}

function dispose(id) {
  const instance = instances.get(id);
  if (!instance) return;
  instance.cancelFit();
  instance.resizeObserver.disconnect();
  for (const subscription of instance.subscriptions) subscription.dispose();
  instance.term.dispose();
  instances.delete(id);
}

window.SyntaxisTerminalBridge = { mount, write, action, dispose };
