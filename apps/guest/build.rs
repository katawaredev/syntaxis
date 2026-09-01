use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=../../tailwind.css");

    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo must provide CARGO_MANIFEST_DIR"),
    );
    let output = manifest_dir.join("assets/tailwind.css");
    if !output.exists() {
        fs::File::create(output).expect("failed to create Tailwind output placeholder");
    }
    fs::copy(
        manifest_dir.join("../../assets/geist-latin-wght-normal.woff2"),
        manifest_dir.join("assets/geist-latin-wght-normal.woff2"),
    )
    .expect("failed to copy the shared Geist font");
}
