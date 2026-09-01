use std::{env, error::Error, fs, io, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=../../tailwind.css");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Cargo must provide CARGO_MANIFEST_DIR",
        )
    })?);
    let output = manifest_dir.join("assets/tailwind.css");
    if !output.exists() {
        fs::File::create(output)?;
    }
    for asset in [
        "geist-latin-wght-normal.woff2",
        "favicon.ico",
        "favicon.svg",
        "favicon-96x96.png",
        "apple-touch-icon.png",
        "site.webmanifest",
        "web-app-manifest-192x192.png",
        "web-app-manifest-512x512.png",
    ] {
        fs::copy(
            manifest_dir.join("../../assets").join(asset),
            manifest_dir.join("assets").join(asset),
        )?;
    }
    Ok(())
}
