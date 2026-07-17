import { WTerm } from "@wterm/dom";

const instances = new Map();

function emit(kind, id, detail = {}) {
  window.dispatchEvent(
    new CustomEvent("syntaxis-terminal", {
      detail: { kind, id, ...detail },
    }),
  );
}

async function mount(id) {
  dispose(id);

  const container = document.getElementById(id);
  if (!container) throw new Error(`Terminal container ${id} was not found`);

  // Dioxus may reuse the same DOM node when the keyed terminal component is
  // replaced. Dispose the renderer that previously owned that node (or any
  // renderer whose node has already been detached) before mounting a new one.
  for (const [instanceId, instance] of instances) {
    if (instance.container === container || !instance.container.isConnected) {
      dispose(instanceId);
    }
  }
  container.replaceChildren();

  const term = new WTerm(container, {
    autoResize: true,
    cursorBlink: true,
    onData(data) {
      emit("input", id, { data });
    },
    onResize(cols, rows) {
      const rect = container.getBoundingClientRect();
      emit("resize", id, {
        columns: cols,
        rows,
        pixelWidth: Math.min(65535, Math.round(rect.width)),
        pixelHeight: Math.min(65535, Math.round(rect.height)),
      });
    },
  });

  await term.init();
  instances.set(id, { term, container });

  // Let the layout settle, then signal readiness.
  requestAnimationFrame(() => {
    if (instances.get(id)?.term !== term) return;
    emit("ready", id);
  });
}

function write(id, data) {
  const instance = instances.get(id);
  if (!instance) return;
  instance.term.write(new Uint8Array(data));
}

async function action(id, name) {
  const instance = instances.get(id);
  if (!instance) {
    emit("action_result", id, { action: name, ok: false, message: "Terminal is not ready" });
    return;
  }
  const { term, container } = instance;
  try {
    switch (name) {
      case "clear":
        term.write("\x1b[2J\x1b[H");
        break;
      case "copy": {
        const selection = window.getSelection()?.toString();
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
        term.write(text);
        emit("action_result", id, { action: name, ok: true, message: "Clipboard pasted" });
        break;
      }
      case "focus": {
        const textarea = container.querySelector("textarea");
        if (textarea instanceof HTMLTextAreaElement) textarea.focus({ preventScroll: true });
        else term.focus();
        break;
      }
      case "fit": {
        const rect = container.getBoundingClientRect();
        const cols = Math.max(1, Math.floor(rect.width / 8));
        const rows = Math.max(1, Math.floor(rect.height / 18));
        term.resize(cols, rows);
        break;
      }
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
  instance.term.destroy();
  instances.delete(id);
}

window.SyntaxisTerminalBridge = { mount, write, action, dispose };
