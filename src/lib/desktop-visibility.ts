// Single source of truth for "which virtual desktops does this sticky live on?".
//
// The `desktop_id` column stores one of:
//   ""                    - not assigned to any desktop (unknown / lookup failed)
//   "*"                   - pinned to every desktop
//   "{guid}[,{guid}...]"  - assigned to one or more specific desktops
//
// This rule was previously open-coded in five places with three different
// behaviours, which caused issue #3 (multi-desktop stickies dimmed as though
// they lived elsewhere). Every caller must use these helpers.

/** Marker stored in `desktop_id` meaning "show on every virtual desktop". */
export const ALL_DESKTOPS = "*";

/** Parse a stored `desktop_id` into the set of desktops it names. */
export function parseDesktopIds(desktopId: string): Set<string> {
  if (!desktopId) return new Set();
  return new Set(
    desktopId
      .split(",")
      .map((id) => id.trim())
      .filter((id) => id.length > 0),
  );
}

/** Serialize a set of desktop ids back into storage form. */
export function serializeDesktopIds(ids: Set<string>): string {
  return Array.from(ids).join(",");
}

/**
 * Is a sticky with this `desktop_id` present on `currentDesktopId`?
 *
 * A sticky pinned to all desktops is on every desktop. An unassigned sticky
 * ("") is not reported as being on any particular desktop - callers decide
 * whether "unknown" should be shown or dimmed.
 */
export function isOnDesktop(desktopId: string, currentDesktopId: string): boolean {
  const ids = parseDesktopIds(desktopId);
  if (ids.has(ALL_DESKTOPS)) return true;
  if (!currentDesktopId) return false;
  return ids.has(currentDesktopId);
}

/**
 * What clicking a sticky in the manager should do.
 *
 * `travel` means the window must be placed on that desktop before being
 * activated - Windows has no documented call to switch desktops directly, so
 * the app gets there by activating a window that lives on the target.
 */
export type Navigation =
  | { kind: "focus" }
  | { kind: "travel"; desktopId: string };

/**
 * Decide where clicking a sticky should take you.
 *
 * Errs towards staying put: we only travel when the sticky positively lives
 * somewhere else and we know where "here" is.
 */
export function navigationFor(desktopId: string, currentDesktopId: string): Navigation {
  if (!currentDesktopId) return { kind: "focus" };
  if (isOnDesktop(desktopId, currentDesktopId)) return { kind: "focus" };

  // Not here, so travel to the first desktop it names. Insertion order is
  // preserved by parseDesktopIds, which keeps the destination predictable.
  const [first] = parseDesktopIds(desktopId);
  return first ? { kind: "travel", desktopId: first } : { kind: "focus" };
}

/** The subset of a sticky this module needs in order to place it. */
export interface DesktopPlaceable {
  desktop_id: string;
  pinned?: number;
}

/**
 * Should a sticky be treated as living on the current desktop?
 *
 * Used to drive the manager's "other desktop" dimming, so it errs towards
 * "present": a sticky is only reported as absent when we positively know which
 * desktop we are on AND the sticky names desktops that exclude it.
 */
export function isStickyOnCurrentDesktop(
  sticky: DesktopPlaceable,
  currentDesktopId: string,
): boolean {
  if (!currentDesktopId) return true;
  if (!sticky.desktop_id) return true;
  if (sticky.pinned === 1) return true;
  return isOnDesktop(sticky.desktop_id, currentDesktopId);
}
