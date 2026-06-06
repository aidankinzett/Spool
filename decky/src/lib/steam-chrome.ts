// Steam Game-Mode / Big Picture chrome bar heights, measured off live Game Mode
// through the CEF debugger (port 8081). Big Picture's root font-size is a fixed
// 16px even at the Deck's native 1280x800, so these are absolute pixels on a
// real Deck regardless of viewport.
//
// The `#header` (top) and `#Footer` (bottom) bars are fixed-position and overlay
// page content, so a full-screen route insets its content by these amounts to
// clear them — see `components/safe-area.tsx`.
export const STEAM_HEADER_HEIGHT = 40;
export const STEAM_FOOTER_HEIGHT = 42;
