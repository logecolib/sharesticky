// Putting the user's working set back after a restart.
//
// The stickies table records every note, but until now nothing recorded which
// of them were *showing*, so the app always came back empty.

import type { Sticky } from "./tauri-bridge";
import { ALL_DESKTOPS, parseDesktopIds } from "./desktop-visibility";

export interface RestoreStep {
  sticky: Sticky;
  /** Desktop to place it on, or undefined to open it wherever it lands. */
  desktopId?: string;
}

/** Which stickies to reopen at startup, and where to put them. */
export function restorePlanFor(stickies: Sticky[]): RestoreStep[] {
  return stickies
    .filter((s) => s.is_open === 1)
    .map((sticky) => ({ sticky, desktopId: homeDesktopOf(sticky.desktop_id) }));
}

/**
 * The one desktop a sticky should be restored onto, if there is one.
 *
 * A sticky pinned to all desktops has no particular home and the desktop
 * monitor brings it along anyway, so naming a desktop here would only fight it.
 */
function homeDesktopOf(desktopId: string): string | undefined {
  const ids = parseDesktopIds(desktopId);
  if (ids.size === 0 || ids.has(ALL_DESKTOPS)) return undefined;

  // Insertion order is preserved, so a multi-desktop sticky lands somewhere
  // predictable rather than somewhere arbitrary.
  const [first] = ids;
  return first;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ScreenBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * Keep a restored window somewhere the user can actually reach.
 *
 * A note saved on a monitor that is no longer attached would otherwise come
 * back at coordinates off the visible desktop, which is indistinguishable from
 * having lost it.
 */
/** Does any part of this rectangle fall on this screen? */
function overlaps(rect: Rect, screen: ScreenBounds): boolean {
  return (
    rect.x < screen.x + screen.width &&
    rect.x + rect.width > screen.x &&
    rect.y < screen.y + screen.height &&
    rect.y + rect.height > screen.y
  );
}

/**
 * Keep a restored note somewhere the user can actually reach it.
 *
 * Checked against *every* attached screen, not just the primary: a note living
 * on a second monitor that is still plugged in must be left where it is.
 * Only a note that falls on no attached screen at all - because the monitor it
 * was saved on is gone - gets moved, and then onto the first screen.
 *
 * A partly-visible note is left alone; it is reachable, and moving it would be
 * more surprising than leaving it.
 */
export function ensureOnAttachedScreen(
  rect: Rect,
  screens: ScreenBounds[],
): { x: number; y: number } {
  // Monitor enumeration can fail. Opening the note where it was beats moving it
  // on the strength of nothing.
  if (screens.length === 0) return { x: rect.x, y: rect.y };

  if (screens.some((screen) => overlaps(rect, screen))) {
    return { x: rect.x, y: rect.y };
  }

  return clampToVisible(rect, screens[0]);
}

export function clampToVisible(rect: Rect, screen: ScreenBounds): { x: number; y: number } {
  const maxX = screen.x + screen.width - rect.width;
  const maxY = screen.y + screen.height - rect.height;

  return {
    // A window larger than the screen cannot satisfy both edges; prefer the
    // top-left, so at least its titlebar and close button are reachable.
    x: Math.max(screen.x, Math.min(rect.x, maxX)),
    y: Math.max(screen.y, Math.min(rect.y, maxY)),
  };
}
