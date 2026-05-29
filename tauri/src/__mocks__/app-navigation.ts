// Stub for $app/navigation — used only by Vitest (standalone, no SvelteKit dev server).
// The real goto() is provided by SvelteKit at runtime.
import { vi } from 'vitest';
export const goto = vi.fn();
export const pushState = vi.fn();
export const replaceState = vi.fn();
export const invalidate = vi.fn();
export const preloadData = vi.fn();
export const preloadCode = vi.fn();
export const beforeNavigate = vi.fn();
export const afterNavigate = vi.fn();
export const onNavigate = vi.fn();
