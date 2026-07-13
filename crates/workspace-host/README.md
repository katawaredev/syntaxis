# syntaxis-workspace-host

Direct host-OS implementation of `syntaxis-workspace` for Linux desktop and the
self-hosted server runtime. Clients using this crate operate on the filesystem
of the process that loaded it; remote clients reach these services through a
separate transport adapter.

Responsibilities are split by module:

- `registry`: migrated SQLite persistence and unrestricted/allowlisted registration;
- `path_scope`: canonical workspace-bound path resolution;
- `files`: bounded text I/O, atomic writes, and safe mutations;
- `watcher`: recursive, filtered, batched filesystem events.

Host-OS dependencies are target-gated and never enter the web WASM build.
