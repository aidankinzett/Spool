import type { CSSProperties, ReactNode } from "react";
import { STEAM_HEADER_HEIGHT, STEAM_FOOTER_HEIGHT } from "../lib/steam-chrome";

// Insets full-screen route content into the band between Game Mode's top header
// and bottom footer bars. Both bars are fixed-position and overlay the page, so
// without this the first rows slide under the (opaque) header and the last rows
// under the footer's MENU/BACK hints. Pads the top by the header height and the
// bottom by the footer height (debugger-measured; see lib/steam-chrome.ts).
//
// Meant for content pages that flow top-to-bottom (the LAN peers list, a peer's
// game grid). Pages that intentionally paint full-bleed (the peer-game detail
// panel's blurred backdrop) manage their own insets instead of wrapping here.
export function SafeArea({
  children,
  style,
}: {
  children: ReactNode;
  style?: CSSProperties;
}) {
  return (
    <div
      style={{
        boxSizing: "border-box",
        minHeight: "100%",
        paddingTop: STEAM_HEADER_HEIGHT,
        paddingBottom: STEAM_FOOTER_HEIGHT,
        ...style,
      }}
    >
      {children}
    </div>
  );
}
