// Keeping windows in step with each other.
//
// Every window is its own webview with its own Zustand store, so a sticky
// edited in its own window is invisible to the manager unless we say so. The
// only channel between them is Tauri events.

import type { Sticky } from "./tauri-bridge";

/**
 * One sticky changed, and here is what changed about it.
 *
 * Deliberately carries the delta rather than being a bare "something changed"
 * ping: the editor fires on every keystroke, and a ping would make the manager
 * re-read every sticky from SQLite each time.
 */
export const STICKY_UPDATED_EVENT = "sticky-updated";

export interface StickyUpdatePayload {
  id: string;
  changes: Partial<Sticky>;
}

/** Fold an incoming change into the stickies a window already has. */
export function applyStickyUpdate(
  stickies: Map<string, Sticky>,
  { id, changes }: StickyUpdatePayload,
): Map<string, Sticky> {
  const existing = stickies.get(id);

  // An update can arrive for a sticky this window never loaded. A partial patch
  // is not enough to build one from, so ignore it rather than inventing a
  // half-populated card.
  if (!existing) return stickies;
  if (Object.keys(changes).length === 0) return stickies;

  const next = new Map(stickies);
  next.set(id, { ...existing, ...changes });
  return next;
}

export interface Debounced<A extends unknown[]> {
  (...args: A): void;
  cancel(): void;
}

/** Call `fn` once the calls stop, with whatever arguments came last. */
export function debounce<A extends unknown[]>(
  fn: (...args: A) => void,
  delayMs: number,
): Debounced<A> {
  let timer: ReturnType<typeof setTimeout> | undefined;

  const debounced = (...args: A) => {
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      fn(...args);
    }, delayMs);
  };

  debounced.cancel = () => {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  };

  return debounced;
}
