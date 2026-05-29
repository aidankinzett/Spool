#!/bin/sh
# rclone `--auth-proxy` helper.
#
# rclone runs this once per login, piping a JSON blob {"user","pass","public_key"}
# on stdin. For a valid login we must print an rclone backend spec (JSON) on
# stdout; to reject, exit non-zero / print nothing.
#
# We don't make the auth decision here — we forward the credentials to the Spool
# lock server, which validates them against its accounts DB and returns the
# backend spec (a `local` remote jailed to the account's own directory). curl -f
# exits non-zero on an HTTP error (401/403), which rclone reads as a rejected
# login.
set -eu

input=$(cat)

printf '%s' "$input" | curl -sf -m 5 -X POST \
  -H 'Content-Type: application/json' \
  -H "X-Internal-Secret: ${WEBDAV_AUTH_SECRET:-}" \
  --data-binary @- \
  "${SPOOL_AUTH_URL:-http://spool-lock:47633/internal/webdav-auth}"
