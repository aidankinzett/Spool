<script lang="ts">
  /**
   * Toast stack — fixed top-right of the viewport, newest at the top.
   * Positioned below the 40px WindowChrome so it doesn't sit over the
   * drag area / window controls.
   */
  import { toasts } from '$lib/toasts.svelte';
  import Toast from './Toast.svelte';
</script>

<div
  class="pointer-events-none fixed right-4 top-[52px] z-50 flex flex-col gap-2.5"
>
  {#each toasts.items as t (t.id)}
    <div class="pointer-events-auto">
      <Toast toast={t} />
    </div>
  {/each}
</div>
