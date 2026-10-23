#!/bin/sh
# Copies dist/ into the installed plugin on a Decky device and reloads the
# plugin's frontend over Steam's CEF debugger (scripts/reload-frontend.mjs).
# No sudo and no plugin_loader restart, so it suits quick frontend iteration;
# backend (main.py) changes still need `bun run deploy`.
#
# Needs the plugin's dist/ to be writable by `deck`. `bun run deploy` leaves it
# that way (its `sudo rsync -a` keeps the deck-owned files from /tmp);
# `deploy:local` and installs that chown the plugin to root don't.
#
# Usage: sh scripts/deploy-dev.sh <host>
set -e
host="$1"
dest="homebrew/plugins/spool-backup/dist/"
if ! rsync -a --delete dist/ "deck@$host:$dest"; then
  echo "deploy:dev: couldn't write ~/$dest on $host as deck." >&2
  echo "Run \`bun run deploy\` once (it leaves dist/ owned by deck), or chown it to deck on the device." >&2
  exit 1
fi
node "$(dirname "$0")/reload-frontend.mjs" "$host"
