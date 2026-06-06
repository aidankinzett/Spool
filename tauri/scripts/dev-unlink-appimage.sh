#!/usr/bin/env bash
# Inverse of dev-link-appimage.sh: points the Decky plugin back at your real
# installed build of Spool.
#
# The plugin reads `spool_command` from its settings file on load and uses it
# to launch `spool --headless-server`. Clearing that key makes the plugin fall
# back to `~/.local/share/Spool/spool-launcher.sh` (the launcher the installed
# app writes on each launch). This script empties the key and restarts the
# plugin_loader service so the change takes effect immediately.
#
# Run via `bun run dev:unlink` from tauri/.
set -euo pipefail

SETTINGS_DIR="${HOME}/homebrew/settings/spool-backup"
SETTINGS_FILE="$SETTINGS_DIR/settings.json"

if [ ! -f "$SETTINGS_FILE" ]; then
  echo "Nothing to do: $SETTINGS_FILE does not exist."
  echo "The plugin already resolves spool from its default launcher."
  exit 0
fi

# Preserve existing settings (e.g. notify), clear only spool_command.
jq '.spool_command = ""' "$SETTINGS_FILE" > "$SETTINGS_FILE.tmp" \
  && mv "$SETTINGS_FILE.tmp" "$SETTINGS_FILE"

echo "spool_command cleared -> plugin falls back to ~/.local/share/Spool/spool-launcher.sh"

# Restart the plugin loader so the plugin re-reads the settings.
# Try pkexec (GUI auth dialog) first; fall back to a manual hint.
if pkexec systemctl restart plugin_loader 2>/dev/null; then
  echo "plugin_loader restarted."
else
  echo ""
  echo "Restart plugin_loader to pick up the change:"
  echo "  sudo systemctl restart plugin_loader"
fi
