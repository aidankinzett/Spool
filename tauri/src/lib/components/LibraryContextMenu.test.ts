import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import type { GameEntry } from '$lib/types';
import ContextMenuHarness from './ContextMenuHarness.svelte';
import { removeGameDialog } from '$lib/removeGame.svelte';

vi.mock('$lib/api', () => ({
  api: new Proxy({}, { get: () => vi.fn(() => Promise.resolve()) }),
  assetUrl: () => '',
}));
vi.mock('$lib/toasts.svelte', () => ({
  toasts: { show: vi.fn(), push: vi.fn(), success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));
vi.mock('$lib/nav', () => ({ openView: vi.fn() }));
vi.mock('$lib/confirm.svelte', () => ({ confirmDialog: vi.fn(async () => false) }));

// The menu measures itself to clamp to the viewport; jsdom has no
// ResizeObserver, and the callback never needs to fire for these assertions.
globalThis.ResizeObserver = class {
  observe() {}
  unobserve() {}
  disconnect() {}
} as unknown as typeof ResizeObserver;

function makeGame(over: Partial<GameEntry> = {}): GameEntry {
  return {
    id: 'g1',
    catalog_number: 1,
    game_name: 'Hollow Knight',
    exe_path: 'C:/Games/HollowKnight/hk.exe',
    safe_name: 'hollow-knight',
    game_folder_path: 'C:/Games/HollowKnight',
    accent_color: '#88ccff',
    installed: true,
    ...over,
  } as GameEntry;
}

describe('LibraryContextMenu — Remove…', () => {
  beforeEach(() => removeGameDialog.close());

  it('opens the remove dialog after the menu closes itself', async () => {
    // The handler calls onclose() first, which clears the parent state that
    // feeds the `game` prop. Reading `game` afterwards must still yield the
    // game the user right-clicked, or the dialog never opens (#488).
    render(ContextMenuHarness, { props: { game: makeGame() } });

    const item = await screen.findByRole('menuitem', { name: /Remove…/ });
    item.click();

    expect(removeGameDialog.current).not.toBeNull();
    expect(removeGameDialog.current?.game.game_name).toBe('Hollow Knight');
  });

  it('opens the editor after the menu closes itself', async () => {
    // Same shape as Remove — every action handler dismisses the menu before
    // reading `game`, so they all break together (#488).
    const { openView } = await import('$lib/nav');
    render(ContextMenuHarness, { props: { game: makeGame() } });

    const item = await screen.findByRole('menuitem', { name: /Edit…/ });
    item.click();

    expect(openView).toHaveBeenCalledWith('edit', { id: 'g1' });
  });

  it('reaches the restore confirmation with the game accent', async () => {
    // `accent` is a derived over the same prop, so reading it after the menu
    // dismisses throws exactly like reading `game` does — the handler has to
    // take the colour off the captured entry instead (#488).
    const { confirmDialog } = await import('$lib/confirm.svelte');
    render(ContextMenuHarness, { props: { game: makeGame() } });

    const item = await screen.findByRole('menuitem', { name: /Restore saves…/ });
    item.click();
    await vi.waitFor(() => expect(confirmDialog).toHaveBeenCalled());

    expect(vi.mocked(confirmDialog).mock.calls[0][0]).toMatchObject({
      accent: '#88ccff',
      title: 'Restore saves from backup?',
    });
  });
});
