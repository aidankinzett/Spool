/**
 * Custom icon dictionary — drawn on a 16×16 viewBox.
 *
 * For most icons we use Lucide (`@lucide/svelte`) directly — it's a
 * maintained, consistent set. The only icons defined here are ones that
 * need pixel-precise alignment with our custom window chrome, where
 * Lucide's slightly different proportions and stroke would look off.
 *
 * Anything new that isn't chrome-specific: use Lucide.
 */

export type IconDef = { d: string; fill?: boolean };

const _icons = {
  // Window controls — paths centered in the 16×16 viewBox so they render
  // correctly when the Icon is placed in a flex-centered button.
  winMin: { d: 'M4 8h8' },
  winMax: { d: 'M4.5 4.5h7v7h-7z' },
  winClose: { d: 'M4.5 4.5 11.5 11.5M11.5 4.5l-7 7' },
} as const satisfies Record<string, IconDef>;

export type IconName = keyof typeof _icons;

export const icons: Record<IconName, IconDef> = _icons;
