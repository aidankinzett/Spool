import { useEffect, useRef, useState } from "react";
import { appDetailsClasses, appDetailsHeaderClasses } from "@decky/ui";
import { useServerBase } from "../../hooks/use-server-base";
import { useSpoolPlaytime } from "../../hooks/use-spool-playtime";
import { useBackingUp } from "../../hooks/use-backing-up";
import { useParams } from "../../lib/steam";
import { SpoolBar } from "./spool-bar";
import { ensureReelKeyframes } from "./reel";

// Height of the SpoolBar (kept in sync with the bar's own `height`), used to
// offset it up from the bottom edge of the hero capsule.
const BAR_HEIGHT = 44;
// Inset from the capsule's left/right edges, and the gap above its bottom edge.
const SIDE_INSET = "2.8vw";
const BOTTOM_GAP = 14;
// How far to lift Steam's title logo so the bar doesn't cover it. The bar
// occupies BAR_HEIGHT + BOTTOM_GAP up from the capsule's bottom edge; lifting
// the logo container by the same amount clears it with a small gap.
const TITLE_SHIFT = BAR_HEIGHT + BOTTOM_GAP;
// Attribute we toggle on the capsule to scope the title-shift CSS to our pages.
const ACTIVE_ATTR = "data-spool-bar-active";

// Inject (once) a scoped rule that lifts the hero's title logo while our bar is
// shown. The logo lives in Steam's box-sizer machinery; `BoxSizerContainer` is
// the bottom-anchored element (a direct child of the capsule) that holds it,
// regardless of where the user has positioned the logo within the capsule. A
// CSS rule rather than an inline transform so it survives Steam's React
// re-renders; scoped by `ACTIVE_ATTR` so it only affects the capsule we mark.
//
// Injected into `doc` — the capsule's ownerDocument (the Big Picture window),
// NOT the plugin's global `document` (SharedJSContext). The two differ in
// Steam; injecting into the plugin's document would put the rule in a document
// that doesn't contain the logo, so it would never apply.
function ensureTitleShiftStyle(doc: Document | null | undefined) {
  const id = "spool-title-shift";
  if (!doc || doc.getElementById(id)) return;
  const el = doc.createElement("style");
  el.id = id;
  // `!important` is required: Steam's own stylesheet sets the box-sizer
  // container's `transform` with `!important` (its base box positioning, an
  // identity translate by default), which would otherwise win over ours.
  el.textContent =
    `[${ACTIVE_ATTR}] .${appDetailsHeaderClasses.BoxSizerContainer}{` +
    `transform:translateY(-${TITLE_SHIFT}px)!important;transition:transform .12s ease;}`;
  doc.head.appendChild(el);
}

// Walk from our injected anchor to the page's hero banner element ("TopCapsule"):
// anchor → parent (InnerContainer) → the `Header` sibling → its `TopCapsule`
// child. Mirrors MoonDeck's `findTopCapsuleParent`. The anchor is spliced as the
// first child of the InnerContainer so it shares the capsule's top edge.
function findTopCapsule(anchor: HTMLElement | null): HTMLElement | null {
  const siblings = anchor?.parentElement?.children;
  if (!siblings) return null;
  let header: Element | undefined;
  for (const child of siblings) {
    if (child.className?.includes?.(appDetailsClasses.Header)) {
      header = child;
      break;
    }
  }
  if (!header) return null;
  for (const child of header.children) {
    if (child.className?.includes?.(appDetailsHeaderClasses.TopCapsule)) {
      return child as HTMLElement;
    }
  }
  return null;
}

// Badge wrapper injected into the game detail page's InnerContainer via
// afterPatch. Uses useParams to read appid from Steam's internal router —
// window.location.pathname is always '/index.html' in Steam's CEF context.
//
// Rather than sit in the page flow (which lands it over the blurred backdrop
// region Steam paints below the header), the bar renders inside a zero-height
// anchor and is absolutely positioned over the bottom of the hero capsule, so
// it overlays the sharp banner art. Same approach MoonDeck uses for its launch
// button. The library is fetched once and handed to the SpoolBar.
export function PatchWrapper() {
  const { base } = useServerBase();
  const { appid: appidStr } = useParams<{ appid: string }>();
  const appid = parseInt(appidStr ?? "0", 10);
  const { game, loading, refresh } = useSpoolPlaytime(appid, base);
  const backingUp = useBackingUp(appid);

  const anchorRef = useRef<HTMLDivElement | null>(null);
  const capsuleRef = useRef<HTMLElement | null>(null);
  // Vertical offset of the bar within the capsule (px from the capsule top), or
  // null until the capsule is measured. `hidden` tracks the fullscreen-hero
  // animation so the bar isn't left floating over the expanded banner.
  const [top, setTop] = useState<number | null>(null);
  const [hidden, setHidden] = useState(false);

  // When a backup finishes (backingUp falls back to false), re-fetch so the
  // bar swaps the spinning reel for the fresh "Synced · Nm ago" line.
  const wasBackingUp = useRef(backingUp);
  useEffect(() => {
    if (wasBackingUp.current && !backingUp) void refresh();
    wasBackingUp.current = backingUp;
  }, [backingUp, refresh]);

  // Locate the hero capsule, keep the bar pinned to its bottom edge across
  // resizes, and hide it while the fullscreen banner animation runs.
  useEffect(() => {
    const capsule = findTopCapsule(anchorRef.current);
    if (!capsule) return;
    capsuleRef.current = capsule;
    // Inject our styles into the document the bar actually renders in (the Big
    // Picture window), not the plugin's SharedJSContext `document`.
    const doc = capsule.ownerDocument;
    ensureTitleShiftStyle(doc);
    ensureReelKeyframes(doc);

    const measure = () =>
      setTop(Math.max(0, capsule.offsetHeight - BAR_HEIGHT - BOTTOM_GAP));
    measure();

    const ro = new ResizeObserver(measure);
    ro.observe(capsule);

    const mo = new MutationObserver(() => {
      const cn = capsule.className;
      const fullscreen =
        cn.includes(appDetailsHeaderClasses.FullscreenEnterStart) ||
        cn.includes(appDetailsHeaderClasses.FullscreenEnterActive) ||
        cn.includes(appDetailsHeaderClasses.FullscreenEnterDone) ||
        cn.includes(appDetailsHeaderClasses.FullscreenExitStart) ||
        cn.includes(appDetailsHeaderClasses.FullscreenExitActive);
      const aborted = cn.includes(appDetailsHeaderClasses.FullscreenExitDone);
      setHidden(fullscreen && !aborted);
      measure();
    });
    mo.observe(capsule, { attributes: true, attributeFilter: ["class"] });

    return () => {
      ro.disconnect();
      mo.disconnect();
      capsule.removeAttribute(ACTIVE_ATTR);
      capsuleRef.current = null;
    };
  }, [game?.id]);

  // Mark the capsule active (lifting its title) exactly when the bar is on
  // screen, so the logo only shifts while something covers it.
  const barVisible = !!game && top !== null && !hidden;
  useEffect(() => {
    const capsule = capsuleRef.current;
    if (!capsule) return;
    if (barVisible) capsule.setAttribute(ACTIVE_ATTR, "");
    else capsule.removeAttribute(ACTIVE_ATTR);
  }, [barVisible]);

  // Keep mounting the anchor while a backup runs even before the first fetch
  // resolves, so the bar isn't gated behind `loading`/`game` — but the bar
  // itself only renders once we have a game to describe.
  if (!appid || (!backingUp && (loading || !game))) return null;

  return (
    <div
      id="spool-bar-anchor"
      ref={anchorRef}
      style={{ position: "relative", height: 0 }}
    >
      {barVisible && (
        <div
          style={{
            position: "absolute",
            top,
            left: SIDE_INSET,
            right: SIDE_INSET,
            // Above the capsule art (a later DOM sibling) but well below Steam's
            // portal-level overlays (menus, modals, the QAM).
            zIndex: 100,
          }}
        >
          <SpoolBar game={game} backingUp={backingUp} appid={appid} />
        </div>
      )}
    </div>
  );
}
