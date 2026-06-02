// Full-screen library route registered via routerHook. The QAM "Browse
// Library" button navigates here.
export const SPOOL_ROUTE = "/spool";

// Per-game detail page route. `:id` is the Spool library game ID.
export const SPOOL_GAME_ROUTE = "/spool/game/:id";

// LAN peers list page.
export const SPOOL_LAN_ROUTE = "/spool/lan";

// A single peer's shared games. `:peerAddr` is the peer's IP, `:peerPort` is
// its file-server port.
export const SPOOL_LAN_PEER_ROUTE = "/spool/lan/:peerAddr/:peerPort";

// LAN peer game detail page. `:gameId` is the game ID on the remote library.
export const SPOOL_LAN_GAME_ROUTE = "/spool/lan-game/:peerAddr/:peerPort/:gameId";
