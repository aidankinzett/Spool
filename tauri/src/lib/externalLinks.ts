// External-link click interception.
//
// WebKitGTK (Tauri's Linux webview) has no handler for a navigation that asks
// for a new window, so clicking an `<a href="https://…" target="_blank">` ends
// there and no browser opens (issue #493). Rather than rewriting every link
// into a button, one delegated listener on the document catches clicks that
// resolve to an off-origin http(s) URL and hands them to the backend opener,
// which spawns the host browser with the AppImage environment stripped (see
// system_open.rs). New links added later are covered without extra wiring.
//
// Same-origin hrefs are left completely alone so SvelteKit's own router keeps
// handling in-app navigation and fragment links.

import { api } from './api';

/**
 * Decide whether a click on `anchor` should leave the app.
 *
 * `origin` is the window's own origin, which differs per platform and build
 * (`http://localhost:1420` in dev, `tauri://localhost` / `http://tauri.localhost`
 * in a bundle), so it is passed in rather than assumed. Returns the absolute
 * URL to open, or `null` when the link is internal or not a web link at all.
 *
 * Reads the anchor's `href` *property*, not the attribute, so a relative href
 * is already resolved against the document before it is compared.
 */
export function externalUrlFor(
  anchor: HTMLAnchorElement,
  origin: string,
): string | null {
  const href = anchor.href;
  if (!href) return null;
  let url: URL;
  try {
    url = new URL(href, origin);
  } catch {
    return null;
  }
  // Only ordinary web links go out; the backend enforces this too.
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
  // In-app navigation (including bare `#fragment` links) resolves to our own
  // origin — that belongs to the router, not the browser.
  if (url.origin === origin) return null;
  return url.href;
}

/**
 * Start intercepting external-link clicks for this window. Returns a teardown
 * function. Called once per window from `+layout.svelte`.
 */
export function startExternalLinks(): () => void {
  const onClick = (e: MouseEvent) => {
    // Something closer to the link already handled it (a component that calls
    // preventDefault, a drag, …) — don't open a second time.
    if (e.defaultPrevented) return;
    // Left button only. Middle-click arrives as `auxclick`, and right-click
    // opens the context menu; neither reaches this listener.
    if (e.button !== 0) return;
    const target = e.target;
    if (!(target instanceof Element)) return;
    const anchor = target.closest('a[href]');
    if (!(anchor instanceof HTMLAnchorElement)) return;
    // `download` means "save this", which is not a browser navigation.
    if (anchor.hasAttribute('download')) return;

    const url = externalUrlFor(anchor, window.location.origin);
    if (!url) return;

    // Modifier-clicks are handled the same way: there are no tabs or extra
    // windows in this app, so ctrl/shift/alt-click still means "open the link",
    // and the browser it lands in decides where to put it.
    e.preventDefault();
    api.openUrl(url).catch((err) => {
      console.error('[externalLinks] failed to open', url, err);
    });
  };

  // Capture phase so a stopPropagation() inside a component (e.g. a modal that
  // swallows clicks to keep them off its backdrop) can't hide the link click.
  document.addEventListener('click', onClick, true);
  return () => document.removeEventListener('click', onClick, true);
}
