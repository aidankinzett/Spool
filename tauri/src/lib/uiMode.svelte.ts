// Single source of truth for the resolved UI density/layout mode.
//
// `init()` is called once per window at boot (in +layout.svelte) and again
// whenever the user changes the Settings control. It resolves the persisted
// `ui_mode` setting to a concrete `'desktop' | 'touch'` and writes it to
// `<html data-mode>`, which is what the PR 1 density tokens key off
// (`[data-mode='touch']` in app.css). Only the nav layer, the chrome
// wrapper, and the one library layout branch read `resolved` directly;
// everything else just inherits density from `data-mode`.
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { UiMode } from './types';

class UiModeStore {
  /** Concrete mode the UI renders at. Defaults to desktop so first paint
   *  matches the PR 1 :root token values before init() resolves. */
  resolved = $state<'desktop' | 'touch'>('desktop');
  /** The persisted user choice (Auto/Desktop/Touch). */
  setting = $state<UiMode>('auto');

  async init(setting: UiMode) {
    this.setting = setting;
    this.resolved = setting === 'auto' ? await this.detect() : setting;
    document.documentElement.dataset.mode = this.resolved;
  }

  private async detect(): Promise<'desktop' | 'touch'> {
    const coarse = matchMedia('(pointer: coarse)').matches;
    let small = false;
    try {
      const size = await getCurrentWindow().innerSize();
      small = Math.min(size.width, size.height) <= 900; // Deck/Ally panels
    } catch {
      // innerSize() can reject outside a Tauri window (e.g. browser dev) —
      // fall back to the pointer signal alone.
    }
    return coarse || small ? 'touch' : 'desktop';
  }
}

export const uiMode = new UiModeStore();
