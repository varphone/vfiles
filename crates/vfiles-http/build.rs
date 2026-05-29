use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=VFILES_FRONTEND_DIST");

    if std::env::var_os("CARGO_FEATURE_EMBED").is_none() {
        return;
    }

    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set"),
    );
    let default_dist = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root should exist")
        .join("client")
        .join("dist");

    let configured_dist = std::env::var_os("VFILES_FRONTEND_DIST")
        .map(PathBuf::from)
        .unwrap_or(default_dist);
    let frontend_dist = configured_dist.canonicalize().unwrap_or(configured_dist);

    if !frontend_dist.is_dir() || !frontend_dist.join("index.html").is_file() {
        panic!(
            "embed feature requires a built frontend dist directory containing index.html; resolved path: {}",
            frontend_dist.display()
        );
    }

    println!("cargo:rerun-if-changed={}", frontend_dist.display());
    println!(
        "cargo:rustc-env=VFILES_EMBED_FRONTEND_DIST={}",
        frontend_dist.display()
    );
}
