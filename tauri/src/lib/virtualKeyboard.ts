// KDE on-screen keyboard trigger.
//
// WebKitGTK (Tauri's Linux webview) doesn't request the Wayland text-input
// protocol when an HTML field gains focus, so KDE's keyboard never auto-shows.
// We watch focus on editable elements ourselves and ask KWin to show/hide its
// keyboard over D-Bus (the `*VirtualKeyboard` backend commands). Linux-only —
// the caller gates on platform so we don't fire pointless IPC elsewhere.

import { api } from './api';

/** Input types that get a text keyboard. Excludes the ones the OSK can't help
 *  with (checkboxes, buttons, native pickers). */
const NON_TEXT_INPUT_TYPES = new Set([
  'checkbox',
  'radio',
  'button',
  'submit',
  'reset',
  'range',
  'color',
  'file',
  'image',
]);

function isEditable(node: EventTarget | null): boolean {
  if (!(node instanceof HTMLElement)) return false;
  if (node.tagName === 'TEXTAREA') return true;
  if (node.tagName === 'INPUT') {
    return !NON_TEXT_INPUT_TYPES.has((node as HTMLInputElement).type);
  }
  return node.isContentEditable;
}

/**
 * Start tracking focus and toggling the KDE on-screen keyboard. Returns a
 * teardown function. Safe to call once per window (each window runs +layout).
 */
export function startVirtualKeyboard(): () => void {
  // Debounce the hide so tabbing from one field to the next doesn't flap the
  // keyboard shut and open again — the focusout fires just before the next
  // focusin, so a short delay lets the re-show cancel a pending hide.
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  const onFocusIn = (e: FocusEvent) => {
    if (!isEditable(e.target)) return;
    if (hideTimer) {
      clearTimeout(hideTimer);
      hideTimer = undefined;
    }
    api.showVirtualKeyboard().catch(() => {});
  };

  const onFocusOut = (e: FocusEvent) => {
    if (!isEditable(e.target)) return;
    if (hideTimer) clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      hideTimer = undefined;
      api.hideVirtualKeyboard().catch(() => {});
    }, 150);
  };

  document.addEventListener('focusin', onFocusIn);
  document.addEventListener('focusout', onFocusOut);

  return () => {
    if (hideTimer) clearTimeout(hideTimer);
    document.removeEventListener('focusin', onFocusIn);
    document.removeEventListener('focusout', onFocusOut);
  };
}
