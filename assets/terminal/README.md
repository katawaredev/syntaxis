# Terminal renderer assets

`ghostty-web.bundle.js` bundles the application bridge in `bridge-source.js`
with the pinned `ghostty-web` npm package version 0.4.0. The upstream package's
WASM is embedded in that bundle; `ghostty-vt.wasm` is also retained beside it as
the reviewed upstream artifact. `ghostty-web.LICENSE` is the upstream MIT
license.

To refresh the bundle after deliberately changing the pinned version, install
that exact package and bundle `bridge-source.js` with esbuild for the browser
IIFE format. Do not replace the version without reviewing the API and license.
