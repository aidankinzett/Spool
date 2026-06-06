// LAN peers list page. The QAM "Browse LAN games" button navigates here.
export const SPOOL_LAN_ROUTE = "/spool/lan";

// A single peer's shared games. `:peerAddr` is the peer's IP, `:peerPort` is
// its file-server port. Games install in place from this list, so there's no
// separate per-game route.
export const SPOOL_LAN_PEER_ROUTE = "/spool/lan/:peerAddr/:peerPort";
