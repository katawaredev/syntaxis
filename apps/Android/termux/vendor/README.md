# Manganis 0.7.10 compatibility patch

`manganis/` is the published crates.io `manganis` 0.7.10 source, with its
upstream MIT and Apache-2.0 licenses included. The root Cargo patch applies it
consistently to direct and transitive dependencies.

Only `src/android/mod.rs` differs: the `callback` module and its re-export are
restricted to 64-bit Android targets. Upstream's callback implementation contains
a compile-time rejection of every 32-bit Android build, including headless servers
that never call JNI. Syntaxis's Termux backend only needs asset registration.
No pointer conversion or JNI behavior is changed, and no 32-bit JNI callback API
is offered. Web, desktop, and 64-bit Android behavior is unchanged.

Remove this patch when upstream gates the optional callback system appropriately.
Do not use it to claim that Dioxus's native Android renderer supports ARMv7.
