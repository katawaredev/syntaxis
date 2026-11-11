# Terminal renderer bridge

`bridge-source.js` imports xterm and FitAddon. `bun run build:terminal` generates
the ignored `crates/runtime-remote/assets/terminal.bundle.js`, including its CSS.
The runtime adapter loads it idempotently when a terminal mounts; shared Terminal
UI does not own script loading. The build cache hashes sources and package versions.

Keep xterm unless a measured emulation or complex-script problem justifies a
replacement. A renderer change needs source-link/disposal compatibility, compressed
payload and startup measurements, plus real mobile IME, keyboard, selection,
scrolling and resize checks. A different browser renderer does not reduce server
PTY work. See [browser acceptance](../../../../docs/testing.md#browser-acceptance).
