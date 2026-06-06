use std::path::Path;

fn main() {
    // The Linux-only Decky plugin installer (`src/decky_install.rs`) embeds the
    // built plugin bundle via `include_str!("../../../decky/dist/index.js")`.
    // That file is a `bun run build` artifact and is gitignored, so on a fresh
    // checkout it may be absent — which would hard-fail the Linux compile before
    // CI's "Build Decky plugin" step has a chance to run on a stale cache, or
    // when a dev runs `cargo build` directly. Create an empty placeholder if
    // missing so the compile always succeeds; real Linux release builds run
    // `bun run build` first so the genuine bundle is what actually gets embedded.
    //
    // Only relevant when targeting Linux (the embed is `#[cfg(target_os =
    // "linux")]`); skip otherwise to avoid creating stray files on Win/macOS.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        let dist = Path::new("../../decky/dist");
        let index = dist.join("index.js");
        if !index.exists() {
            let _ = std::fs::create_dir_all(dist);
            let _ = std::fs::write(
                &index,
                "// placeholder — run `bun run build` in decky/ to embed the real plugin bundle\n",
            );
        }
        // Rebuild Spool when the embedded bundle changes.
        println!("cargo:rerun-if-changed=../../decky/dist/index.js");
    }

    // tauri_build::build() validates that every externalBin entry exists on
    // disk with the target-triple suffix (e.g. `ludusavi-aarch64-unknown-linux-gnu`).
    // Real binaries are downloaded by `scripts/download-sidecars.js`; stub any
    // that are missing so `cargo check` / `cargo clippy` work without them —
    // the stubs are never executed, only bundled when doing a real `tauri build`.
    let target = std::env::var("TARGET").unwrap_or_default();
    let is_windows = target.contains("windows");
    let ext = if is_windows { ".exe" } else { "" };
    let sidecars = ["ludusavi", "rclone"];
    for name in sidecars {
        let stub = Path::new("binaries").join(format!("{name}-{target}{ext}"));
        if !stub.exists() {
            let _ = std::fs::create_dir_all("binaries");
            let _ = std::fs::write(&stub, []);
        }
    }

    tauri_build::build()
}
