# Startup hypotheses

## Initial ranked list (pre-baseline)

This is a set of questions, not profiling evidence. It must be ranked again after
`baseline.json` and trace artifacts exist.

1. **Critical-path JavaScript/WASM work** — determine how much of the mobile
   startup budget is spent downloading, compiling, instantiating, and hydrating
   the single Dioxus WASM bundle. `src/app.rs` mounts all routes in one router;
   the source comment records that route splitting is currently blocked upstream.
2. **Global eager assets** — measure the transfer and execution cost of the
   global Tailwind stylesheet, `ui.js`, `ai-chat.js`, websocket compatibility,
   and the preloaded Geist font before changing any loading behavior.
3. **Home route requests** — profile the workspace-list and runtime-state
   requests started by `src/workspace/home.rs`; verify whether server latency,
   serialization, or client hydration is on the usable-UI critical path.
4. **Editor/terminal bundles** — establish whether generated CodeMirror and
   xterm assets are requested or executed before their routes are used. The
   bundles are generated separately, but the editor bridge is embedded in the
   Rust crate and may affect WASM size.
5. **Server/document delivery** — use server timing and browser network traces to
   separate response generation from browser work. Do not infer this from source
   inspection alone.

## Active idea families

- **Exploit:** improve the largest measured critical-path cost without changing
  observable behavior.
- **Near miss:** retain submetric wins that need a second measurement or a
  compatible follow-up.
- **Structural:** investigate route/code splitting and route-local asset loading
  if traces show they dominate.
- **Simplification:** remove eager work or dependencies only when traces prove it
  is unnecessary for the representative workload.

No hypothesis has been accepted yet. Baseline and profile artifacts are expected
from a real benchmark machine.
