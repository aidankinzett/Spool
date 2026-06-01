// Full-screen library route registered via routerHook. The QAM "Browse
// Library" button navigates here.
export const SPOOL_ROUTE = "/spool";

// Per-game detail page route. `:id` is the Spool library game ID.
export const SPOOL_GAME_ROUTE = "/spool/game/:id";

// LAN peer game detail page. `:peerAddr` is the peer's IP, `:peerPort` is its
// file-server port, `:gameId` is the game ID on the remote library.
export const SPOOL_LAN_GAME_ROUTE = "/spool/lan-game/:peerAddr/:peerPort/:gameId";
