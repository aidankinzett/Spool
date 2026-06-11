# Suggested Commands

All commands run from `tauri/` unless noted.

## Frontend & Dev Server
- `bun install` — Install frontend dependencies
- `bun run tauri dev` — Dev mode: hot-reload frontend + auto-rebuild backend
- `bun run tauri build` — Release binary + NSIS installer

## Backend Checks (run from tauri/src-tauri/)
- `cargo check` — Quick check of backend code
- `cargo clippy --all-targets -- -D warnings` — Run linter (strict warnings, CI fails on any warning)
- `cargo test --all` — Run unit and integration tests (Note: only runs on Linux/WSL. Dies at launch on Windows due to manifest comctl32 issues)

## Frontend Checks (run from tauri/)
- `bun run check` — Run svelte-check
- `bun run lint` — Run ESLint
- `bun run test` — Run Vitest unit tests
- `bun run test:e2e` — Run Playwright/WebDriver E2E suite (builds app first)