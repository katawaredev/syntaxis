# Performance harness

This directory measures server-app startup and Home/editor/Git interactions. It
also contains real-browser smoke harnesses; product unit tests live under `tests`
and beside their owning bridges. See [Testing](../docs/testing.md) for that map.

## Measure a change

On the same benchmark machine with the repository toolchain installed:

```sh
bun run autoresearch:benchmark
bun run autoresearch:compare -- autoresearch/baseline.json autoresearch/results/run-....json
```

The first run writes `baseline.json`; later runs write timestamped `results/`.
`AUTORESEARCH_SKIP_BUILD=true` reuses a release artifact only when its build already
succeeded and collection needs retrying. Normal comparisons rebuild both sides.
`bun run autoresearch:verify` delegates to the web/server checks and follows the
root `AGENTS.md` validation approval rule.

The harness uses the pinned Puppeteer with the machine's Chromium, fresh browser
processes, alternating route order and a fixed mobile workload. It records raw
collection order, distributions, tool versions, source state and asset sizes.
Compare repeated runs within one machine/workload pair. Use relative changes for
cross-machine comparisons and compressed asset bytes for portable size comparisons.
Do not discard cold-process outliers or promote a change on one Lighthouse score.

## Fixture and readiness

`provision.mjs` copies `fixture/` into ignored `runtime/fixture` and initializes an
independent deterministic Git repository. Measurements must never mutate the source
fixture or the Syntaxis worktree. The normal registry/API and evaluator-only bearer
token expose that copy; product code must not special-case the benchmark.

Keep fixture contents, viewport, repetition count and readiness selectors stable
between baseline and candidate. Home is ready when the Recent Projects section
exists and its loading indicator is gone. Editor readiness requires the mounted,
editable CodeMirror textbox. The paired Home task includes navigation and the New
Project interaction. A focused Git check edits only the runtime copy and restores
it after diff/highlighting and overflow checks.

`bun run autoresearch:recent-projects` runs these interaction checks against an
already-running benchmark server. `lighthouserc.json` owns audit settings;
`scripts/bundle-budgets.json` owns the current WASM limits. Generated results,
profiles and baseline files are ignored. Establish a fresh baseline after toolchain
or workload changes; old campaign numbers belong in Git history.

## Lessons worth preserving

- Route-local editor, terminal and AI loading reduce initial work. Diff highlighting
  should reuse CodeMirror rather than pulling a separate parser stack into WASM.
- LTO, a single codegen unit and the WASM `z` profile trade build/runtime costs for
  download size. Measure interaction latency as well as raw and Brotli bytes.
- Moving work past FCP can worsen blocking and task completion. Evaluate the full
  interaction rather than a paint metric alone.
- Workspace enumeration was cheap in the earlier campaign; SSR seeding increased
  hydration cost. Re-profile before optimizing that path.

Keep new findings actionable: the changed source path, reproducible workload,
observed tradeoff and a comparison artifact. Avoid permanent per-run status diaries.
