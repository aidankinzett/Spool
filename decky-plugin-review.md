# Spool Decky Plugin — Review

## Executive summary

This review covers the Spool Decky plugin and the headless plugin server that backs it — the Rust loopback HTTP server (`plugin_server.rs`), the supporting Rust subsystems it drives (`session.rs`, `suspend.rs`, `rclone.rs`, `decky_install.rs`), the Python backend (`decky/main.py`), and the TypeScript frontend (callables, components, the Steam-UI patch).

**Counts by severity** (after verifier refinement and post-review resolutions):

| Severity | Count |
|---|---|
| High | 3 |
| Medium | 6 |
| Low | 11 |
| Nit | 2 |
| Disputed (unadjudicated) | 5 |
| Resolved/Dismissed | 3 |

**Headline risks:**

1. **Restore/pull from the Decky game page can overwrite the live saves of a session running in another process.** The headless server's restore and cloud-pull paths never take the cross-process per-game run lock; their only guard is an in-process `RunState` that does not coordinate across the tray GUI, the attached `--run`, and the headless server — exactly the three processes that coexist in Game Mode. (High)
2. **A force-killed session on one device can silently revert a peer's "play here instead" takeover.** The forced-close fallback's `mark_session_pending_backup_from_config` is missing the ownership guard its in-process sibling has, so it blindly overwrites a peer's live Active marker and then force-uploads stale saves over the peer's. (High)
3. **The Steam-UI patch can throw into Steam's render instead of degrading to "no badge."** Unguarded property access on `findClassModule` results means a future Steam class-marker rename throws a `TypeError` with no surrounding try/catch — the failure mode the architecture intends to be a no-op is actually a crash. (High, refined down from the reviewers' split — see note)
4. **The forced-close safety net is gated on a fragile cross-language appid equality with no fallback or diagnostic.** This is the one check standing between a Steam force-kill and a captured save; three independently-maintained quoting/CRC paths must agree byte-for-byte. (Resolved post-review: The Rust helpers were verified to be byte-identical on calculation, and the TS side forwards Steam's assigned appid verbatim, making the silent no-op unreachable today, though the fragile coupling remains a target for deduplication/hardening.)
5. **Several non-idempotent POSTs can double-fire or spuriously fail under timeout/decode edge cases**, and the LAN-download Cancel button silently no-ops during the install-start window.

**Overall health read:** The subsystem is well-architected for its hostile environment (force-kills, suspend, multi-process, cross-device). Most findings are edge cases the design already half-anticipates, and the verifiers downgraded many from medium to low on realistic-impact grounds (loopback-only bind, single-user handheld, idempotent-in-outcome operations, embedded-recoverable payloads). The three genuinely sharp issues are the **run-lock bypass on restore/pull/install-deps** (live-save/prefix corruption across processes) and the **missing peer-ownership guard in the forced-close marker write** (silent takeover reversion + stale force-upload). The appid coupling is a latent single-point-of-failure worth hardening even though it is not reachable in current code. The Python HTTP retry/timeout handling is the recurring source of low-severity correctness drift and would benefit from one disciplined fix.

---

## Confirmed findings

### High

**Decky restore/pull endpoints bypass the cross-process per-game run lock**
`tauri/src-tauri/src/plugin_server.rs:375-409, 299-330`
`post_restore` (→ `runner::restore_save_revision_core`) and `post_pull_cloud_saves` (→ `runner::pull_cloud_saves_core`) acquire no machine-wide per-game run lock. Their desktop-command equivalents wrap the same cores in `RunState.try_acquire`, but `RunState` is an in-process `Mutex` that does not coordinate across the several Spool processes the system intentionally runs (tray GUI, attached `spool --run`, headless `spool --headless-server`). The play workflow holds the genuinely cross-process lock (`proc_lock::try_acquire_run`) for the whole session, and the wipe path takes it too — but restore/pull do not. So in Game Mode, "restore an earlier save" or "Sync now" from the Decky page while the same game is still running as an attached `--run` will run `ludusavi restore`/`restore_with_redirects` straight into the live save location with zero exclusion against the live session. The code comment at `plugin_server.rs:368-374` acknowledges the dropped lock and assumes "racing a live session is unlikely" — but Game Mode is precisely where attached `--run` and the headless server coexist.
*Fix:* Push run-lock acquisition into `restore_save_revision_core`/`pull_cloud_saves_core` so every caller (desktop command and plugin server) is covered uniformly, returning a busy error on `None`, replacing the per-process `RunState` guard with the cross-process lock.

**Forced-close fallback overwrites a peer's live session marker (missing ownership guard)**
`tauri/src-tauri/src/rclone.rs:821-831` (caller `plugin_server.rs:268`)
The Decky forced-close fallback calls `mark_session_pending_backup_from_config` before backing up. Unlike its in-process sibling `mark_session_pending_backup` (`rclone.rs:793-816`), the `_from_config` variant has no peer-takeover ownership check — it unconditionally writes a `PendingBackup` marker. Scenario: Deck A suspends mid-session (marker=suspended); Deck B does "Play here instead" (writes its own Active marker); Deck A resumes and `resume_session` correctly detects the takeover and does not clobber B's marker; Deck A's game is then force-killed, `on_app_stop` fires, and `mark_session_pending_backup_from_config` blindly overwrites Deck B's live Active marker with Deck A's `PendingBackup`. This erases the peer's live cross-device record (the steal is silently reverted in the session view) and Deck A then force-uploads its potentially-stale saves over B's. The in-process path documents exactly this case as something it must avoid; the fallback path regressed the guard.
*Fix:* Mirror the in-process ownership guard in `mark_session_pending_backup_from_config` — read the existing marker first and return early if `device_id != cfg.device_id`. Factor the guarded write into one shared helper so both paths can't drift again.

**Unguarded `findClassModule` access crashes the game-detail patch when Steam renames CSS-module markers** *(refined to High here despite both verifiers landing on Medium — see rationale)*
`decky/src/components/patch/patch-wrapper.tsx:43-44, 57-64, 124-130; decky/src/index.tsx:62-70`
`appDetailsClasses`/`appDetailsHeaderClasses` resolve at `@decky/ui` load via `findClassModule`, which returns `ClassModule | void`. If a Steam Game-Mode update renames/removes the `TopCapsule`/`HeaderLoaded` marker keys, these become `undefined`, and the patch then does unguarded property access (`appDetailsClasses.InnerContainer`, `appDetailsHeaderClasses.BoxSizerContainer`, five `Fullscreen*` keys). Accessing `.InnerContainer` on `undefined` throws `TypeError`. **Verifier corrections to incorporate:** (1) the `index.tsx:63` throw occurs during React's render phase and *is* catchable by an error boundary if Steam has one above that subtree, so "breaks the whole route" is conditional, not guaranteed; the `patch-wrapper.tsx` `useEffect` sites do escape boundaries but only fire after the `index.tsx` splice already succeeded. (2) The *exported* constants are declared as concrete `Record` types in `static-classes.d.ts`, not `| void` — so TypeScript will not flag the unguarded access and gives a false sense of safety, which strengthens the finding. (3) The "architecture map's stated intent" quote is reviewer-authored, not from the repo.
*Why High here:* Both verifiers refined to Medium on the grounds that the trigger requires a Steam class-rename and the route-break is conditional. Retained as a headline-level concern because the realistic blast radius — a single Steam UI update bricking the entire game-detail route for every user with the plugin installed, the opposite of the intended graceful "no badge" degradation — is severe even if the trigger is infrequent, and the type system actively hides it. Treat as High-priority hardening; the maintainer may reclassify to Medium if comfortable with the conditional-boundary mitigation.
*Fix:* Bail early in the patch handler and the patch-wrapper effects if `!appDetailsClasses || !appDetailsHeaderClasses`; wrap the splice handler body in try/catch returning `ret` unchanged so a structural mismatch degrades to "no badge." Log once via `console.warn`.

### Medium

**`POST /games/{id}/install-deps` bypasses the per-game run lock**
`tauri/src-tauri/src/plugin_server.rs:659-693` (caller) → `tauri/src-tauri/src/proton.rs:422-512` (core)
Same defect class as the confirmed High "restore/pull bypass the run lock," but for Wine-prefix writes. `post_install_deps` runs `umu-run winetricks` against `WINEPREFIX=prefixes/<game_id>/` without taking the `proc_lock::try_acquire_run` lock. Triggering "Install dependencies" from the QAM while the same game is running as an attached `--run` starts a second wineserver/Proton session writing into the live prefix. winetricks assumes exclusive prefix access, so concurrent use can corrupt the prefix. Severity **Medium** — real prefix-corruption risk, but the trigger requires a deliberate long-running action from the QAM mid-play.
*Fix:* Push run-lock acquisition into `install_proton_deps_core` inside `proton.rs`:
```rust
let _run_lock = crate::proc_lock::try_acquire_run(game_id)?.ok_or_else(|| {
    AppError::Other("This game is busy right now (running, or being removed) — close it and try again.".into())
})?;
```
This also ensures uniform coverage for the desktop command wrapper.

**LAN peer-proxy endpoints issue outbound HTTP to an arbitrary caller-supplied host:port with no allow-list** *(reviewers split medium/low; medium retained — see note)*
`tauri/src-tauri/src/plugin_server.rs:757-810, 824-864`
`get_lan_peer_games`, `get_lan_peer_cover`, and `post_lan_install` take `{addr}`/`{port}` straight from the request path, build `http://{addr}:{port}/...`, and issue an outbound GET via `state.http` (5s timeout) with no check that the target is a peer in `state.lan.snapshot()`. Any local process reaching the loopback server can make the headless Spool server fetch arbitrary internal URLs (other loopback ports, link-local metadata, other LAN hosts) and read the response body back — an unauthenticated outbound HTTP relay. **Verifier correction:** the loopback-only bind plus the single-user-handheld threat model (no internal services or cloud-metadata endpoint to pivot to in practice) puts realistic impact at low; the technical defect and fix are correct as written. Retained at Medium because it is a clear missing-validation defect across three handlers that is cheap to close and removes a relay primitive entirely; the maintainer may treat as Low given the threat model.
*Fix:* Look up `(addr, port)` in `state.lan.snapshot()` and reject if no discovered peer matches both the address and its advertised `file_server_port`; at minimum parse `addr` as an `IpAddr` and reject non-RFC1918/link-local/metadata targets. Apply to all three handlers.

**Forced-close fallback silently skipped when the library DB fails to open**
`tauri/src-tauri/src/plugin_server.rs:73-86, 911-915`
`serve()` falls back to an empty in-memory library when `Library::open()` fails (one error log, then `Library::open_in_memory()`). `run_backup` resolves the game via `find_id_by_name`; against an empty library this returns `None`, so the forced-close fallback logs "game not in library" and returns `{acted:true, ok:false}` *without* running the backup — the precise failure the headless server exists to prevent. **Verifier corrections:** "WAL lock contention" is the weakest cited trigger (`open()` uses `busy_timeout(5s)` + WAL, so contention waits rather than errors); the credible triggers are disk-full/permission errors, a corrupted `library.db` header, or a failed JSON import/migration. The early-return at 912 also skips `record_session_headless`, so the impact is **lost playtime + lost backup**, not just the backup. Data is not destroyed — the save remains on local disk and the unsynced rclone marker is set first — and the Decky plugin surfaces a (misleading) toast, so the accurate impact is "post-session backup + playtime skipped with misleading status," not save loss.
*Fix:* On `Library::open()` failure, either refuse to start the server (absent port file ⇔ "not running," which the existing contract handles) or set a `PluginState` flag so backup/session handlers return a distinct retryable `{ok:false, reason:'library unavailable'}` instead of the misleading "game not in library," prompting a retry. The empty-library fallback is fine for read-only endpoints, not the backup path.

**`get_steam_art`: "capsule" kind never resolves via CDN/SteamGridDB fallback** *(reviewers split medium/low)*
`tauri/src-tauri/src/plugin_server.rs:566-608`
For `kind="capsule"`, `get_steam_art` first tries the local file; when there is no local cover on disk it falls through to the live resolver, where `sgdb_kind` only remaps `"header" => "grid"` and `"capsule"` passes through unchanged. Neither `cdn_asset_for` nor `fetch_first_art` knows `"capsule"`, so `resolve_art_bytes` returns `Ok(None)` → 404 every time. The portrait the CDN exposes as `Asset::Cover` (`library_600x900_2x.jpg`) is never fetched, so a Steam game lacking a locally-cached cover never gets its portrait tile in Game Mode; the JS silently skips the 404. `hero`/`logo`/`header` all map correctly; only `capsule` is broken. **Verifier correction (severity → low):** cosmetic, best-effort, narrow precondition. Also the one-line fix restores only the CDN portrait path; to also restore the SteamGridDB portrait fallback, `fetch_first_art` (`steamgriddb.rs:493-501`) needs a matching `"cover"` arm.
*Fix:* `let sgdb_kind = match kind.as_str() { "header" => "grid", "capsule" => "cover", other => other };` plus a `"cover"` arm in `fetch_first_art`.

**Destructive-first, non-atomic Decky install with no rollback** *(both reviewers refined to low)*
`tauri/src-tauri/src/decky_install.rs:152-189`
The elevated script does `rm -rf "$PLUGINS/spool-backup"` *then* `cp -r "$SRC" ...` under `set -e`, so any `cp` failure (disk full, EACCES on an odd file, partial copy) leaves no plugin at all with no restore. **Verifier correction (severity → low):** the destroyed artifact is a 4-file payload embedded in the binary, fully recoverable by re-running the installer; no user data (saves/library/config) is at risk; the realistic trigger (disk full on a tiny copy) is uncommon. Retained in the Medium group as filed but flagged low by both verifiers — treat as low-priority hardening.
*Fix:* Copy to a sibling temp dir then atomic-`mv`: `cp -r "$SRC" "$PLUGINS/.spool-backup.new" && chown -R root:root ... && rm -rf "$PLUGINS/spool-backup" && mv "$PLUGINS/.spool-backup.new" "$PLUGINS/spool-backup"`.

**`systemctl restart plugin_loader` failure is swallowed by `|| true`**
`tauri/src-tauri/src/decky_install.rs:160, 173-188`
The privileged script ends with `systemctl restart plugin_loader 2>/dev/null || true`; under `set -e` a restart failure becomes a no-op, the script exits 0, `status.success()` is true, and `install()` returns `Ok(())`. The UI flips to "Installed," but if the loader did not restart (service masked, transient systemd error, wrong unit name on this distro) the freshly-copied plugin won't load until reboot — defeating the one-click-and-live promise, especially for the forced-close backup it enables. Both verifiers held Medium.
*Fix:* Keep copy/chown as the must-succeed core, then run the restart capturing its exit into a stdout sentinel (`if ! systemctl restart plugin_loader 2>/dev/null; then echo SPOOL_RESTART_FAILED; fi`); read child stdout in Rust and surface a non-fatal warning ("plugin copied but the loader did not restart — reboot or restart Decky"), distinct from a hard failure.

### Low

**`get_steam_art`: failed WebP transcode sends raw WebP bytes mislabeled as PNG**
`tauri/src-tauri/src/plugin_server.rs:628-646`
Local covers can be `.webp`. When `transcode_webp_to_png` fails (the `image` crate doesn't decode every WebP variant), the code logs a warning and falls through to the non-WebP branch, where `mime.contains("jpeg")` is false for `image/webp`, returning `image_type="png"` with the original untouched WebP bytes. The handler base64-encodes WebP and reports `imageType:"png"`; `SetCustomArtworkForApp` rejects WebP, so the call fails or stores a corrupt image while the contract claims PNG. The common (successful) case is fine; this only bites on decode failure, where the mislabeling guarantees a bad outcome instead of a clean 404.
*Fix:* On transcode failure, return an error (404) or fall through to the live resolver; never label undecoded WebP as PNG.

**Active-session record has no cross-process file lock (lost-update window)**
`tauri/src-tauri/src/session.rs:16-24, 104-166`
`SESSION_LOCK` is a process-local `std::sync::Mutex`, invisible to other processes. For the *same* session, the attached `spool --run` process and the headless server can interleave read-then-write of `active-session.json` and lose a field (e.g. `suspended_secs` vs `backed_up`). Atomic rename prevents truncation; the module comment asserts cross-process serialization that the in-process mutex does not provide. **Verifier correction:** the in-process suspend-watcher-vs-finish interleaving the description lists as an example cannot occur (the watcher is aborted+awaited before `finish()` reconciles); the real race is strictly the attached suspend watcher vs the separate headless process. Impact is small because the headless path reads `suspended_secs` into memory before its later write, so a lost update typically lands on a value nothing re-reads.
*Fix:* If correctness under the kill/stop race matters, guard the read-modify-write with the same OS advisory file-lock pattern as `proc_lock.rs` (a `session.lock`); otherwise document the residual window rather than asserting serialization.

**`session.rs` cross-process serialization comment is inaccurate during the Game-Mode backup phase** *(reviewers split low/nit)*
`tauri/src-tauri/src/session.rs:16-24`
The doc comment claims cross-process writers are serialized "by the attached process being dead by the time they run." That is false in the normal flow: the game exits (firing Steam's app-stop → Decky POSTs `/session/game-stopped`) while the attached `--run` workflow is still running `phase_backup → finish()`. Both processes can read-modify-write with only an in-process mutex each. No field is lost *today* because both perform idempotent same-field, same-`session_id` writes with atomic rename — but the safety argument rests on that, not on serialization. **Verifier note:** `post_game_stopped` pre-checks `rec.backed_up` and no-ops, narrowing the window further (with a TOCTOU gap); the suspend-watcher scenario is correctly hedged as hypothetical. This is a doc-correctness defect, not a functional bug.
*Fix:* Correct the comment to state that safety relies on idempotent same-field/same-id writes + atomic rename, not on the attached process being dead; optionally add a file lock if stronger guarantees are wanted.

**"Back up now" (plugin server) backs up whatever the record names, with no appid/game validation**
`tauri/src-tauri/src/plugin_server.rs:283-291`
`post_backup_now` reads the active-session record and backs up `rec.game` with no appid match and no liveness check (`session=None`, so it neither records nor clears). After a forced-close fallback whose cloud upload failed, the record persists with `backed_up=true`; the record is only cleared on a fully-reconciled game-stop, so a stale-but-present record is reachable, and "Back up now" will act on it. **Verifier correction:** the QAM Content-panel "Back up now" is correct by design (it targets the displayed "Last session" game); the bug is specific to the per-game badge-menu "Back up now," where the user is on Game B's page but Game A gets backed up. The consequence is not corrupting the wrong place — it idempotently re-backs-up Game A's unchanged local saves — but Game B is silently *not* backed up despite the explicit request, while the toast reports Game A's name. Fix should be scoped to the per-game caller.
*Fix:* Have the per-game badge-menu "Back up now" pass `game.id`/appid and validate against `rec.steam_appid` like `post_game_stopped` does, or only honor it when `rec.backed_up` is false. Leave the QAM panel's "back up the last session" behavior intact.

**Malformed/partial response on first attempt re-fires a non-idempotent POST** *(reviewers split medium/low)*
`decky/main.py:184-204`
`_request_sync` is designed so a read timeout (server still working) returns `None` without retry, while a connection-level failure retries. But `_do_request` does `resp.read()` + `json.loads(raw)` *after* the server responded; a malformed (`JSONDecodeError`, a `ValueError` subclass) or truncated (`IncompleteRead`, an `HTTPException` subclass) body is caught by the *same* broad except as a connection refusal and falls through to a retry — re-firing the POST for `/session/game-stopped`, `/session/backup-now`, `/games/{id}/restore`, `/games/{id}/install-deps`. **Verifier correction (impact lower than filed):** `/session/game-stopped` is protected by the `rec.backed_up` no-op guard, so no double backup there; malformed JSON cannot occur against the serde-serialized server (only truncation, which on loopback needs the headless process to die mid-write); the genuinely unguarded paths re-run largely idempotent operations (force-overwrite backup under a serializing lock, re-restore the same backup, re-run winetricks), so realistic harm is redundant work, not corruption.
*Fix:* On the first attempt, catch `json.JSONDecodeError`/`ValueError`/`http.client.IncompleteRead` separately from connection-level `OSError`/`HTTPException` and return `None` (server may have acted — do not retry). Note: a complete fix must also reclassify `IncompleteRead`/`HTTPException` post-response failures, which the originally-proposed one-liner leaves in the retry branch.

**Concurrent callers can double-spawn the headless server (orphaned ephemeral-port server)**
`decky/main.py:74-99`
`_ensure_server` has no mutual exclusion. Invoked from async handlers via `run_in_executor` (multi-threaded pool), two threads can both observe no live server before either sets `_server_proc`, and both call `_start_server`, which unconditionally overwrites the global. The first server binds 47650 and is orphaned (never reaped); the second falls back to ephemeral and overwrites `plugin-http-port`. **Verifier correction:** the more easily-triggered case is several `useServerBase()` consumers (peer-games, lan-view, add-to-steam-list, patch-wrapper) each firing `getServerBase()` on mount, so two `get_server_base` calls alone can race — more likely, not less.
*Fix:* Guard `_ensure_server`/`_start_server` with a `threading.Lock` across the "is it alive? → spawn → assign `_server_proc`" read-modify-write; re-check liveness inside the lock.

**`settings.json` written non-atomically**
`decky/main.py:122-125`
`_save_settings` opens in `'w'` (truncating) then `json.dump`s; a crash/kill/disk-full between truncate and flush leaves partial JSON, and `_load_settings` catches `ValueError` → `{}`, silently discarding `spool_command` and `notify`. Diverges from the Rust atomic tmp→rename convention. Settings are re-derivable, so low.
*Fix:* Write to a temp file in the same directory then `os.replace()`; optionally log a warning on `ValueError` so corruption is visible.

**LAN download Cancel during the optimistic "starting" window sends an empty `install_token`** *(filed three times — see Coverage; one root cause)*
`decky/src/components/peer-games.tsx:106-146`
`startDownload` optimistically sets `install_token = ""`, awaits `POST /lan/install`, but never reads the response body (which carries the real `{install_token}`) and only starts polling afterward. The GameRow renders Cancel as soon as `active` is non-null, so a Cancel in that window sends `{install_token: ""}`; the backend's `request_cancel` only matches the exact active token, so it returns `false` and the install keeps running — Cancel silently no-ops until the next poll updates the token. **Verifier corrections:** the empty-token window is larger than "until first poll" because `begin_install` awaits the manifest fetch (host blake3-hashes the whole game folder, up to a 300s timeout) before the POST returns, so for a large game the window is seconds-to-minutes and reliably hittable; but the transfer is *not* left permanently uncancellable — once polling supplies the real token a second Cancel works, and the poll only clears local state on terminal status. Net impact: the *first* Cancel during the starting phase is silently dropped (low). (Originally filed once as High by the ts-components lane; verifiers there held High, but the three other filings and their verifiers converge on low given the transient, self-correcting nature — adjudicate as low.)
*Fix:* Read the token from the POST response and store it before showing Cancel (`const { install_token } = await res.json(); setDownload(d => d ? {...d, install_token} : d)`), or disable Cancel while `download.install_token === ""`.

**`PeerGamesPage` terminal-state `setTimeout(setDownload(null))` never cleared on unmount** *(reviewers split low/nit)*
`decky/src/components/peer-games.tsx:84-104`
A terminal poll schedules `setTimeout(() => setDownload(null), 3000)` that the mount-effect cleanup never clears (it only clears `pollRef`). Navigating away within 3s fires `setDownload` on an unmounted component. React 18 silently no-ops this, the timer self-clears within 3s, and there's no persistent leak — so the substantive point is consistency with the file's careful `cancelled`/`pollRef` discipline.
*Fix:* Store the timer in a ref and clear it in the effect cleanup, or guard with a `cancelled` flag.

**`LanPage` shows "No Spool devices found" identically for an empty network and a server-fetch failure**
`decky/src/components/lan-view.tsx:15-39`
The `poll()` catch sets `peers` to `[]` on any fetch/parse failure, and the render treats `peers.length === 0` as the empty state. If the loopback server becomes unreachable after `base` resolved, the user sees a confident "no devices" rather than a connection error. `baseError` only covers the initial `getServerBase()` resolution.
*Fix:* Track a separate fetch-error state for `/lan/peers` (mirroring `peer-games.tsx`) and only show "No Spool devices found" when the fetch succeeded with an empty array.

**Injected `<style>` elements never removed from the Big Picture document**
`decky/src/components/patch/patch-wrapper.tsx:34-46, 112-113, 136-141; decky/src/components/patch/reel.tsx:16-22`
`ensureTitleShiftStyle`/`ensureReelKeyframes` append id-guarded `<style>` nodes to the Big Picture document head; the effect cleanup and plugin `onDismount` never remove them. After a hot-reload/reinstall, `getElementById(id)` short-circuits `ensure*`, so a stale class name baked into the title-shift node's `textContent` persists and can't be refreshed. **Verifier correction:** only the title-shift node interpolates a class name (`appDetailsHeaderClasses.BoxSizerContainer`); the reel keyframes node is a fully static string, so its leak is a benign duplicate-prevention concern, not a stale-content one.
*Fix:* Remove the injected `<style>` nodes by id in `onDismount`, or make `ensure*` idempotently overwrite an existing node's `textContent` instead of short-circuiting on presence.

**`createReactTreePatcher` reconstructed per route render, defeating its patched-component cache**
`decky/src/index.tsx:46-80`
The `RoutePatch` callback constructs a new `createReactTreePatcher` (fresh empty `caches`) and re-wraps a fresh `renderFunc` on every route render, re-running the uncached tree walk each time instead of hitting the intended de-dup cache. Functionally correct, just more work per render. **Verifier notes:** the substantive fix is hoisting `createReactTreePatcher` out of the per-render callback (restores `caches`); the symbol-tagging of `routeProps` is optional since `afterPatch` doesn't accumulate stacked patches today. (The MoonDeck/SteamGridDB precedent cited in the original fix is imprecise — those use different patching mechanisms — but the hoist remedy stands.)
*Fix:* Hoist `createReactTreePatcher` to plugin/module init so a single `patchHandler` and its caches persist across renders.

### Nit

**`BAR_HEIGHT` duplicated from `SpoolBar`'s literal height with no shared source** *(both reviewers → nit)*
`decky/src/components/patch/patch-wrapper.tsx:12, 116, 19; decky/src/components/patch/spool-bar.tsx:107`
`patch-wrapper` hardcodes `BAR_HEIGHT = 44` (driving the bar's `top` offset and `TITLE_SHIFT`) while `SpoolBar` independently sets `height: 44`, coupled only by a comment. No current malfunction (both are 44); future drift would cause cosmetic mispositioning. Preventative DRY cleanup.
*Fix:* Export one `BAR_HEIGHT` constant consumed by both.

**`onAppStop` TS callable return type omits `ok`/`reason` actually returned by the backend** *(both reviewers → nit)*
`decky/src/api/callables.ts:10-12`
Typed `{ acted: boolean; game?: string }` but Python returns the full `{acted, ok, game, reason}`. Harmless today (the only caller is fire-and-forget) but a latent contract-drift hazard. **Verifier note:** the real payload also includes `cloud_synced`, so a widened type should include it too.
*Fix:* Widen to `{ acted: boolean; ok?: boolean; game?: string; reason?: string; cloud_synced?: boolean }`.

---

## Disputed findings

These have conflicting verdicts; the maintainer should adjudicate.

- **[RESOLVED - NOT A BUG TODAY] `post_game_stopped` appid mismatch silently no-ops the fallback with no game-name fallback** (`plugin_server.rs:248-258`) — One verifier: real latent fragility (three quoting/CRC paths must agree; silent no-op on drift), severity low. Other verifier: not a real bug — all three appids reduce to a single source of truth (`spool_executable()` + name, identically quoted), and the appid check is a deliberate disambiguator because `RegisterForAppLifetimeNotifications` fires for *every* game stop. **Adjudication:** Verified post-review that the silent no-op is not reachable in the normal flow. `session::compute_steam_appid` and `steam::compute_shortcut_app_id` are byte-identical. The TS side forwards Steam's assigned appid verbatim, so it can only mismatch if Steam's stored shortcut appid ≠ the stamped one (which reduces to the Rust equality just proven equal).

- **Suspend marker may not land before freeze if the delay inhibitor can't be acquired** (`suspend.rs:87-91, 118-120, 137-143, 166`) — One verifier: real mechanism, but severity low because a stale Active marker and a suspended marker both classify to `UnsyncedElsewhere` (proven by existing tests); the only divergence is ≤180s of harder "Already playing" warning wording, then they converge. Other verifier: not a real bug for the same reason — the staleness fallback lands on the intended state, no marker is ever silently reclaimed. **Hinge:** cosmetic transient warning-wording difference vs. a meaningful loss of suspend semantics.

- **Final asleep span lost (playtime inflated) if the watcher is aborted before processing resume** (`suspend.rs:139, 147-154, 172`) — One verifier: real but nit; one of the two cited triggers is impossible (`record_suspended_secs` is synchronous and runs before the `resume_session().await`, so an abort there can't drop the span); only an abort at `signal.next().await` with an open span loses it, bounded by post-thaw signal latency. Other verifier: not a real bug — same refutation of the resume-await scenario, and the only valid window is unreachable on real hardware. **Hinge:** is the narrow `signal.next().await` window worth a flush-on-abort, or non-actionable?

- **`_stop_server` deletes the published port file even when an untracked live server owns it** (`decky/main.py:289-305`) — One verifier: not a real bug — the headline path is gated out by the `_server_proc is None` early return, the removed file normally belongs to the just-stopped server, and the client treats file presence as advisory + self-heals; nit at most. Other verifier: real but the practical harm is dominated by the separate double-spawn issue; refine to nit. **Hinge:** essentially agreed it's at most a nit; both attribute real harm to the double-spawn finding instead.

- **`useServerBase` resolves once and never retries** (`decky/src/hooks/use-server-base.ts:11-27`) — One verifier: real, severity low — no retry on a genuine null, recoverable by re-navigating; but likelihood is overstated since `get_server_base`→`_ensure_server` actively starts the server and waits up to 12s. Other verifier: not a real bug — the error is per-mount (QAM/badge/routes remount on next open), and the cited trigger doesn't actually yield `null` because the plugin starts the server itself. **Hinge:** is the no-retry-on-genuine-null gap worth a bounded retry, or fully masked by per-mount remounting + on-demand server start?

- **`spool_backup_finished` emitted with 5 args, listener consumes only `appid`** (`decky/src/index.tsx:101-110`) — One verifier: real nit (untyped listener silently drops `acted/ok/game/reason`; success/failure routed via the separate `spool_backup_toast`). Other verifier: not a real bug — passing extra args to a JS function is idiomatic and harmless; the author concedes it works today. **Hinge:** document-the-contract nit vs. non-issue.

- **QAM "Back up now" vs game-page badge backup are divergent UX paths** (`decky/src/components/content.tsx:73-100`) — One verifier: real low — the QAM path has no appid to drive the spinner store, so a multi-minute op shows only a toast + disabled button. Other verifier: not a real bug — the divergence is structural (no appid in QAM, no game-page context), the disabled button *is* persistent feedback, and the suggested "use status.appid" fix isn't actionable. **Hinge:** worth aligning the two surfaces vs. an intentional, non-actionable difference.

- **[RESOLVED - NOT A BUG TODAY (DUPLICATE)] Decode error on first HTTP attempt retried, firing a duplicate non-idempotent POST** (`decky/main.py:184-204`) — Note this overlaps the *confirmed* "malformed/partial response" finding above. One verifier: real but severity low and the proposed fix is incomplete. Other verifier: not a real bug as framed — axum `Json` always sets Content-Length, so truncation surfaces as `IncompleteRead` (an `HTTPException`, not a decode error) and the pure-`JSONDecodeError` path is unreachable against this server. **Adjudication:** Resolved together with the confirmed finding. The confirmed version captures the actionable core (handle post-response failures as no-retry).

- **[RESOLVED - LATENT-BUT-SAFE, NOT A BUG TODAY] Forced-close fallback appid match is a fragile cross-language coupling with no name fallback** (`plugin_server.rs:248-258`) — Same code as the first disputed item. One verifier: real, refined to low and recommends a shared helper + a test. Other verifier: not a real bug — the two Rust helpers are byte-identical. **Adjudication:** Resolved as latent-but-safe (not a bug today) because the helpers are byte-identical. Deduplication and adding a cross-helper equality test remain recommended to mitigate future drift.

---

## Large-scale refactoring

### Worthwhile

**~~Split `plugin_server.rs` into a router module + per-concern handler submodules~~ ✓ Done — PR #405**
*Scope:* `plugin_server.rs` (1009 lines, 25 handlers across six concerns sharing only `PluginState`).
*Current problem:* The route table sits ~800 lines from handlers like `run_backup`; adding/finding a route means scrolling a 1000-line file.
*Proposed approach:* Convert to a `plugin_server/` directory — `mod.rs` keeps `PluginState`, `serve()`, the router/route table, and bind/port-file logic; move handlers into cohesive submodules (`session`, `library`, `saves`, `steam`, `proton`, `lan`) plus their private helpers (`run_backup`, the image helpers, `PEER_PROXY_TIMEOUT`). No logic moves; `PluginState` stays `pub(crate)`.
*Effort / risk / payoff:* Medium / low / high. Verified safe: the module's only outward symbol is `serve()` (one caller in `headless.rs`); every other item is file-private; the sole cross-submodule dependency is `PluginState`. The proposed submodule boundaries align exactly with the real call graph (each private helper is used by one concern). The `#![cfg(unix)]` gate moves to `mod.rs` cleanly (the caller already gates `serve()`), and `src/lan/` is existing precedent. **Execution notes:** land it as a standalone no-logic-change commit *before* the dedup work (not interleaved); `mod.rs` needs a `use`/declaration block for all 25 handlers (compiler-caught if stale); declare submodules only from the gated `mod.rs` — don't re-`cfg` each child.

**~~Extract a ludusavi-prep helper for the resolve/`ensure_config`/`config_dir` boilerplate — scoped to four handlers~~ ✓ Done — PR #409**
*Scope:* `plugin_server.rs` — `post_pull_cloud_saves`, `get_revisions`, `post_restore`, `post_uninstall_game`.
*Current problem:* These four repeat the identical 3-step ludusavi preamble (~30 lines of exact copy) where only the log tag differs, and each hand-writes its own no-ludusavi error JSON.
*Proposed approach:* One private `ludusavi_prep() -> Result<(exe, config_dir), reason>`; each handler collapses to a `match`.
*Effort / risk / payoff:* Small / low / small-but-positive. **Scope correction:** the original proposal cited five sites including `run_backup`, but `run_backup`'s preamble is non-contiguous and *deliberately* ordered after `record_session_headless` (its `{acted:true, ok:false}` no-ludusavi shape is intentional, signaling that side effects already happened). Routing `run_backup` through the helper would either be a no-op or reorder `ensure_config` after session recording. **Do the four matching handlers only; leave `run_backup` alone and do not "converge" its error shape.**

**~~Introduce one typed loopback HTTP client to replace ~12 ad-hoc `fetch` call sites~~ ✓ Done — PR #410**
*Scope:* `decky/src/lib/server.ts` (new) + `use-spool-playtime.ts`, `add-to-steam-list.tsx`, `lan-view.tsx`, `peer-games.tsx`, `lib/launch.ts`, `lib/steam.ts`.
*Current problem:* 12 raw `fetch` sites re-implement URL building, the `res.ok` check (some omit it — `lan-view.tsx` never checks), the `as T` cast, and JSON-body boilerplate, with no analogue to the desktop app's mandated single `api.ts` wrapper. `types.ts` already mirrors every response shape but isn't bound to URLs.
*Proposed approach:* `serverGet<T>`/`serverSend<T>` plus one named method per route; base resolution stays as-is.
*Effort / risk / payoff:* Medium / low / modest. Isolated frontend TS, no cross-process/cfg/Tauri-event exposure; the `<img>` cover-loading and `plugin-http-port` liveness contract are untouched. **Scope corrections:** ~7-8 sites collapse cleanly; do *not* force-fit the special cases — keep `steam-art`'s expected-404 `continue`, preserve `add-to-steam-list`'s `Array.isArray` runtime guard inside its method (don't lose it to a blind `as T`), keep `/fold` fire-and-forget, keep `/lan/download`'s nullable body explicit. Drop the "SSRF/auditable place" justification — the peer addr is a path segment to loopback, not a security boundary.

**~~Harden the appid coupling: dedupe the computation + add a cross-helper test, plus a name/single-record fallback~~ ✓ Done — PR #416** *(synthesizing the worthwhile parts of a proposal whose Part B was rejected)*
*Scope:* `plugin_server.rs`, `session.rs`, `steam.rs`.
*Current problem:* The forced-close fallback gates on `rec.steam_appid == body.appid`; three duplicated appid computations (`session::compute_steam_appid`, `steam::compute_shortcut_app_id`, the TS `unAppID >>> 0`) must agree byte-for-byte with no test linking the Rust pair.
*Proposed approach:* (Highest leverage) have `session::compute_steam_appid` delegate to `steam::compute_shortcut_app_id` and add one test asserting `upsert_spool_shortcut`'s stamped app_id equals that helper for the same `(exe, name)` — eliminates the drift class at the source. (Belt-and-suspenders) in `post_game_stopped`, after an exact-appid miss, recompute `compute_steam_appid(current_exe, rec.game)` and proceed on match, with a `tracing::warn` so drift is observable.
*Effort / risk / payoff:* Small / low / medium. **Do not** stop freezing `steam_appid` / recompute at read time (the rejected Part B): it conflates `ActiveSession.steam_appid` with `GameEntry.steam_app_id`, which rename/remove reconcile needs frozen, and recompute-at-read merely relocates the same formula coupling. Ship the dedupe+test (the real win); keep the warn+fallback as cheap insurance.

### Considered but not recommended

- **Unify the two error-return conventions (`{ok:false}` 200 vs `StatusCode`) into one `IntoResponse` result type** — a wire-contract change that would break directly-fetched TS clients branching on `res.ok`, can't absorb the image-returning handlers or the `acted` three-way envelope, and excludes the biggest duplicated block (`run_backup`). Accept at most a scoped helper for the five same-convention handlers; reject the unification.
- **Make the local active-session record the single source of truth and project the rclone marker from it** — architecturally unsound: the record exists only for attached Game-Mode launches while the marker is driven by *every* launch on every OS, and the marker's state depends on a remote read of the peer's `device_id` that a local projector can't make. The premise ("two `started_at` for one session") is already reconciled by `seed_from_record`.
- **Drop the in-memory `SuspendedSecs` atomic; checkpoint straight to the record** — introduces a playtime regression: the atomic is per-process (this session only), but the on-disk record can be a stale leftover from a prior force-killed session, so a desktop launch of a same-named game would subtract phantom sleep. Only safe if gated on `wrote_start_this_process()`, which re-adds the plumbing the proposal claims to remove.
- **Extract a shared list-picker modal shell for the three Game-Mode modals** — overstated payoff and a leaky one-size-fits-all shell (install-deps is a multi-select `ToggleField`, the others single-select `DialogButton`). Do the cheap wins instead: one shared `SPIN_KEYFRAMES` + `<BusySpinner>`, optionally a `<PickerHeader>` for the two `DialogButton` modals.
- **Centralise backup-status spinner + result-toast plumbing into one hook** — mis-measures the duplication; the genuine repetition is two wrappers in one file (`badge-menu`'s `runBackup`/`runPull`). Extract a small local helper there instead of a new abstraction.
- **Make `badge-menu.tsx` data-driven** — the "near-identical" handlers differ across four axes (title, description, success copy, error verb), most via per-game interpolation, so the config entries must hold functions; the promised line drop isn't achievable without losing user copy, and it splits the menu into data + JSX. Extract one `confirmDestructive(...)` helper called three times instead.
- **Collapse the Python proxy; have the UI call the loopback server directly** — `base` isn't threaded to the migrated call sites (component-signature surgery), loses the transparent on-demand server restart `_ensure_server` provides, and forces re-implementing the non-idempotent retry rule in `fetch` — leaving it in two languages rather than one.
- **Generate the cross-process JSON contract types from Rust via ts-rs** — the `include_str!` embed forces Decky to build before Rust (inverting the needed codegen ordering), ts-rs's cargo-test trigger never runs on Windows CI, and 3 of 5 cited mirrors are deliberate narrower subsets full-struct export can't model. Sharing the existing hand-written `types.ts` between the GUI and Decky projects is the cheaper move.

---

## Coverage & gaps

Folding in the completeness critic honestly — several handlers and files were not reviewed, and a few findings need deeper verification:

**Unreviewed routes / handlers (`plugin_server.rs`):**
- `GET /covers/*` (`ServeDir`, line 161) — the only filesystem-serving route; never examined for `..`/symlink containment to `covers_dir()`. Relies entirely on tower-http path normalization.
- `GET /games/{id}/steam-launch-info` — no check that `entry.exe_path` still exists; returns a launch contract for a possibly-deleted exe. *(Resolved: Verified post-review that `build_launch_options` and `session::compute_steam_appid` produce identical quoted strings.)*
- `POST /games/{id}/install-deps` and `POST /games/{id}/proton` — *[Gap Closed]*: Verified post-review. `POST /games/{id}/install-deps` was confirmed to bypass the per-game run lock and has been added as a confirmed Medium finding. `POST /games/{id}/proton` was verified as benign (only writes the `proton_version_path` DB field, which is SQLite-WAL-safe).
- `POST /games/{id}/uninstall` and `DELETE /games/{id}` — whether the headless call sites take the per-game wipe lock was not verified.
- `POST /fold` — fires a synchronous cross-device rclone fold with no rate-limit/dedup; concurrent calls can overlap on a quota-limited backend. (The TS side's per-page-view fold was flagged; the server handler was not.)

**Unreviewed frontend/contract files:**
- `decky/src/api/callables.ts` — only `onAppStop`'s return-type drift was confirmed; the full sweep across all callables (`backupNow`, `pullCloudSaves` outcome enum, `restoreSaveRevision` `game_count`) vs `main.py`/server JSON is incomplete.
- `install-deps-modal.tsx`, `proton-version-modal.tsx`, `revision-picker-modal.tsx`, `add-to-steam-list.tsx`, `lan-view.tsx` (~680 LoC) — no correctness review (verb input handling, proton selection, the revision picker's interaction with the 120s-vs-180s timeout).
- `hooks/use-backing-up.ts`, `use-now.ts`, `lib/steam-chrome.ts`, `lib/format.ts` — unexamined.

**Uncovered edge cases / failure modes:**
- **~~`appid=0` / non-Spool-game stop~~ ✓ Investigated & Fixed — PR #414:** `on_app_stop` fires for *every* app stop (the lifetime listener is global), including real Steam games and appid 0, with no client-side guard before POSTing. Cost is a network op + spinner/toast churn on every unrelated game stop — touched by the toast-storm note but never its own finding.
- **Plugin version skew:** the no-handshake gap is noted, but the concrete failure (older `main.py` calling a route the newer server renamed) is never traced to a specific contract break.
- **Offline-cloud during forced-close:** whether the *headless* game-stop path leaves a recoverable `PendingBackup` marker the way the in-process workflow does was not verified.
- **Missing `umu-run` in Game Mode:** install-deps/proton routes call `proton::` cores; no dependency doctor runs on the headless path.

**Findings needing deeper verification:**
- **[Resolved]** The appid-equality coupling (one confirmed-adjacent + two disputed filings) was verified post-review. `session::compute_steam_appid` and `steam::compute_shortcut_app_id` are indeed byte-identical, settling the disputed appid items.
- "Forced-close fallback skipped on DB-open failure" was filed twice (confirmed list) — these are the same finding; dedupe.
- The LAN empty-token cancel was filed three times across lanes — same root cause; confirm the queued cancel re-fires correctly once the real token lands on both ends.

**What this means:** The two highest-value unreviewed items (the install-deps/proton run-lock gap and the `compute_steam_appid` ↔ `compute_shortcut_app_id` byte-equality) have been resolved post-review. Remaining open gaps include path-traversal containment on `GET /covers/*`, wipe-lock verification on the headless uninstall path, server-side rate-limiting on `/fold`, and a detailed correctness review of unreviewed modals and hooks.
