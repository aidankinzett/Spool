// Deterministic library fixtures for the seeded E2E specs.
//
// library.json on disk is a plain JSON array of GameEntry (see
// src-tauri/src/library.rs). Every Rust field is #[serde(default)], so a
// minimal subset per entry is enough — the backend fills in the rest.

export const SEED_GAMES = [
  {
    id: 'e2e-alpha',
    catalog_number: 1,
    game_name: 'Fixture Game Alpha',
    exe_path: '/tmp/spool-e2e/alpha.exe',
    safe_name: 'fixture-game-alpha',
    developer: 'Test Studio',
    description: 'A seeded fixture used by the E2E suite.',
    playtime_minutes: 125,
  },
  {
    id: 'e2e-beta',
    catalog_number: 2,
    game_name: 'Fixture Game Beta',
    exe_path: '/tmp/spool-e2e/beta.exe',
    safe_name: 'fixture-game-beta',
    developer: 'Test Studio',
    description: 'Second seeded fixture.',
    playtime_minutes: 0,
  },
] as const;

export const SEED_GAME_NAMES = SEED_GAMES.map((g) => g.game_name);
