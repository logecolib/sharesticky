import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { availableMonitors } from "@tauri-apps/api/window";

export interface Sticky {
  id: string;
  doc_id: string;
  content: string;
  color: string;
  desktop_id: string;
  position_x: number;
  position_y: number;
  width: number;
  height: number;
  pinned: number; // SQLite integer (0 or 1)
  is_open: number; // SQLite integer (0 or 1) - was this note showing?
  sharing_tier: number;
  share_key: string;
  created_at: number;
  updated_at: number;
}

// SQLite now lives in Rust (SQLCipher-encrypted); the webview reaches it only
// through these commands. Signatures are unchanged from the old direct-SQL
// bridge, so the store and its callers are untouched. Column whitelisting,
// id/timestamp generation, and the updated_at-stamping rule all moved into Rust.

export async function getAllStickies(): Promise<Sticky[]> {
  return invoke<Sticky[]>("list_stickies");
}

export async function createSticky(color: string = "#fff9c4"): Promise<Sticky> {
  return invoke<Sticky>("create_sticky", { color });
}

/** Record an edit: content, colour, desktop assignment. Stamps `updated_at`. */
export async function updateSticky(id: string, data: Partial<Sticky>): Promise<void> {
  return invoke("update_sticky", { id, patch: data });
}

/**
 * Record window state - where a note sits, how big it is, whether it is open.
 *
 * Deliberately does **not** stamp `updated_at`. The manager sorts by that, so
 * stamping it here would make merely opening or dragging a note jump its card
 * to the top of the list, under the cursor of whoever just clicked it.
 */
export async function updateStickyWindowState(
  id: string,
  data: Partial<Pick<Sticky, "position_x" | "position_y" | "width" | "height" | "is_open">>,
): Promise<void> {
  return invoke("update_sticky_window_state", { id, patch: data });
}

export async function deleteSticky(id: string): Promise<void> {
  return invoke("delete_sticky", { id });
}

/** A note's Yjs document bytes; empty array means it has none yet (seed it). */
export async function getStickyDoc(id: string): Promise<Uint8Array> {
  const bytes = await invoke<number[]>("get_sticky_doc", { id });
  return new Uint8Array(bytes);
}

/** Persist a note's Yjs document plus its derived `content` projection. */
export async function saveStickyDoc(
  id: string,
  bytes: Uint8Array,
  content: string,
): Promise<void> {
  return invoke("save_sticky_doc", { id, bytes: Array.from(bytes), content });
}

export async function openStickyWindow(sticky: Sticky): Promise<void> {
  await invoke("open_sticky_window", {
    options: {
      id: sticky.id,
      position_x: sticky.position_x,
      position_y: sticky.position_y,
      width: sticky.width,
      height: sticky.height,
      pinned: sticky.pinned === 1,
    },
  });
}

// --- Phase 2: Virtual Desktop ---

export interface DesktopInfo {
  id: string;
  name: string;
  is_current: boolean;
}

export async function listDesktops(): Promise<DesktopInfo[]> {
  return invoke<DesktopInfo[]>("list_desktops");
}

export async function getCurrentDesktopId(): Promise<string> {
  return invoke<string>("get_current_desktop_id");
}

/**
 * Bounds of every attached screen, in **logical** pixels.
 *
 * Tauri reports monitors in physical pixels while sticky geometry is stored
 * logical, so the conversion belongs here at the boundary. Mixing the two is
 * what made restored notes drift across the screen before (#12).
 */
export async function attachedScreens(): Promise<
  { x: number; y: number; width: number; height: number }[]
> {
  const monitors = await availableMonitors();
  return monitors.map((m) => ({
    x: m.position.x / m.scaleFactor,
    y: m.position.y / m.scaleFactor,
    width: m.size.width / m.scaleFactor,
    height: m.size.height / m.scaleFactor,
  }));
}

/**
 * Put a sticky's window on `desktopId` (if given) and activate it.
 *
 * Activating is what carries the user across virtual desktops; Windows has no
 * documented call to switch desktops directly.
 */
export async function placeAndFocusSticky(
  sticky: Sticky,
  desktopId?: string,
): Promise<void> {
  return invoke("place_and_focus_sticky", {
    options: {
      id: sticky.id,
      position_x: sticky.position_x,
      position_y: sticky.position_y,
      width: sticky.width,
      height: sticky.height,
      pinned: sticky.pinned === 1,
    },
    desktopId,
  });
}

export async function getStickyDesktopId(stickyId: string): Promise<string> {
  return invoke<string>("get_sticky_desktop_id", { stickyId });
}

export async function moveStickyToDesktop(stickyId: string, desktopId: string): Promise<void> {
  return invoke("move_sticky_to_desktop", { stickyId, desktopId });
}

export async function setStickyDesktops(stickyId: string, desktopIds: string[]): Promise<void> {
  return invoke("set_sticky_desktops", { stickyId, desktopIds });
}

export function onDesktopChanged(callback: (desktopId: string) => void): Promise<UnlistenFn> {
  return listen<string>("desktop-changed", (event) => callback(event.payload));
}
