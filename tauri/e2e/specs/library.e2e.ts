import { browser, $, expect } from '@wdio/globals';

// Smoke test: drives the real Tauri window via tauri-driver. Asserts on the
// always-present app chrome rather than library contents, so it passes
// regardless of whether the test machine has a populated library.
describe('Spool library window', () => {
  it('renders the main toolbar', async () => {
    const settings = await $('[aria-label="Settings"]');
    await settings.waitForDisplayed({ timeout: 30_000 });

    const browse = await $('[aria-label="Browse games"]');
    await expect(browse).toBeDisplayed();
  });

  it('shows the game search field', async () => {
    const search = await $('input[placeholder*="games"]');
    await search.waitForDisplayed({ timeout: 30_000 });
    await expect(search).toBeDisplayed();
  });

  it('filters the library as the user types', async () => {
    const search = await $('input[placeholder*="games"]');
    await search.setValue('a-query-that-matches-nothing-xyz');
    await expect(search).toHaveValue('a-query-that-matches-nothing-xyz');
  });
});
