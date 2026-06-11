# Coding Conventions

## Writing Style
- Explain how things work plainly and neutrally.
- Do not use self-congratulatory framing ("key insight", "magic is", "elegant/clever").
- State what code does and why, without praising or disparaging tools/dependencies.

## Rust Backend
- **Tauri State**: Prefer per-concern `State<T>` dependencies on commands rather than a single god state object.
- **Lock Discipline**: Never hold a `std::sync::Mutex` guard across an `.await` point. Use `tokio::sync::Mutex` if state must span an await.
- **Multi-Process Database Writes**: SQLite edits must use targeted `json_set()` updates or runtime-field-preserving `replace` to prevent clobbering.
- **File Locking**: Cross-process backup uses `acquire_backup()` advisory file lock on `backup.lock`. Per-game locks use `proc_lock::try_acquire_run`.
- **JSON Compatibility**: Carry `#[serde(default)]` at the struct level, never per-field, to allow smooth fallback on schema changes.
- **Tauri Events**: Names must be colon-namespaced (e.g. `library:changed`). Tauri 2 rejects `.` in event names.

## Frontend & IPC
- **API Wrapper**: All IPC commands must be invoked via the typed wrapper in `tauri/src/lib/api.ts`.
- **Types**: Keep TS interface mirrors in `tauri/src/lib/types.ts` synchronized with Rust serde types.