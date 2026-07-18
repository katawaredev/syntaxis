# Terminal renderer assets

`bridge-source.js` is the application bridge that imports from the `@xterm/xterm`
package. It is bundled with esbuild into `terminal.bundle.js` (browser IIFE
format, minified) by the build script. The xterm.js CSS is inlined into the bundle
and injected into the document on first mount.

## Building

From the project root:

```sh
bun install
bun run build:terminal
```

This generates `terminal.bundle.js`. The generated file is git-ignored — run the
build after `bun install`. The build script stores a content hash beside the
bundle and skips esbuild while its source files and pinned package versions are
unchanged.

To change the pinned version, edit `@xterm/xterm` in `package.json` and re-run the
build. Review the API and license before upgrading.

## Why xterm.js (and when to reconsider Ghostty)

Keep xterm.js unless a demonstrated terminal-emulation or complex-script issue
justifies a migration. `ghostty-web` can support the custom source link provider,
but it is a separate WebAssembly wrapper around Ghostty rather than the official
Ghostty browser UI. At the time of the last evaluation, it added a roughly 423 KB
WASM asset plus JavaScript, required asynchronous initialization, targeted an
older xterm API, and returned no disposable from `registerLinkProvider`. It did
not reduce VPS work because terminal emulation runs in the browser either way.

Before proposing Ghostty again, check the current *published* package rather than
only its main branch:

- Confirm custom `registerLinkProvider` support and adapt its disposal semantics.
- Confirm compatibility with the pinned xterm FitAddon API or replace the addon.
- Measure compressed JS/WASM size, initialization time, memory, and rendering on
  a slow connection and representative low-end phone.
- Test iOS Safari and Android keyboard/IME input, composition, touch selection,
  scrolling, viewport resizing when the soft keyboard opens, and accessibility.
- Check whether `libghostty` and the wrapper now have stable versioned APIs and
  no longer require downstream patches.

Better grapheme/complex-script handling is the strongest reason to revisit it.
Without a concrete xterm limitation, the extra payload and smaller mobile testing
surface are poor tradeoffs for this mobile-first application.
