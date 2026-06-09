const stack = $state<symbol[]>([]);

export function registerModal(id: symbol): void {
  stack.push(id);
}

export function unregisterModal(id: symbol): void {
  const i = stack.indexOf(id);
  if (i !== -1) stack.splice(i, 1);
}

/** Returns true only for the last-registered (visually topmost) modal. */
export function isTopModal(id: symbol): boolean {
  return stack.length > 0 && stack[stack.length - 1] === id;
}

/** Z-index for a registered modal; 50-based, one step per stack depth. */
export function modalZIndex(id: symbol): number {
  const i = stack.indexOf(id);
  return i === -1 ? 50 : 50 + i;
}
