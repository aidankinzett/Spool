# Tech Stack

## Backend (Rust)
- Framework: Tauri 2
- Database: SQLite (via `sqlx` in WAL mode with `busy_timeout` for concurrent access)
- HTTP Server: `axum` (LAN sharing subsystem on port 47632 or ephemeral fallback)
- CLI/Sidecars: `ludusavi` (manifest cache, backup/restore), `rclone` (cloud sync / control plane)
- External APIs: Steam CDN, SteamGridDB (fallback), Steam Store metadata

## Frontend (SvelteKit)
- Framework: SvelteKit 5
- Language: TypeScript
- Package Manager: Bun

## Platform Targets
- Windows: Primary target (built with NSIS installer, uses launcher_stub.exe for Armoury Crate integration)
- Linux: Primary target (AppImage, Steam Deck/SteamOS gamescope/Game Mode detection, Decky Loader backup plugin helper)