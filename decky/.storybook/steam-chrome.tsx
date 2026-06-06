// Reproductions of Steam Big Picture / Game-Mode's top header and bottom footer
// bars, so full-screen plugin stories can be checked against the chrome Steam
// stacks over every route. Metrics were read off live Game Mode through the CEF
// debugger (port 8081):
//
//   #header — 40px tall, z-index 6000, pointer-events: none, a right-aligned
//             status cluster (search, notifications, network, battery, clock,
//             focused-app avatar). Steam's real header is transparent; this
//             harness paints it opaque so content sliding under it is clipped.
//   #Footer — 42px tall, z-index 7000, rgba(0,0,0,0.5) + backdrop blur,
//             0 16px padding, controller hints MENU (left) / BACK (right).
//             Label is 12px / 700 / uppercase / 0.5px tracking / #fff; the
//             glyph sits in a dark circle.
//
// The bar heights are pinned in pixels, not rem: Big Picture's root font-size
// is a fixed 16px (verified unchanged at 1280x800, the Deck's native res), so
// the bars are absolute 40px / 42px on a real Deck regardless of viewport — px
// here reproduces that exactly even if Storybook's root font-size is restyled.
// Both bars are fixed to the canvas edges, mirroring how Steam overlays them
// above page content — so a story whose content slides under the chrome shows
// the collision here too. The controller glyphs are approximations (Steam ships
// its own glyph SVGs); the bar metrics and stacking order are faithful.
import type { ReactNode } from "react";
import type { Decorator } from "@storybook/react-vite";
import { FaMagnifyingGlass, FaBell, FaWifi, FaBatteryFull, FaBars } from "react-icons/fa6";
import { STEAM_HEADER_HEIGHT, STEAM_FOOTER_HEIGHT } from "../src/lib/steam-chrome";

const HEADER_H = STEAM_HEADER_HEIGHT;
const FOOTER_H = STEAM_FOOTER_HEIGHT;

function HeaderBar() {
  return (
    <div
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        right: 0,
        height: HEADER_H,
        zIndex: 6000,
        pointerEvents: "none",
        display: "flex",
        alignItems: "center",
        justifyContent: "flex-end",
        gap: "0.85rem",
        paddingRight: "0.9rem",
        color: "#fff",
        // The real #header is transparent; here it's an opaque bar so content
        // that slides under it is visibly clipped (the safe-area collision is
        // the whole point of rendering the chrome in stories).
        background: "#0c0e13",
      }}
    >
      <FaMagnifyingGlass size={18} />
      <FaBell size={18} />
      <FaWifi size={18} />
      <FaBatteryFull size={22} color="#6cc04a" />
      <span style={{ fontSize: "1rem", fontWeight: 500, letterSpacing: "0.3px" }}>10:27</span>
      {/* Focused-app avatar — Steam frames the active app in an accent border. */}
      <div
        style={{
          width: "1.7rem",
          height: "1.7rem",
          borderRadius: 4,
          border: "2px solid #c95ec0",
          background: "#0b0e14",
        }}
      />
    </div>
  );
}

function Hint({ glyph, label }: { glyph: ReactNode; label: string }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
      <div
        style={{
          width: "1.5rem",
          height: "1.5rem",
          borderRadius: "50%",
          background: "#2a2d34",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: "0.72rem",
          fontWeight: 700,
          color: "#fff",
        }}
      >
        {glyph}
      </div>
      <span
        style={{
          fontSize: 12,
          fontWeight: 700,
          textTransform: "uppercase",
          letterSpacing: "0.5px",
          color: "#fff",
        }}
      >
        {label}
      </span>
    </div>
  );
}

function FooterBar() {
  return (
    <div
      style={{
        position: "fixed",
        bottom: 0,
        left: 0,
        right: 0,
        height: FOOTER_H,
        zIndex: 7000,
        background: "rgba(0,0,0,0.5)",
        backdropFilter: "blur(100px)",
        WebkitBackdropFilter: "blur(100px)",
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "0 16px",
      }}
    >
      <Hint glyph={<FaBars size={11} />} label="Menu" />
      <Hint glyph="B" label="Back" />
    </div>
  );
}

/** Wraps a story with the Steam header + footer overlay bars. */
export function SteamChrome({ children }: { children: ReactNode }) {
  return (
    <>
      {children}
      <HeaderBar />
      <FooterBar />
    </>
  );
}

/** Storybook decorator form — add to a full-screen story's `decorators`. */
export const withSteamChrome: Decorator = (Story) => (
  <SteamChrome>
    <Story />
  </SteamChrome>
);
