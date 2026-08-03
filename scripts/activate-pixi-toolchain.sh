#!/usr/bin/env bash

# Rust defaults to a literal `cc`, but conda-forge exposes its native compiler
# through CC so that the matching sysroot and linker flags are retained.
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="${CC:?c-compiler did not set CC}"

# cc-rs must use Clang, rather than the native GCC, for the web build's C shim.
export CC_wasm32_unknown_unknown="clang"
