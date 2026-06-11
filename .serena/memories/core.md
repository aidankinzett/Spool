# Spool Core

Cross-platform (Windows + Linux) game library + save-management wrapper built with Tauri 2 and SvelteKit 5.

## Project Structure
- `tauri/src-tauri/` - Rust backend
- `tauri/src/` - SvelteKit frontend (routes under `tauri/src/routes/`, shared code under `tauri/src/lib/`)
- `%LOCALAPPDATA%\Spool\` (Windows) or `~/.local/share/Spool/` (Linux) - Application data directory

## Key Invariants
- Single long-lived Tauri process owns all persistence, subprocess orchestration, and workflow state. Frontend is a view only.
- Window close hides to tray; quit is only via tray menu.
- SQLite database (`library.db`) is used for multi-process safety.
- Cross-process backup lock (`proc_lock.rs`) ensures exclusive access to the shared backup tree.

## References
- For technologies, frameworks, and versions, read `mem:tech_stack`.
- For developer commands (dev, build, test, check), read `mem:suggested_commands`.
- For coding style, lock discipline, and IPC conventions, read `mem:conventions`.
- For verification checklist before finishing tasks, read `mem:task_completion`.