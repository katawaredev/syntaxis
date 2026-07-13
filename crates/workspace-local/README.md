# syntaxis-workspace-local

Native implementation of `syntaxis-workspace` for Linux desktop and the
self-hosted server runtime.

Responsibilities are split by module:

- `registry`: migrated SQLite persistence and local/allowlisted registration;
- `path_scope`: canonical workspace-bound path resolution;
- `files`: bounded text I/O, atomic writes, and safe mutations;
- `watcher`: recursive, filtered, batched filesystem events.

Native dependencies are target-gated and never enter the web WASM build.
