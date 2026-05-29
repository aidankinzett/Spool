// Single source of truth for the resolved UI density/layout mode.
//
// `init()` is called once per window at boot (in +layout.svelte) and again
// whenever the user changes the Settings control. It resolves the persisted
// `ui_mode` setting to a concrete `'desktop' | 'touch'` and writes it to
// `<html data-mode>`, which is what the PR 1 density tokens key off
// (`[data-mode='touch']` in app.css). Only the nav layer, the chrome
// wrapper, and the one library layout branch read `resolved` directly;
// everything else just inherits density from `data-mode`.
import type { UiMode } from './types';

class UiModeStore {
  /** Concrete mode the UI renders at. Defaults to desktop so first paint
   *  matches the PR 1 :root token values before init() resolves. */
  resolved = $state<'desktop' | 'touch'>('desktop');
  /** The persisted user choice (Auto/Desktop/Touch). */
  setting = $state<UiMode>('auto');

  async init(setting: UiMode) {
    this.setting = setting;
    this.resolved = setting === 'auto' ? this.detect() : setting;
    document.documentElement.dataset.mode = this.resolved;
  }

  private detect(): 'desktop' | 'touch' {
    // Pointer type is the only reliable auto signal: coarse = touchscreen-
    // primary device (Steam Deck, ROG Ally). Window size was removed —
    // innerSize() returns physical pixels, is DPI-unreliable, and the window
    // may not be at its final size when init() runs at boot.
    return matchMedia('(pointer: coarse)').matches ? 'touch' : 'desktop';
  }
}

export const uiMode = new UiModeStore();
