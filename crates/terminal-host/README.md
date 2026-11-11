# syntaxis-terminal-host

Linux host PTY sessions for the remote terminal server. The Dioxus WebSocket
endpoint lives in `runtime-remote`; this crate owns only process/session
lifecycle and byte streams.
