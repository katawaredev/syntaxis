// Bundled with ghostty-web 0.4.0 into ghostty-web.bundle.js.
import { FitAddon, init, Terminal } from "ghostty-web";

const instances = new Map();
let initialized;

function emit(kind, id, detail = {}) {
  window.dispatchEvent(
    new CustomEvent("syntaxis-terminal", {
      detail: { kind, id, ...detail },
    }),
  );
}

async function mount(id) {
  dispose(id);
  initialized ??= init();
  await initialized;

  const container = document.getElementById(id);
  if (!container) throw new Error(`Terminal container ${id} was not found`);

  // Dioxus may reuse the same DOM node when the keyed terminal component is
  // replaced. Dispose the renderer that previously owned that node (or any
  // renderer whose node has already been detached) before adding a new canvas.
  for (const [instanceId, instance] of instances) {
    if (instance.container === container || !instance.container.isConnected) {
      dispose(instanceId);
    }
  }
  container.replaceChildren();

  const terminal = new Terminal({
    allowProposedApi: false,
    cursorBlink: true,
    cursorStyle: "block",
    fontFamily: "'Geist Mono', 'SFMono-Regular', Consolas, monospace",
    fontSize: 13,
    lineHeight: 1.25,
    scrollback: 5000,
    theme: {
      background: "#1f2021",
      foreground: "#e5e5e5",
      cursor: "#e5e5e5",
      cursorAccent: "#1f2021",
      selectionBackground: "#3b82f666",
      black: "#282a2e",
      red: "#e06c75",
      green: "#98c379",
      yellow: "#e5c07b",
      blue: "#61afef",
      magenta: "#c678dd",
      cyan: "#56b6c2",
      white: "#d7dae0",
      brightBlack: "#5c6370",
      brightRed: "#ff7a85",
      brightGreen: "#b3e58d",
      brightYellow: "#ffd68a",
      brightBlue: "#72c2ff",
      brightMagenta: "#dd91f3",
      brightCyan: "#6bdde8",
      brightWhite: "#ffffff",
    },
  });
  const fit = new FitAddon();
  terminal.loadAddon(fit);
  terminal.open(container);
  const disposables = [
    terminal.onData((data) => emit("input", id, { data })),
    terminal.onResize(({ cols, rows }) => {
      const rect = container.getBoundingClientRect();
      emit("resize", id, {
        columns: cols,
        rows,
        pixelWidth: Math.min(65535, Math.round(rect.width)),
        pixelHeight: Math.min(65535, Math.round(rect.height)),
      });
    }),
  ];
  const instance = { terminal, fit, disposables, container, ready: false, pending: [] };
  instances.set(id, instance);
  fit.observeResize();
  requestAnimationFrame(() => {
    if (instances.get(id) !== instance) return;
    fit.fit();
    // ghostty-web renders incrementally and browsers may recycle the previous
    // canvas backing store. Clear only after the canvas has its fitted size,
    // then replay bytes that arrived while the renderer was mounting.
    terminal.reset();
    instance.ready = true;
    for (const data of instance.pending) terminal.write(data);
    instance.pending.length = 0;
    terminal.focus();
    emit("ready", id);
  });
}

function write(id, data) {
  const instance = instances.get(id);
  if (!instance) return;
  const bytes = new Uint8Array(data);
  if (instance.ready) {
    instance.terminal.write(bytes);
  } else {
    instance.pending.push(bytes);
  }
}

async function action(id, name) {
  const instance = instances.get(id);
  if (!instance) {
    emit("action_result", id, { action: name, ok: false, message: "Terminal is not ready" });
    return;
  }
  const { terminal, fit } = instance;
  try {
    switch (name) {
      case "clear":
        terminal.clear();
        break;
      case "copy": {
        const selection = terminal.getSelection();
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
        terminal.paste(text);
        emit("action_result", id, { action: name, ok: true, message: "Clipboard pasted" });
        break;
      }
      case "focus":
        terminal.focus();
        break;
      case "fit":
        fit.fit();
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
  for (const disposable of instance.disposables) disposable.dispose();
  instance.fit.dispose();
  instance.terminal.dispose();
  instances.delete(id);
}

window.SyntaxisTerminalBridge = { mount, write, action, dispose };
