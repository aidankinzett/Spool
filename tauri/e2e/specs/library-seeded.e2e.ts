import { browser, $, $$, expect } from '@wdio/globals';
import { SEED_GAMES } from '../fixtures/library.js';

// Exercises the app's primary loop against a deterministic, pre-seeded
// library.json (written by wdio.conf.ts into an isolated XDG_DATA_HOME).
//
// Isolation relies on XDG_DATA_HOME, which only the Linux build honours, so
// these specs skip elsewhere rather than read a real user's library.
describe('Spool library (seeded)', function () {
  before(function () {
    if (process.platform !== 'linux') this.skip();
  });

  const search = () => $('input[placeholder*="games"]');
  const rows = () => $$('[data-testid="game-row"]');

  it('renders every seeded game in the sidebar', async () => {
    await search().waitForDisplayed({ timeout: 30_000 });
    await expect(rows()).toBeElementsArrayOfSize(SEED_GAMES.length);

    for (const game of SEED_GAMES) {
      await expect(
        $(`[data-game-name="${game.game_name}"]`),
      ).toBeDisplayed();
    }
  });

  it('filters the list as the user types', async () => {
    await search().setValue('Alpha');
    await browser.waitUntil(async () => (await rows()).length === 1, {
      timeout: 5_000,
      timeoutMsg: 'expected exactly one row to match "Alpha"',
    });
    await expect($('[data-game-name="Fixture Game Alpha"]')).toBeDisplayed();
  });

  it('shows the detail panel for the selected game', async () => {
    // Reset the filter so any seeded game is selectable. clearValue() maps to
    // the Element Clear command — setValue('') would send an empty Send Keys
    // payload, which WebKitWebDriver rejects.
    await search().clearValue();
    await $('[data-game-name="Fixture Game Alpha"]').click();

    const title = $('[data-testid="game-title"]');
    await title.waitForDisplayed({ timeout: 5_000 });
    await expect(title).toHaveText('Fixture Game Alpha');
    await expect($('[data-testid="play-button"]')).toBeDisplayed();
  });
});
