<script lang="ts">
  /**
   * Text input — hairline border, oxide-accent focus ring, optional mono
   * font (for paths / API keys / catalog numbers), optional masking with
   * a reveal toggle.
   *
   * Two-way bound via `value`. Fires `oncommit` on blur/Enter so callers
   * can persist on commit without firing on every keystroke.
   */
  import { Eye, EyeOff } from '@lucide/svelte';

  let {
    value = $bindable(''),
    placeholder,
    mono = false,
    masked = false,
    monospace,
    readonly = false,
    full = false,
    oncommit,
    onchange,
  }: {
    value: string;
    placeholder?: string;
    /** Render input text in JetBrains Mono (paths, keys, ids). */
    mono?: boolean;
    /** Mask the value like a password; adds a reveal toggle on the right. */
    masked?: boolean;
    /** Legacy alias for `mono` — keep both readable. */
    monospace?: boolean;
    readonly?: boolean;
    full?: boolean;
    /** Fires on blur or Enter — for "save on commit" behaviour. */
    oncommit?: (value: string) => void;
    /** Fires on every keystroke. */
    onchange?: (value: string) => void;
  } = $props();

  const isMono = $derived(mono || monospace);
  let revealed = $state(false);
  let focused = $state(false);

  function commit() {
    oncommit?.(value);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      if (e.currentTarget instanceof HTMLInputElement) e.currentTarget.blur();
    }
  }
</script>

<div
  class="group inline-flex h-8 items-center gap-1 rounded-sm border bg-bg-2 px-2 text-[12.5px] transition-colors {full
    ? 'w-full'
    : ''}"
  style:border-color={focused ? 'var(--color-spool)' : 'var(--color-line-2)'}
>
  <input
    bind:value
    {placeholder}
    {readonly}
    type={masked && !revealed ? 'password' : 'text'}
    onfocus={() => (focused = true)}
    onblur={() => {
      focused = false;
      commit();
    }}
    oninput={() => onchange?.(value)}
    onkeydown={handleKeydown}
    class="min-w-0 flex-1 bg-transparent text-ink-0 outline-none placeholder:text-ink-3 {isMono
      ? 'font-mono text-[12px]'
      : ''}"
  />
  {#if masked}
    <button
      type="button"
      onclick={() => (revealed = !revealed)}
      class="inline-flex h-5 w-5 items-center justify-center rounded-xs text-ink-2 hover:text-ink-0"
      aria-label={revealed ? 'Hide' : 'Show'}
    >
      {#if revealed}
        <EyeOff size={12} />
      {:else}
        <Eye size={12} />
      {/if}
    </button>
  {/if}
</div>
