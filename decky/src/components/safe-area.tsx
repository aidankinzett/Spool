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
//
// `scroll` caps the inset band to the viewport height and turns it into a flex
// column, so a child can claim the remaining space (flex: 1, minHeight: 0,
// overflowY: scroll) and scroll its own overflow rather than letting a long
// list grow past the screen and clip under the footer. Without it the band is
// `minHeight: 100%` and grows to fit its content.
export function SafeArea({
  children,
  style,
  scroll = false,
}: {
  children: ReactNode;
  style?: CSSProperties;
  scroll?: boolean;
}) {
  return (
    <div
      style={{
        boxSizing: "border-box",
        paddingTop: STEAM_HEADER_HEIGHT,
        paddingBottom: STEAM_FOOTER_HEIGHT,
        ...(scroll
          ? { height: "100vh", display: "flex", flexDirection: "column", minHeight: 0 }
          : { minHeight: "100%" }),
        ...style,
      }}
    >
      {children}
    </div>
  );
}
