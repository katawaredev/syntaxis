# Performance autoresearch report

## Second-pass outcome

The second pass found that syntax highlighting for Git diffs pulled every
`dioxus-code`/Arborium language parser into the root client WASM even though the
route-local CodeMirror bundle already contains the required parsers. Diff
highlighting now uses that existing browser module, and the editor bridge is a
cacheable ES module instead of fetched text executed with `eval` on every mount.

Against the accepted first-pass build, two candidate collections reproduced the
important runtime gains. The retained five-run candidate reduced median TBT by
22.7%, Lighthouse application-UI usability time by 53.1%, Recent Projects
readiness by 35.7%, the paired Home task by 32.5%, and editor readiness by 29.1%.
Initial WASM Brotli size fell 58.3%, from 4,017,290 B to 1,676,041 B. LCP and the
90 ms observed load remained effectively unchanged. FCP varied between neutral
and 3.8% better across the two candidate sets, so no independent FCP improvement
is claimed.

The candidate release build completed successfully. Browser checks found no
console, page, or request failures and no horizontal overflow on Home or editor
at 320x700 or 1440x900. Full repository QA remains to be run after explicit
confirmation because it can rewrite Rust formatting and Clippy findings.

## Second-pass measurements

The baseline and candidate used the same release artifact workflow, 390x844 DPR
2 mobile workload, toolchain, fixed fixture contents, and five repetitions. Each
direct route measurement launched a fresh Chromium process; collection order
alternated between Home and editor.

| Metric | First-pass baseline median (range) | Second-pass candidate median (range) | Change |
| --- | ---: | ---: | ---: |
| FCP | 1,094.0 ms (1,052.0–1,103.7) | 1,052.4 ms (1,051.1–1,116.3) | -3.8% |
| LCP | 1,354.3 ms (1,352.2–1,503.7) | 1,352.4 ms (1,351.1–1,352.9) | -0.1% |
| TBT | 422 ms (376–464) | 326 ms (326–332) | -22.7% |
| Initial page load | 87 ms (79–95) | 90 ms (79–95) | +3.4% |
| Application UI usable | 21,727.6 ms (21,719.4–21,765.9) | 10,180.4 ms (10,093.5–10,183.7) | -53.1% |
| Recent Projects usable | 406.6 ms (404.8–1,207.8) | 261.4 ms (228.3–271.4) | -35.7% |
| New Project interaction | 33.9 ms (32.2–44.3) | 36.3 ms (32.2–1,077.8) | +7.1% |
| Paired Home task | 446.6 ms (438.1–1,240.1) | 301.3 ms (286.6–1,306.1) | -32.5% |
| Editor usable | 657.9 ms (649.8–745.6) | 466.4 ms (457.6–477.1) | -29.1% |

The isolated interaction delta is 2.4 ms and is below the observed browser
variability. Its candidate range contains a repeatable first-process animation
frame outlier, so the paired navigation-to-completed-task distribution is the
more representative Home workflow result.

| Release asset measure | First-pass baseline | Second-pass candidate | Change |
| --- | ---: | ---: | ---: |
| Initial WASM raw | 42,432,547 B | 6,980,469 B | -83.5% |
| Initial WASM Brotli | 4,017,290 B | 1,676,041 B | -58.3% |
| All public assets raw | 45,328,739 B | 9,641,988 B | -78.7% |
| All public assets Brotli | 4,787,824 B | 2,412,780 B | -49.6% |

The all-assets reduction is smaller because the 1.67 MB raw CodeMirror bridge is
still shipped as a route-local asset. It is no longer duplicated as Rust parser
code in the initial WASM.

## Second-pass harness improvements

- Increased both direct-browser and Lighthouse collections from three to five
  runs and added tested median, percentile, variance, deviation, and MAD helpers
  while preserving collection-order samples.
- Isolated every timed navigation in a fresh Chromium process and alternated
  route order, so “cold” no longer means a warm browser reused across all runs.
- Added the New Project interaction and paired Home task, editor readiness,
  long-task profiles, browser-error gates, and narrow-mobile/desktop overflow
  checks.
- Rejects Lighthouse/workload viewport or repetition drift, records source-state
  hashes, filters only newly created `.report.json` files, and handles server
  startup failure, timeout, and shutdown deterministically.
- Copies the fixture into an ignored runtime directory and initializes a
  deterministic standalone Git repository. The focused Git smoke temporarily
  changes only that copy, verifies the diff and route-local highlighting, and
  restores it without touching the parent Syntaxis worktree.

## First-pass outcome

The campaign reduced median mobile FCP by 16.0%, Lighthouse application-UI
usability time by 11.3%, TBT by 6.0%, and the initial WASM's Brotli size by
12.0%. LCP was effectively unchanged. The direct editor metric was also
effectively unchanged. Recent Projects remained dominated by a repeatable cold
browser outlier and its final median was 3.3% slower, so no improvement is
claimed for that metric.

The tables below retain the durable baseline-to-final results. Raw reports and
profiles were intentionally discarded after the campaign because they are tied
to one machine and toolchain; a future campaign should establish a fresh local
baseline.

## First-pass method

Baseline and candidates used the same machine, optimized Dioxus web server,
deterministically provisioned fixture, Chromium 151.0.7922.173, 390x844 viewport
at DPR 2, Lighthouse mobile simulation, and three repetitions. The evaluator
records the full environment, raw values, medians, ranges, variance, release
asset sidecar sizes, network timings, and the selectors used for readiness.

Recent Projects is timed from navigation start until both conditions hold:

- `section[aria-labelledby="recent-title"]` exists.
- `[aria-label="Loading recent projects"]` is absent.

Editor usability is timed to the mounted, editable CodeMirror textbox. The
fixture is registered through the normal workspace registry and API; there is
no fixture-name or benchmark-environment branch in product code. Each collection
requires a newly generated Lighthouse report set, preventing stale report reuse.

Toolchain: Rust 1.97.1, Dioxus 0.7.10, Bun 1.3.14, Lighthouse CLI 0.15.1, Linux
7.1.8 on an Intel i5-6600K with 16 GB RAM.

## Original baseline and final measurements

| Metric | Baseline median (range) | Final median (range) | Change |
| --- | ---: | ---: | ---: |
| FCP | 1,255.6 ms (1,254.2–1,501.4) | 1,054.6 ms (1,053.1–1,098.7) | -16.0% |
| LCP | 1,358.2 ms (1,353.6–1,501.4) | 1,353.1 ms (1,351.7–1,354.6) | -0.4% |
| TBT | 450 ms (449–3,773) | 423 ms (398–428) | -6.0% |
| Initial page load | 163 ms (84–2,859) | 91 ms (81–95) | -44.2% |
| Application UI usable | 24,434.2 ms (24,429.7–26,741.5) | 21,684.9 ms (21,652.7–21,722.5) | -11.3% |
| Recent Projects usable | 390.6 ms (375.6–1,169.8) | 403.4 ms (375.7–1,203.8) | +3.3% |
| Editor usable | 605.7 ms (592.9–1,309.7) | 611.3 ms (604.1–625.0) | +0.9% |

The large baseline ranges, especially its first cold-browser samples, are why
experiments were promoted only after repeated candidate sets. The final FCP
standard deviation was 21.1 ms, TBT 13.1 ms, application usability 28.5 ms,
Recent Projects 384.0 ms, and editor usability 8.7 ms. The initial-page-load
improvement is reported as an observed secondary result rather than a principal
claim because its baseline variance was especially high.

| Release asset measure | Baseline | Final | Change |
| --- | ---: | ---: | ---: |
| Initial WASM raw | 46,006,954 B | 42,432,547 B | -7.8% |
| Initial WASM Brotli | 4,562,886 B | 4,017,290 B | -12.0% |
| All public assets raw | 47,237,840 B | 45,328,739 B | -4.0% |
| All public assets Brotli | 4,876,205 B | 4,787,824 B | -1.8% |

The total-asset reduction is smaller than the initial-WASM reduction because the
editor bridge was moved to a route-local asset, not removed.

## First-pass successful experiments

- **AR-001 — route-local CodeMirror bridge:** removed a 1.67 MB generated editor
  bridge from the Home WASM and loaded the same bridge when the editor mounts.
  Two candidate sets reproduced roughly 9.5% lower application-usability time
  with neutral editor readiness.
- **AR-002 — correctness repairs:** restored the websocket helper through a
  hashed asset, enabled mobile zoom, and corrected two contrast failures. This
  removed the accessibility and console-error assertion failures without a
  material performance regression.
- **AR-003 — release LTO:** enabled LTO with one codegen unit. Two candidate sets
  reproduced smaller WASM and lower runtime blocking. A clean release build now
  takes about 12 minutes instead of about five on this machine.
- **AR-005 — route-local AI helper:** removed the AI helper and its global
  mutation observer from non-AI routes. Two candidate sets reproduced a 9–12%
  FCP improvement. A direct AI-route smoke test confirmed the helper loaded and
  exposed its expected API without browser errors.

## First-pass rejected major ideas

- **AR-004 — SSR workspace-list seeding:** removed the client workspace-list
  request and improved Recent Projects by only 17 ms, while regressing FCP by
  217 ms, LCP by 151 ms, TBT by 59 ms, and application usability by 231 ms. The
  server/API work was already only a few milliseconds; moving it into the
  document and hydration path was counterproductive. The change was reverted.
- **AR-006 — defer the global UI helper:** improved FCP to 952.6 ms but regressed
  TBT by 23.5% and application usability by 110 ms. It merely shifted required
  work after FCP, so it was rejected and reverted.

## Correctness and generality

The first-pass product source passed the repository's web and server quality
gates. The second-pass release build and focused browser smoke checks succeeded;
full current QA is pending confirmation. Combined browser smoke checks covered:

- empty Recent Projects;
- populated Recent Projects with small, medium, and larger repository paths;
- 320x700 narrow mobile, the fixed 390x844 workload, and 1440x900 desktop;
- the fixed editor route and editable CodeMirror surface; and
- a changed Rust file on the Git route with route-local syntax highlighting; and
- the AI route after route-local helper loading.

The Recent Projects section became ready under the exact two-condition rule in
all states and viewports. The representative browser checks produced no page or
console errors.

Lighthouse's performance assertions do **not** all pass: second-pass TBT is 326
ms, above the existing 300 ms threshold, and interactive time is 10,180 ms,
slightly above its 10,000 ms threshold. Accessibility and browser-error
assertions pass. No
assertion, test, lint rule, accessibility behavior, or feature was weakened.

## Current bottlenecks and remaining opportunities

The dominant remaining cost is the unified 1.68 MB Brotli client WASM and about
326 ms of blocking under Lighthouse mobile simulation. Its interactive metric
is now around 10.18 seconds. Document delivery, workspace enumeration,
serialization, runtime-state requests, and availability requests remain only a
few milliseconds and are not material bottlenecks. Editor/LSP and terminal
assets are not on the Home request path.

The most credible follow-up is true route/code splitting once the documented
upstream Walrus failure is resolved. A separate size-symbol profile could then
identify feature-preserving modularization opportunities in shared UI and icon
code. The first-process animation-frame outlier should remain visible rather than
be discarded; five-run medians and the paired task metric keep it from distorting
the main responsiveness conclusion. Smaller scheduling changes are unlikely to
be worthwhile without new trace evidence.

The release-build-time increase from first-pass LTO remains the main accepted
tradeoff. The second pass adds a route-local dynamic import when a diff first
requests highlighting, but plain diff text renders immediately and the same
module is already required by the editor. Further work should use the same fixed
workload and establish a fresh baseline before making changes.
