# Task Completion Checklist

Before considering a task finished, verify the following steps:

## Verification & Compilation
- Run `cargo check` and `cargo clippy --all-targets -- -D warnings` on the backend (from `tauri/src-tauri/`).
- Run `bun run check` (svelte-check) and `bun run lint` (ESLint) on the frontend (from `tauri/`).
- Run tests: `cargo test --all` (Linux/WSL only) and `bun run test` (Vitest).

## IPC & Synchronicity
- If any Tauri commands were added or modified in the Rust backend, ensure the typed frontend API wrapper in `tauri/src/lib/api.ts` is updated.
- Ensure TypeScript interface mirrors in `tauri/src/lib/types.ts` match Rust backend structs exactly.