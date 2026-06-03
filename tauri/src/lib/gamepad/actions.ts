/**
 * Svelte actions for opting a surface into gamepad / keyboard navigation.
 * See `./nav` for the engine they drive.
 */

import type { Action } from 'svelte/action';
import { pushScope, updateScope } from './nav';

export interface GamepadScopeParams {
  /** Invoked on B / Escape — typically closes the modal or pops the view. */
  onBack?: () => void;
  /** Invoked for buttons not used by built-in nav (North/West, bumpers,
   *  Start/Select…), with the gilrs button name. Bind extra actions here. */
  onButton?: (button: string) => void;
}

/**
 * Mark a container as a navigation scope. Focus is trapped inside it while it's
 * the top scope; the first focusable is focused on mount, and the parent
 * scope's focus is restored when the element unmounts.
 *
 * ```svelte
 * <div use:gamepadScope={{ onBack: close }}> … </div>
 * ```
 */
export const gamepadScope: Action<HTMLElement, GamepadScopeParams | undefined> = (node, params) => {
  const release = pushScope(node, { onBack: params?.onBack, onButton: params?.onButton });

  return {
    update(next) {
      updateScope(node, { onBack: next?.onBack, onButton: next?.onButton });
    },
    destroy() {
      release();
    },
  };
};

export interface FocusableParams {
  /** Skip this element during navigation even though it matches focusable. */
  skip?: boolean;
}

/**
 * Make a custom (non-button) element focusable by the navigation engine.
 * Native `<button>`/`<a>`/inputs are already picked up, so this is only needed
 * for things like div-based library tiles. Activation (A/Enter) fires a click,
 * so attach your handler with `onclick`.
 *
 * ```svelte
 * <div use:focusable role="button" onclick={open}> … </div>
 * ```
 */
export const focusable: Action<HTMLElement, FocusableParams | undefined> = (node, params) => {
  node.setAttribute('data-gp-focusable', '');
  if (!node.hasAttribute('tabindex')) node.setAttribute('tabindex', '-1');
  if (params?.skip) node.setAttribute('data-gp-skip', '');

  return {
    update(next) {
      if (next?.skip) node.setAttribute('data-gp-skip', '');
      else node.removeAttribute('data-gp-skip');
    },
    destroy() {
      node.removeAttribute('data-gp-focusable');
      node.removeAttribute('data-gp-skip');
    },
  };
};
