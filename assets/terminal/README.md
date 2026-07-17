# Terminal renderer assets

`bridge-source.js` is the application bridge that imports from the `@wterm/dom`
package. It is bundled with esbuild into `terminal.bundle.js` (browser IIFE
format, minified) by the build script. The wterm CSS is inlined into the bundle
and injected into the document on first mount.

## Building

From the project root:

```sh
bun install
bun run build:terminal
```

This generates `terminal.bundle.js`. The generated file is git-ignored — run the
build after `bun install`.

To change the pinned version, edit `@wterm/dom` in `package.json` and re-run the
build. Review the API and license before upgrading.
