<script lang="ts">
  /**
   * Themed tri-state selection checkbox (sharp-cornered, accent fill). A
   * `<button role="checkbox">` rather than a native input so it can carry the
   * accent styling and an `aria-checked="mixed"` indeterminate state without the
   * imperative `node.indeterminate` dance native inputs need.
   *
   * `onToggle` fires on click; the click's propagation is stopped so the
   * checkbox can sit inside a clickable row without double-firing.
   */
  import { Check, Minus } from '@lucide/svelte';

  let {
    state,
    onToggle,
    label,
  }: {
    /** none = empty, some = indeterminate (partial), all = checked. */
    state: 'none' | 'some' | 'all';
    onToggle: () => void;
    label: string;
  } = $props();

  const filled = $derived(state === 'all' || state === 'some');
</script>

<button
  type="button"
  role="checkbox"
  aria-checked={state === 'some' ? 'mixed' : state === 'all'}
  aria-label={label}
  onclick={(e) => {
    e.stopPropagation();
    onToggle();
  }}
  class="flex size-4 shrink-0 items-center justify-center rounded-[3px] transition-colors"
  style:border={`1.5px solid ${filled ? 'var(--color-spool)' : 'var(--color-line-3)'}`}
  style:background={filled ? 'var(--color-spool)' : 'transparent'}
  style:color="var(--color-bg-0)"
>
  {#if state === 'all'}
    <Check size={11} strokeWidth={3} />
  {:else if state === 'some'}
    <Minus size={11} strokeWidth={3} />
  {/if}
</button>
