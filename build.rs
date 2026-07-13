use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=tailwind.css");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = manifest_dir.join("assets/tailwind.css");

    // `asset!` validates paths during ordinary Cargo checks, while Dioxus only
    // generates Tailwind output for `dx build` and `dx serve`. Keep a disposable
    // placeholder so Cargo-only quality gates work from a clean checkout.
    if !output.exists() {
        fs::File::create(output).expect("failed to create Tailwind output placeholder");
    }
}
