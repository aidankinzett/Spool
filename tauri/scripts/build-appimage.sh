#!/usr/bin/env bash
# Builds the Tauri AppImage locally, applying the same libwayland stripping
# and repack that the CI does. Run via `bun run build:appimage` from tauri/.
#
# Env vars:
#   TAURI_SIGNING_PRIVATE_KEY / TAURI_SIGNING_PRIVATE_KEY_PASSWORD
#     If set, the repacked AppImage is re-signed (same as CI).
#     If absent, the build runs unsigned (updater artifacts are skipped).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAURI_DIR="$(dirname "$SCRIPT_DIR")"
REPO_ROOT="$(dirname "$TAURI_DIR")"
APPIMAGE_DIR="$TAURI_DIR/src-tauri/target/release/bundle/appimage"
APPIMAGETOOL_CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/spool/appimagetool"
APPIMAGETOOL_URL="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
TAURI_CONF="$TAURI_DIR/src-tauri/tauri.conf.json"

# ── Decky plugin ─────────────────────────────────────────────────────────────
echo "==> Building Decky plugin..."
cd "$REPO_ROOT/decky"
if command -v pnpm &>/dev/null; then
  pnpm install --frozen-lockfile
  pnpm build
else
  corepack enable
  corepack pnpm install --frozen-lockfile
  corepack pnpm build
fi

# ── Frontend deps + sidecars ──────────────────────────────────────────────────
echo "==> Installing frontend dependencies..."
cd "$TAURI_DIR"
bun install

echo "==> Downloading sidecars (if needed)..."
bun run download-sidecars

# ── Patch tauri.conf.json if no signing key ───────────────────────────────────
# When TAURI_SIGNING_PRIVATE_KEY is absent the build fails after producing the
# AppImage because it can't sign the updater artifact. Temporarily disable
# updater artifact creation so the build completes, then restore the config.
CONF_ORIG=$(cat "$TAURI_CONF")
if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  echo "==> No signing key — building unsigned (updater artifacts disabled)"
  trap 'printf "%s\n" "$CONF_ORIG" > "$TAURI_CONF"' EXIT
  jq '.bundle.createUpdaterArtifacts = false' \
    "$TAURI_CONF" > "$TAURI_CONF.tmp" && mv "$TAURI_CONF.tmp" "$TAURI_CONF"
else
  trap 'printf "%s\n" "$CONF_ORIG" > "$TAURI_CONF"' EXIT
fi

# ── Tauri AppImage build ──────────────────────────────────────────────────────
# NO_STRIP=1: linuxdeploy-plugin-appimage bundles an old `strip` from Ubuntu
# that can't handle .relr.dyn ELF sections produced by modern toolchains
# (CachyOS, Bazzite, SteamOS). Skip stripping; the libraries work fine as-is.
echo "==> Building Tauri AppImage (this takes a while)..."
NO_STRIP=1 bun run tauri build --bundles appimage

# Restore config now (trap fires on EXIT but also restore early so the strip
# step doesn't inherit a modified config path if something re-reads it)
printf "%s\n" "$CONF_ORIG" > "$TAURI_CONF"
trap - EXIT

# ── Strip bundled libwayland-* ────────────────────────────────────────────────
# linuxdeploy-plugin-gtk over-bundles libwayland-{client,cursor,egl,server}.
# On Wayland sessions with newer Mesa the stale bundled libwayland-client
# aborts WebKit with EGL_BAD_PARAMETER before render. Strip them so WebKit
# falls back to the host's libs which match the host compositor's protocol.
echo "==> Extracting AppImage..."
cd "$APPIMAGE_DIR"
APPIMAGE=$(ls Spool_*_amd64.AppImage 2>/dev/null | grep -v '^Spool_amd64' | head -1)
if [ -z "$APPIMAGE" ]; then
  echo "Error: no AppImage found in $APPIMAGE_DIR" >&2
  exit 1
fi
chmod +x "$APPIMAGE"
./"$APPIMAGE" --appimage-extract >/dev/null

echo "==> Stripping libwayland-* from squashfs-root..."
rm -f squashfs-root/usr/lib/libwayland-client.so.* \
      squashfs-root/usr/lib/libwayland-cursor.so.* \
      squashfs-root/usr/lib/libwayland-egl.so.* \
      squashfs-root/usr/lib/libwayland-server.so.*

# ── appimagetool ─────────────────────────────────────────────────────────────
if [ ! -x "$APPIMAGETOOL_CACHE" ]; then
  echo "==> Downloading appimagetool (cached to $APPIMAGETOOL_CACHE)..."
  mkdir -p "$(dirname "$APPIMAGETOOL_CACHE")"
  wget -q --show-progress -O "$APPIMAGETOOL_CACHE" "$APPIMAGETOOL_URL"
  chmod +x "$APPIMAGETOOL_CACHE"
fi

echo "==> Repacking AppImage..."
rm -f "$APPIMAGE" "${APPIMAGE}.sig" 2>/dev/null || true
ARCH=x86_64 "$APPIMAGETOOL_CACHE" --appimage-extract-and-run squashfs-root Spool_amd64.AppImage
rm -rf squashfs-root

# ── Optional re-sign ──────────────────────────────────────────────────────────
if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  echo "==> Re-signing AppImage..."
  cd "$TAURI_DIR"
  bunx @tauri-apps/cli signer sign "$APPIMAGE_DIR/Spool_amd64.AppImage"
else
  echo "==> Skipping signing (set TAURI_SIGNING_PRIVATE_KEY to sign)"
fi

echo ""
echo "Done: $APPIMAGE_DIR/Spool_amd64.AppImage"
