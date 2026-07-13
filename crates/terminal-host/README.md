# syntaxis-terminal-host

Linux host PTY sessions for the remote terminal server. The Dioxus WebSocket
endpoint remains in the application crate; this crate owns only process/session
lifecycle and byte streams.
