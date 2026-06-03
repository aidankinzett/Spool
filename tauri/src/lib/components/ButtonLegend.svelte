<script lang="ts">
  import GamepadButton, { type GpButton } from '$lib/components/GamepadButton.svelte';

  /**
   * Gamepad hint bar — the persistent button legend used by the touch
   * (gamepad-first) layout, in the style of console / Big-Picture UIs. Shows
   * what each face/shoulder button does in the current view.
   */
  let {
    items,
    size = 18,
  }: {
    items: { button: GpButton; label: string }[];
    size?: number;
  } = $props();
</script>

<div class="flex items-center justify-end gap-4 px-4">
  {#each items as item (item.button)}
    <span class="flex items-center gap-1.5">
      <GamepadButton button={item.button} {size} />
      <span
        class="font-mono whitespace-nowrap text-ink-2"
        style:font-size="var(--text-sm)"
        style:letter-spacing="0.02em"
      >
        {item.label}
      </span>
    </span>
  {/each}
</div>
