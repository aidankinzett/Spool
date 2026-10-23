<script module lang="ts">
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import type { AchievementView, GameAchievements, PlaySession } from '$lib/types';
  import PostGameSummaryCard from './PostGameSummaryCard.svelte';

  const now = Date.now();
  const HOUR = 60 * 60 * 1000;

  // A session that ended two hours ago on this device.
  const session: PlaySession = {
    session_id: 'dev-studio:story',
    device_id: 'dev-studio',
    device_name: 'Studio PC',
    game_name: 'The Witcher 3: Wild Hunt',
    started_at: new Date(now - 3.4 * HOUR).toISOString(),
    ended_at: new Date(now - 2 * HOUR).toISOString(),
    duration_secs: Math.round(1.4 * 3600),
  };

  function a(
    api_name: string,
    display_name: string,
    description: string,
    global_percent: number,
    hoursAgo: number | null,
  ): AchievementView {
    return {
      api_name,
      display_name,
      description,
      hidden: false,
      icon: null,
      icon_gray: null,
      global_percent,
      unlocked_at: hoursAgo === null ? null : Math.floor((now - hoursAgo * HOUR) / 1000),
      unlocked_on: null,
      live_at: hoursAgo === null ? null : now - hoursAgo * HOUR,
    };
  }

  function achievements(items: AchievementView[]): GameAchievements {
    return {
      appid: 292030,
      schema_source: 'steam',
      total: items.length,
      unlocked: items.filter((i) => i.unlocked_at !== null).length,
      items,
    };
  }

  const earned = achievements([
    a('ACH_FIRST', 'Contractor', 'Complete your first contract.', 71.4, 240),
    a('ACH_ALCHEMY', 'Alchemist', 'Brew 20 potions.', 38.2, 3),
    a('ACH_GWENT', 'Card Collector', 'Win 10 rounds of Gwent.', 22.9, 2.5),
    a('ACH_ENDING', 'Full Circle', 'Reach the final ending.', 4.1, 2.1),
    a('ACH_SWORDS', 'Bladesmith', 'Craft a master-tier sword.', 9.6, null),
    a('ACH_DEATHMARCH', 'Death March', 'Finish on the hardest difficulty.', 2.3, null),
  ]);

  const none = achievements([
    a('ACH_FIRST', 'Contractor', 'Complete your first contract.', 71.4, 240),
    a('ACH_SWORDS', 'Bladesmith', 'Craft a master-tier sword.', 9.6, null),
  ]);

  const { Story } = defineMeta({
    title: 'Detail/PostGameSummaryCard',
    component: PostGameSummaryCard,
    render: template,
  });
</script>

{#snippet template(args: Record<string, unknown>)}
  <div style="max-width: 760px; padding: 24px;">
    <PostGameSummaryCard
      gameId={`story-${String(args.variant)}`}
      sessions={[session]}
      data={args.data as GameAchievements}
      deviceId="dev-studio"
      accent="#c9a36f"
    />
  </div>
{/snippet}

<Story name="Unlocked achievements" args={{ variant: 'earned', data: earned }} />

<Story name="No unlocks this session" args={{ variant: 'none', data: none }} />
