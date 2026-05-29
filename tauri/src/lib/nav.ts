// Centralised navigation helper — picks window-spawn (desktop) or in-app
// routing (touch) based on the resolved UI mode. Caller doesn't need to
// know which strategy is active.
import { goto } from '$app/navigation';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { uiMode } from './uiMode.svelte';

type View = 'settings' | 'add' | 'browse' | 'edit';

const WINDOWS: Record<View, {
  url: string; title: string;
  width: number; height: number;
  minWidth: number; minHeight: number;
}> = {
  settings: { url: '/settings', title: 'Spool — Settings', width: 1180, height: 760, minWidth: 900,  minHeight: 600 },
  add:      { url: '/add',      title: 'Add Game · Spool', width: 720,  height: 560, minWidth: 600,  minHeight: 480 },
  browse:   { url: '/browse',   title: 'Browse Games',     width: 1280, height: 800, minWidth: 1100, minHeight: 600 },
  edit:     { url: '/edit',     title: 'Edit · Spool',     width: 720,  height: 560, minWidth: 600,  minHeight: 480 },
};

/** Open a named view. On touch: routes in-app via goto(). On desktop:
 *  spawns a decorations-free child window (focuses it if already open). */
export async function openView(view: View, params?: Record<string, string>): Promise<void> {
  if (uiMode.resolved === 'touch') {
    const qs = params ? '?' + new URLSearchParams(params).toString() : '';
    await goto(WINDOWS[view].url + qs);
    return;
  }
  const existing = await WebviewWindow.getByLabel(view);
  if (existing) {
    await existing.setFocus();
    return;
  }
  const w = WINDOWS[view];
  const qs = params ? '?' + new URLSearchParams(params).toString() : '';
  new WebviewWindow(view, {
    url: w.url + qs,
    title: w.title,
    width: w.width,
    height: w.height,
    minWidth: w.minWidth,
    minHeight: w.minHeight,
    decorations: false,
    resizable: true,
    center: true,
    backgroundColor: '#0b0c0e',
  });
}
