/**
 * App-wide gamepad / keyboard spatial navigation engine.
 *
 * The webview can't read controllers itself on Linux (no libmanette), so the
 * Rust `gamepad.rs` bridge reads the pad via gilrs and emits `gamepad:input`
 * Tauri events. This module is the frontend half: it turns those events (plus
 * arrow-key/Enter/Escape parity for desktop + keyboard-emulation layouts) into
 * focus movement over the real DOM.
 *
 * Model:
 *   - Navigation is **spatial** — a direction moves native focus to the
 *     geometrically-nearest focusable element, so it works on any layout with
 *     no per-screen route tables.
 *   - A **scope stack** traps focus. A modal pushes a scope; navigation is
 *     confined to it until it pops, then the parent scope (and its remembered
 *     focus) is restored. With no scope pushed, the whole document is navigable.
 *   - **Focus memory** per scope: re-entering a scope restores the element that
 *     was focused when you left it.
 *
 * Activation (A / Enter) dispatches a native click on the focused element, so
 * any `<button>`/`<a>`/element with an onclick just works. Back (B / Escape) is
 * delegated to the active scope's `onBack`.
 *
 * Surfaces opt in with the `gamepadScope` / `focusable` actions (see
 * `./actions`). Native `<button>`/`<a>`/inputs are focusable automatically;
 * `focusable` is only needed for custom elements (e.g. div-based tiles).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type Direction = 'up' | 'down' | 'left' | 'right';

interface Scope {
  el: HTMLElement;
  onBack?: () => void;
  /** Receives gilrs button names not consumed by built-in nav — i.e. anything
   *  other than the dpad (move), South (activate), East (back): North/West,
   *  LeftTrigger/RightTrigger (bumpers), Start/Select/Mode, etc. Lets a surface
   *  bind extra actions (X = details, Y = search, LB/RB = switch tab…). */
  onButton?: (button: string) => void;
  /** Last element focused inside this scope, restored on re-entry. */
  lastFocused?: HTMLElement;
}

interface GamepadInput {
  kind: string;
  button?: string;
  axis?: string;
  value: number;
}

const FOCUSABLE_SELECTOR = [
  '[data-gp-focusable]',
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled]):not([type="hidden"])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

const scopeStack: Scope[] = [];
let started = false;
let unlistenGamepad: UnlistenFn | undefined;
// Bumped on every start/stop so an in-flight `listen()` promise that resolves
// after teardown (or after a restart) can tell its registration is stale and
// unlisten itself instead of leaking.
let navGen = 0;

// Held-direction auto-repeat for the dpad. gilrs emits a single press/release
// (no key-repeat), so we synthesise repeat here: one move on press, then after
// an initial delay, steady repeats until release — like a keyboard's typematic.
const REPEAT_DELAY_MS = 350;
const REPEAT_INTERVAL_MS = 90;
let heldDir: Direction | null = null;
let repeatTimer: ReturnType<typeof setTimeout> | undefined;
let repeatInterval: ReturnType<typeof setInterval> | undefined;

const DPAD_DIRECTION: Record<string, Direction> = {
  DPadUp: 'up',
  DPadDown: 'down',
  DPadLeft: 'left',
  DPadRight: 'right',
};

function startRepeat(dir: Direction) {
  stopRepeat();
  heldDir = dir;
  move(dir);
  repeatTimer = setTimeout(() => {
    repeatInterval = setInterval(() => move(dir), REPEAT_INTERVAL_MS);
  }, REPEAT_DELAY_MS);
}

function stopRepeat(dir?: Direction) {
  // If a specific direction released, only stop when it's the one repeating.
  if (dir && heldDir !== dir) return;
  heldDir = null;
  if (repeatTimer) clearTimeout(repeatTimer);
  if (repeatInterval) clearInterval(repeatInterval);
  repeatTimer = undefined;
  repeatInterval = undefined;
}

/** The scope navigation is currently confined to (top of stack), or null. */
function activeScope(): Scope | null {
  return scopeStack.length ? scopeStack[scopeStack.length - 1] : null;
}

/** The container to search for focusables — the active scope, else <body>. */
function activeRoot(): HTMLElement {
  return activeScope()?.el ?? document.body;
}

/** Is the element actually rendered (has layout box, not hidden)? */
function isVisible(el: HTMLElement): boolean {
  if (el.hasAttribute('data-gp-skip')) return false;
  const rects = el.getClientRects();
  if (rects.length === 0) return false;
  const style = getComputedStyle(el);
  return style.visibility !== 'hidden' && style.display !== 'none';
}

function focusablesIn(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(isVisible);
}

function centerOf(r: DOMRect): { x: number; y: number } {
  return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
}

/**
 * Pick the best focusable in `dir` from `current`. Scoring favours elements
 * that lie squarely in the travel direction: progress along the travel axis
 * plus a penalty for drifting off it, so the natural neighbour wins over a
 * closer-but-sideways one.
 */
function bestInDirection(items: HTMLElement[], current: HTMLElement, dir: Direction): HTMLElement | null {
  const cc = centerOf(current.getBoundingClientRect());
  let best: HTMLElement | null = null;
  let bestScore = Infinity;

  for (const el of items) {
    if (el === current) continue;
    const c = centerOf(el.getBoundingClientRect());
    const dx = c.x - cc.x;
    const dy = c.y - cc.y;

    let primary: number;
    let cross: number;
    switch (dir) {
      case 'left':
        primary = -dx;
        cross = Math.abs(dy);
        break;
      case 'right':
        primary = dx;
        cross = Math.abs(dy);
        break;
      case 'up':
        primary = -dy;
        cross = Math.abs(dx);
        break;
      case 'down':
        primary = dy;
        cross = Math.abs(dx);
        break;
    }

    // Must make real progress in the travel direction.
    if (primary <= 1) continue;

    // Cross-axis drift is penalised heavily so aligned neighbours win.
    const score = primary + cross * 2;
    if (score < bestScore) {
      bestScore = score;
      best = el;
    }
  }

  return best;
}

function focusEl(el: HTMLElement) {
  el.focus({ preventScroll: true });
  el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

/** A focused range or number input absorbs dpad left/right as a value step
 *  (instead of moving focus). Up/down still navigates away — keeping a vertical
 *  list traversable while the horizontal axis tweaks the value. */
function isStepInput(el: Element | null): el is HTMLInputElement {
  return el instanceof HTMLInputElement && (el.type === 'range' || el.type === 'number');
}

/** Nudge a range/number input by one step, honouring min/max, and fire input +
 *  change so two-way bindings and commit handlers react (the keyboard does this
 *  natively; the gamepad path has to synthesise it). */
function adjustRange(el: HTMLInputElement, sign: 1 | -1) {
  const step = Number(el.step) || 1;
  const min = el.min !== '' ? Number(el.min) : -Infinity;
  const max = el.max !== '' ? Number(el.max) : Infinity;
  const cur = Number(el.value);
  const next = Math.min(max, Math.max(min, cur + sign * step));
  if (next !== cur) {
    el.value = String(next);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }
}

/** Move focus one step in `dir` within the active scope. */
function move(dir: Direction) {
  const root = activeRoot();

  // A focused slider / number input eats horizontal input as a value change;
  // up/down still navigates away. Held-repeat works for free since this runs
  // per move() tick.
  const focused = document.activeElement;
  if ((dir === 'left' || dir === 'right') && isStepInput(focused) && root.contains(focused)) {
    adjustRange(focused, dir === 'right' ? 1 : -1);
    return;
  }

  const items = focusablesIn(root);
  if (items.length === 0) return;

  const active = document.activeElement as HTMLElement | null;
  const current = active && root.contains(active) && isVisible(active) ? active : null;

  if (!current) {
    // Nothing focused in scope yet — enter at the first item.
    focusEl(items[0]);
    return;
  }

  const next = bestInDirection(items, current, dir);
  if (next) focusEl(next);
}

/** Activate the focused element (A / Enter). */
function activate() {
  const el = document.activeElement as HTMLElement | null;
  if (el && activeRoot().contains(el) && typeof el.click === 'function') {
    el.click();
  }
}

/** Back / cancel (B / Escape) — delegated to the active scope. */
function back() {
  activeScope()?.onBack?.();
}

/** Flag the current input modality on <html> so CSS can show focus rings only
 *  for gamepad/keyboard, never for mouse. */
type InputMode = 'gamepad' | 'keyboard' | 'mouse';
let inputModeState: InputMode = 'mouse';

function setInputMode(mode: InputMode) {
  inputModeState = mode;
  document.documentElement.setAttribute('data-gp-input', mode);
}

/** The most recent input modality. Surfaces use this to apply gamepad-only
 *  behaviour (e.g. select-on-focus) without affecting touch/mouse. */
export function inputMode(): InputMode {
  return inputModeState;
}

function onGamepad(p: GamepadInput) {
  if (p.kind === 'button-down') {
    const dir = p.button ? DPAD_DIRECTION[p.button] : undefined;
    if (dir) {
      setInputMode('gamepad');
      startRepeat(dir);
      return;
    }
    if (p.button === 'South') {
      // A
      setInputMode('gamepad');
      activate();
      return;
    }
    if (p.button === 'East') {
      // B
      setInputMode('gamepad');
      back();
      return;
    }
    // Any other button (North/West, bumpers, Start/Select…) → active scope.
    if (p.button) {
      setInputMode('gamepad');
      activeScope()?.onButton?.(p.button);
    }
  } else if (p.kind === 'button-up') {
    const dir = p.button ? DPAD_DIRECTION[p.button] : undefined;
    if (dir) stopRepeat(dir);
  } else if (p.kind === 'disconnected') {
    // A pad unplugged mid-hold can't send its button-up, so the repeat would run
    // forever — stop it. (#290)
    stopRepeat();
  } else if (p.kind === 'axis') {
    setInputMode('gamepad');
    // gilrs reports +Y up, +X right. Bridge only emits on a threshold crossing,
    // so the stick is single-step (no held-repeat) for now.
    if (p.axis === 'LeftStickX') move(p.value > 0 ? 'right' : 'left');
    else if (p.axis === 'LeftStickY') move(p.value > 0 ? 'up' : 'down');
  }
}

/** Editable fields keep native arrow-key behaviour (caret movement). A gamepad
 *  dpad still navigates away — only the keyboard path defers here. */
function isEditable(el: Element | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  const tag = el.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable;
}

function onKeydown(e: KeyboardEvent) {
  // Only steer focus while a scope is active (a modal/view opted in). Outside
  // a scope, leave the browser's native Tab behaviour alone.
  if (scopeStack.length === 0) return;

  // Don't hijack arrows while typing — let the caret move in text fields.
  if (isEditable(document.activeElement)) return;

  switch (e.key) {
    case 'ArrowUp':
      setInputMode('keyboard');
      move('up');
      e.preventDefault();
      break;
    case 'ArrowDown':
      setInputMode('keyboard');
      move('down');
      e.preventDefault();
      break;
    case 'ArrowLeft':
      setInputMode('keyboard');
      move('left');
      e.preventDefault();
      break;
    case 'ArrowRight':
      setInputMode('keyboard');
      move('right');
      e.preventDefault();
      break;
    // Enter/Space already activate a focused button natively; Escape is handled
    // by components' own keydown handlers (and mapped from B via `back`).
  }
}

function onPointerActivity() {
  setInputMode('mouse');
}

/** Tear down any held-dpad auto-repeat when input can no longer be trusted. The
 *  Rust bridge only forwards button-up while a Spool window holds focus, so a
 *  release that lands after the window blurs (alt-tab, game launch, tray hide)
 *  is dropped — without this the repeat keeps firing move() forever and resumes
 *  auto-moving focus on its own once the window is refocused. (#290) */
function onWindowBlur() {
  stopRepeat();
}

function onVisibilityChange() {
  if (document.hidden) stopRepeat();
}

/**
 * Start the engine. Idempotent — safe to call from `+layout.svelte` on every
 * mount. No-ops cleanly if the Tauri event bridge isn't available (Storybook,
 * unit tests): keyboard navigation still works.
 */
export function startGamepadNav() {
  if (started) return;
  started = true;
  const gen = ++navGen;

  // Default to mouse modality so programmatic focus (e.g. a view's initial
  // focus) doesn't flash a ring before the user touches a key or pad.
  setInputMode('mouse');

  listen<GamepadInput>('gamepad:input', (e) => onGamepad(e.payload))
    .then((fn) => {
      // If we were stopped (or restarted) while this promise was in flight, the
      // registration is stale — tear it down now rather than leak it.
      if (!started || gen !== navGen) {
        fn();
        return;
      }
      unlistenGamepad = fn;
    })
    .catch((err) => {
      // Non-Tauri context (Storybook/tests) — keyboard nav still works.
      console.warn('[gamepad-nav] event bridge unavailable:', err);
    });

  window.addEventListener('keydown', onKeydown, true);
  // Any pointer movement or press reverts to mouse/touch modality (so the focus
  // ring hides and gamepad-only behaviours like select-on-focus stand down).
  window.addEventListener('pointermove', onPointerActivity, { passive: true });
  window.addEventListener('pointerdown', onPointerActivity, { passive: true, capture: true });
  // A held dpad's button-up is dropped when the window blurs (the Rust bridge
  // gates input on focus), so tear the auto-repeat down on blur / tab-hide so it
  // can't run away. (#290)
  window.addEventListener('blur', onWindowBlur);
  document.addEventListener('visibilitychange', onVisibilityChange);
}

export function stopGamepadNav() {
  if (!started) return;
  started = false;
  navGen++;
  stopRepeat();
  unlistenGamepad?.();
  unlistenGamepad = undefined;
  window.removeEventListener('keydown', onKeydown, true);
  window.removeEventListener('pointermove', onPointerActivity);
  window.removeEventListener('pointerdown', onPointerActivity, true);
  window.removeEventListener('blur', onWindowBlur);
  document.removeEventListener('visibilitychange', onVisibilityChange);
}

/**
 * Push a navigation scope (typically a modal or a top-level view). Focus is
 * confined to `el` until the returned release fn is called. On push, the
 * scope's remembered focus (or its first focusable) is focused; on release,
 * the parent scope's remembered focus is restored.
 */
export function pushScope(
  el: HTMLElement,
  opts: { onBack?: () => void; onButton?: (button: string) => void } = {},
): () => void {
  // Remember where focus was so we can restore it when this scope pops.
  const parent = activeScope();
  if (parent && document.activeElement instanceof HTMLElement && parent.el.contains(document.activeElement)) {
    parent.lastFocused = document.activeElement;
  }

  // A dpad held across a scope change (e.g. a press that opens a modal) would
  // otherwise keep auto-repeating `move` into the freshly-pushed scope. Cancel
  // it so the held direction doesn't bleed across — the user re-presses inside
  // the new scope.
  stopRepeat();

  const scope: Scope = { el, onBack: opts.onBack, onButton: opts.onButton };
  scopeStack.push(scope);

  // Focus the first focusable on the next frame so the scope's content has
  // mounted. Only show a ring if the user is already in gamepad/keyboard mode.
  requestAnimationFrame(() => {
    if (activeScope() !== scope) return;
    const items = focusablesIn(el);
    // Priority: remembered focus → an explicit `data-gp-autofocus` target → first.
    const autofocus = el.querySelector<HTMLElement>('[data-gp-autofocus]');
    const target = (
      scope.lastFocused && el.contains(scope.lastFocused)
        ? scope.lastFocused
        : autofocus && isVisible(autofocus)
          ? autofocus
          : items[0]
    ) as HTMLElement | undefined;
    if (target && !el.contains(document.activeElement)) focusEl(target);
  });

  return () => releaseScope(scope);
}

function releaseScope(scope: Scope) {
  const idx = scopeStack.indexOf(scope);
  if (idx === -1) return;
  scopeStack.splice(idx, 1);

  // Stop any auto-repeat that was running under the scope being torn down, so a
  // still-held dpad doesn't keep moving focus in whatever scope is now active.
  stopRepeat();

  // Restore the parent scope's remembered focus.
  const parent = activeScope();
  if (parent?.lastFocused && parent.el.contains(parent.lastFocused) && isVisible(parent.lastFocused)) {
    focusEl(parent.lastFocused);
  }
}

/** Update handlers for the scope owning `el` (used by action `update`). */
export function updateScope(
  el: HTMLElement,
  opts: { onBack?: () => void; onButton?: (button: string) => void },
) {
  const scope = scopeStack.find((s) => s.el === el);
  if (scope) {
    scope.onBack = opts.onBack;
    scope.onButton = opts.onButton;
  }
}
