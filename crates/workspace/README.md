# syntaxis-workspace

Platform-neutral workspace contracts shared by the Dioxus client, server, mocks,
and host runtime. This crate owns serializable records, runtime capabilities,
safe errors, relative paths, file versions, change events, and operation traits.

It intentionally has no dependency on Dioxus, SQLite, filesystem watching, or a
host filesystem API, so it can compile into the web WASM client.
