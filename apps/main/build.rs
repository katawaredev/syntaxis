use std::{env, fs, path::PathBuf};

const COPIED_ASSETS: &[&str] = &[
    "ai-chat.js",
    "ai/chat.css",
    "apple-touch-icon.png",
    "favicon-96x96.png",
    "favicon.ico",
    "favicon.svg",
    "files/markdown-preview.css",
    "geist-latin-wght-normal.woff2",
    "site.webmanifest",
    "terminal/terminal.bundle.js",
    "ui.js",
    "web-app-manifest-192x192.png",
    "web-app-manifest-512x512.png",
    "websocket-compat.js",
];

fn main() {
    println!("cargo:rerun-if-changed=../../tailwind.css");
    println!("cargo:rerun-if-changed=src/wasm_stderr.c");

    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .expect("Cargo must provide CARGO_MANIFEST_DIR to build scripts"),
    );
    let repository_assets = manifest_dir.join("../../assets");
    let app_assets = manifest_dir.join("assets");
    fs::create_dir_all(&app_assets).expect("failed to create the main app asset directory");

    for relative_path in COPIED_ASSETS {
        println!("cargo:rerun-if-changed=../../assets/{relative_path}");
        let source = repository_assets.join(relative_path);
        let destination = app_assets.join(relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).expect("failed to create a main app asset subdirectory");
        }
        fs::copy(&source, &destination).unwrap_or_else(|error| {
            panic!(
                "failed to stage main app asset {}: {error}",
                source.display()
            )
        });
    }

    let output = app_assets.join("tailwind.css");

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
