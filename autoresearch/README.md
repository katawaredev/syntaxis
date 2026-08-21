# Syntaxis startup performance campaign

This directory contains the repeatable evaluator and the evidence for the mobile
cold-start campaign. It is intentionally separate from product code and does not
contain benchmark-only behavior in the application.

## Architecture and performance map

- **Runtime:** Syntaxis is a Dioxus 0.7 fullstack application. The browser client
  is Rust compiled to WebAssembly; the server owns authentication, filesystem,
  workspaces, Git, terminals, previews, language servers, and Pi integration.
- **Document path:** `index.html` supplies the viewport and websocket compatibility
  script. Dioxus renders `src/app.rs`, which installs global links, the Tailwind
  stylesheet, UI and AI scripts, then mounts the router.
- **Startup/UI path:** the home route (`src/workspace/home.rs`) immediately starts
  the workspace-list cache and runtime-state resource. Workspace routes use
  `src/workspace/shell.rs`; files/editor UI is under `src/files/` and
  `crates/code-editor/`.
- **Editor boundary:** `assets/code-editor/bridge-source.js` and
  `assets/code-editor/lsp-source.js` are bundled by
  `scripts/build-code-editor.mjs`. The Rust editor includes the generated bridge
  and sends configuration through Dioxus `document::eval`; language-server code is
  loaded separately by the browser bridge.
- **Terminal boundary:** xterm sources under `assets/terminal/` are bundled by
  `scripts/build-terminal.mjs` and loaded only by terminal UI.
- **Server responsibilities:** the fullstack server and host crates perform
  filesystem and process work. The Lighthouse server helper starts the optimized
  web server at `127.0.0.1:4173` and is already used by `lighthouserc.json`.
- **Existing evaluator:** `lighthouserc.json` runs three median-aggregated audits
  at a 390x844 mobile viewport. `package.json`/`Justfile` build a release web app,
  run LHCI, and write reports to `lighthouse-reports/`.
- **Likely startup cost centers to measure:** HTML/server delivery, websocket and
  Dioxus bootstrap, WASM transfer/compile/hydration, global CSS and scripts,
  generated editor/terminal bundles, and eager workspace/runtime requests. No
  product optimization is implied by this list.

## Evaluator commands

From a real benchmark machine, after the fixed fixture and toolchain are
available:

```sh
bun run autoresearch:benchmark
bun run autoresearch:verify
```

`benchmark` runs the existing release Lighthouse setup, collects raw audit
reports, records the configured mobile inputs, computes medians/ranges, and
writes `autoresearch/baseline.json` on the first run (or a timestamped result
under `autoresearch/results/` on later runs). It also records release public
asset sizes. The `interactive` Lighthouse audit is retained as the consistent
proxy for application UI usability.

Results are compared within a machine/workload pair, not by comparing absolute
milliseconds across computers. Use the same PC, browser/tool versions, viewport,
fixture, server mode, and repetition count for baseline versus candidate:

```sh
bun run autoresearch:compare -- autoresearch/baseline.json autoresearch/results/run-....json
```

Each result carries tool versions and workload metadata so results from another
PC can form an independent baseline. Cross-machine comparisons should use
relative change from each machine's own baseline; asset-byte deltas are the most
portable absolute metric.

Recent Projects is explicitly part of the workload. The benchmark records its
stable section/loading selectors in every result and measures
`recentProjectsUsableMs` when the optional Playwright package is available:

```sh
AUTORESEARCH_MEASURE_RECENT_PROJECTS=true bun run autoresearch:benchmark
```

The metric is navigation start to the Recent Projects section becoming
non-loading. Without Playwright, the Lighthouse collector reports it as pending
rather than pretending that FCP or interactive measures it. The standalone
`bun run autoresearch:recent-projects` command is useful while validating the
fixture and selector behavior.

The editor route is deliberately not guessed by the harness: set
`AUTORESEARCH_EDITOR_URL` to the fixed fixture project's editor URL when the
real server fixture has been provisioned. It is recorded as benchmark input but
is not reported as measured until a reliable route-specific browser step is
added.

`verify` delegates to the repository's existing non-mutating web and server
checks (`mise run check` and `mise run check:server`). It must be run on a capable
machine; this environment intentionally does not execute builds or tests.

## Fixed workload

`fixture/` is the small, deterministic representative project used to provision
one workspace. It must be copied/imported by the benchmark operator rather than
special-cased by product code. Keep the fixture contents and URL constant across
baseline and experiments. Use a second larger realistic project periodically to
check that a result generalizes.

## Evidence layout

- `baseline.json`: generated baseline; never hand-edit measurements.
- `results/`: subsequent benchmark results (ignored locally unless explicitly
  retained as evidence).
- `profiles/`: browser/server/profile artifacts and notes.
- `hypotheses.md`: ranked evidence-backed bottlenecks and active idea families.
- `experiments.jsonl`: one machine-readable decision per experiment.
- `final-report.md`: stopping report for the completed campaign.

The baseline cannot be established in this environment. No baseline values are
invented here; the first real-machine run is the reference point.
