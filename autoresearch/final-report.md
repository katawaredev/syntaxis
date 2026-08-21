# Mobile startup performance campaign report

## Outcome

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

## Method

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

## Successful experiments

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

## Rejected major ideas

- **AR-004 — SSR workspace-list seeding:** removed the client workspace-list
  request and improved Recent Projects by only 17 ms, while regressing FCP by
  217 ms, LCP by 151 ms, TBT by 59 ms, and application usability by 231 ms. The
  server/API work was already only a few milliseconds; moving it into the
  document and hydration path was counterproductive. The change was reverted.
- **AR-006 — defer the global UI helper:** improved FCP to 952.6 ms but regressed
  TBT by 23.5% and application usability by 110 ms. It merely shifted required
  work after FCP, so it was rejected and reverted.

## Correctness and generality

The final product source passed the repository's web and server quality gates.
Browser smoke checks also covered:

- empty Recent Projects;
- populated Recent Projects with small, medium, and larger repository paths;
- 320x700 narrow mobile, the fixed 390x844 workload, and 1440x900 desktop;
- the fixed editor route and editable CodeMirror surface; and
- the AI route after route-local helper loading.

The Recent Projects section became ready under the exact two-condition rule in
all states and viewports. The representative browser checks produced no page or
console errors.

Lighthouse's performance assertions do **not** all pass: final TBT is 423 ms,
above the existing 300 ms threshold, and Lighthouse still warns about the long
interactive time. Accessibility and browser-error assertions now pass. No
assertion, test, lint rule, accessibility behavior, or feature was weakened.

## Current bottlenecks and remaining opportunities

The dominant remaining cost is the unified 4.02 MB Brotli client WASM and roughly
375–440 ms of Dioxus bootstrap evaluation. Lighthouse's simulated interactive
metric remains around 21.7 seconds because that bootstrap produces a late long
task under mobile throttling. Document delivery, workspace enumeration,
serialization, runtime-state requests, and availability requests were measured
in only a few milliseconds and are not material bottlenecks. Editor/LSP and
terminal assets are not on the Home request path after the accepted changes.

The most credible follow-up is true route/code splitting once the documented
upstream Walrus failure is resolved. A separate size/profile campaign could then
identify feature-preserving modularization opportunities in shared UI, icon, or
parser code. It would also be useful to separate warm and first-browser-process
Recent Projects distributions: the roughly 1.2-second first sample persists even
though the workspace API itself is fast. Smaller script scheduling changes are
unlikely to be worthwhile without new trace evidence.

The release-build-time increase from LTO is the main accepted tradeoff. The final
Recent Projects and editor medians are 12.8 ms and 5.6 ms slower than baseline,
respectively; both sit within the observed browser variability, but they remain
recorded rather than presented as wins. Further work should use the same fixed
workload and establish a fresh baseline before making changes.
