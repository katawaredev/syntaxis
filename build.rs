use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=tailwind.css");
    println!("cargo:rerun-if-changed=src/wasm_stderr.c");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = manifest_dir.join("assets/tailwind.css");

    // `asset!` validates paths during ordinary Cargo checks, while Dioxus only
    // generates Tailwind output for `dx build` and `dx serve`. Keep a disposable
    // placeholder so Cargo-only quality gates work from a clean checkout.
    if !output.exists() {
        fs::File::create(output).expect("failed to create Tailwind output placeholder");
    }

    // arborium-tree-sitter's allocation diagnostics reference the C `stderr`
    // global, but wasm32-unknown-unknown's libc does not export it. The parser
    // only touches this path immediately before aborting on allocation failure;
    // supplying the missing symbol keeps normal parsing self-contained in WASM.
    if env::var("TARGET").as_deref() == Ok("wasm32-unknown-unknown") {
        cc::Build::new()
            .file("src/wasm_stderr.c")
            .compile("syntaxis_wasm_stdio");
    }
}
