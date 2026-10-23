<script module lang="ts">
  import { defineMeta } from '@storybook/addon-svelte-csf';
  import { makeAchievements, makePlaySessions } from '../../../.storybook/fixtures';
  import ActivityTimelineCard from './ActivityTimelineCard.svelte';

  const { Story } = defineMeta({
    title: 'Detail/ActivityTimelineCard',
    component: ActivityTimelineCard,
    render: template,
  });
</script>

{#snippet template()}
  <div style="max-width: 560px; padding: 24px;">
    <ActivityTimelineCard
      gameId="story"
      sessions={makePlaySessions('The Witcher 3: Wild Hunt')}
      data={makeAchievements()}
      deviceId="dev-studio"
      accent="#c9a36f"
    />
  </div>
{/snippet}

<!-- Sessions from two devices, with unlocks on this device and the Deck. -->
<Story name="Sessions and unlocks" />
