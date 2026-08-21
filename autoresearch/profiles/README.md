# Profiles

Store reproducible artifacts here, with a short note containing the command,
commit, browser/tool versions, workload, and what the artifact proves. Suggested
artifacts:

- Lighthouse JSON and HTML reports for the fixed 390x844 mobile workload;
- Chrome performance traces covering navigation through usable UI;
- network request/response timing exports;
- server logs or lightweight host timing for document delivery;
- WASM and generated bundle size inventories.

Do not store fabricated or manually edited measurements. Large local captures may
remain untracked; retain a concise checked-in summary in `hypotheses.md`.
