fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    if std::env::var_os("CARGO_FEATURE_EMBED_FRONTEND").is_none() {
        return;
    }

    // Track directory membership as well as contents: hashed assets can be added or removed.
    println!("cargo::rerun-if-changed=frontend/dist");
    let index = std::path::Path::new("frontend/dist/index.html");
    assert!(
        index
            .metadata()
            .is_ok_and(|file| file.is_file() && file.len() > 0),
        "frontend/dist/index.html is missing or empty; install frontend dependencies with \
         `pnpm --dir frontend install --frozen-lockfile`, then run `cargo xtask build`"
    );
}
