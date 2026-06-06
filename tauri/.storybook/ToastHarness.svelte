<script lang="ts">
  /**
   * Story harness for ToastStack, which renders the global toast store rather
   * than props. Resets the store and pushes a sample set on mount so the stack
   * has something to show; clears it again on teardown so stories don't leak
   * toasts into one another.
   */
  import { onMount } from 'svelte';
  import { toasts, type ToastKind } from '$lib/toasts.svelte';
  import ToastStack from '$lib/components/ToastStack.svelte';

  let { which = 'mixed' }: { which?: 'mixed' | 'single' } = $props();

  const SAMPLES: Record<string, Array<Parameters<typeof toasts.show>[0]>> = {
    single: [
      { kind: 'ok', label: 'STEAM', title: 'Added to Steam', sub: 'Restart Steam to see it.', duration: 0 },
    ],
    mixed: [
      { kind: 'ok', label: 'BACKUP · DONE', title: 'Saves backed up', sub: 'The Witcher 3: Wild Hunt · 34 MB', catalog: 'SPL-0001', duration: 0 },
      { kind: 'info', label: 'COVER', title: 'Cover refreshed', sub: 'Pulled the latest from SteamGridDB.', duration: 0 },
      { kind: 'warn', label: 'CLOUD · OFFLINE', title: "Couldn't reach your remote", sub: 'Backup saved locally; will sync when you reconnect.', duration: 0 },
      {
        kind: 'bad',
        label: 'LUDUSAVI · FAILED',
        title: "Couldn't restore saves",
        sub: 'backup not found',
        duration: 0,
        cta: { label: 'Open Ludusavi', onClick: () => {} },
      },
    ],
  };

  onMount(() => {
    toasts.items.splice(0, toasts.items.length);
    for (const t of SAMPLES[which] ?? SAMPLES.mixed) toasts.show(t as Omit<(typeof toasts.items)[number], 'id' | 'createdAt'> & { kind: ToastKind });
    return () => toasts.items.splice(0, toasts.items.length);
  });
</script>

<ToastStack />
