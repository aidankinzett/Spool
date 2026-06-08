use std::path::Path;

fn main() {
    // Load a local `.env` (gitignored) into the compile environment so secrets
    // like the Google Drive OAuth client (read via `option_env!` in rclone.rs)
    // can be supplied from a file during dev instead of exported in the shell.
    // CI sets these as real environment variables, so the file is dev-only.
    load_dotenv();

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

/// Parse `src-tauri/.env` (if present) and forward each `KEY=VALUE` into the
/// crate's compile environment via `cargo:rustc-env`, so `option_env!` /
/// `env!` see them. No external dependency; `.env` is gitignored.
///
/// Rules: blank lines and `#` comments are skipped; a leading `export ` is
/// stripped; the value is split on the first `=`; surrounding single or double
/// quotes are removed. Values already present in the real environment (e.g. CI)
/// take precedence and are not overwritten.
fn load_dotenv() {
    // Always re-run when the file appears, changes, or is removed.
    println!("cargo:rerun-if-changed=.env");

    let contents = match std::fs::read_to_string(".env") {
        Ok(c) => c,
        Err(_) => return, // no .env — nothing to do
    };

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        // Don't clobber a value the environment already provides (CI, shell).
        if std::env::var_os(key).is_some() {
            continue;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        println!("cargo:rustc-env={key}={value}");
    }
}
