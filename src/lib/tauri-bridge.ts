import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import Database from "@tauri-apps/plugin-sql";
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

let dbInstance: Database | null = null;

async function getDb(): Promise<Database> {
  if (!dbInstance) {
    dbInstance = await Database.load("sqlite:sharesticky.db");
  }
  return dbInstance;
}

export async function getAllStickies(): Promise<Sticky[]> {
  const db = await getDb();
  return db.select<Sticky[]>("SELECT * FROM stickies ORDER BY updated_at DESC");
}

export async function createSticky(color: string = "#fff9c4"): Promise<Sticky> {
  const db = await getDb();
  const id = crypto.randomUUID();
  const docId = crypto.randomUUID();
  const now = Date.now();

  await db.execute(
    `INSERT INTO stickies (id, doc_id, content, color, desktop_id, position_x, position_y, width, height, pinned, is_open, sharing_tier, share_key, created_at, updated_at)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)`,
    // A newly created sticky opens immediately, so it starts out on screen.
    [id, docId, "{}", color, "", 100 + Math.random() * 200, 100 + Math.random() * 200, 250, 200, 0, 1, 0, "", now, now]
  );

  const rows = await db.select<Sticky[]>("SELECT * FROM stickies WHERE id = $1", [id]);
  return rows[0];
}

async function writeSticky(
  id: string,
  data: Partial<Sticky>,
  { stampUpdatedAt }: { stampUpdatedAt: boolean },
): Promise<void> {
  const db = await getDb();
  const fields: string[] = [];
  const values: unknown[] = [];
  let paramIdx = 1;

  for (const [key, value] of Object.entries(data)) {
    // Letting the primary key through would renumber the parameters and
    // update a different row.
    if (key === "id") continue;
    fields.push(`${key} = $${paramIdx}`);
    values.push(value);
    paramIdx++;
  }

  if (fields.length === 0) return;

  if (stampUpdatedAt) {
    fields.push(`updated_at = $${paramIdx}`);
    values.push(Date.now());
    paramIdx++;
  }

  values.push(id);
  await db.execute(
    `UPDATE stickies SET ${fields.join(", ")} WHERE id = $${paramIdx}`,
    values
  );
}

/** Record an edit: content, colour, desktop assignment. Stamps `updated_at`. */
export async function updateSticky(id: string, data: Partial<Sticky>): Promise<void> {
  return writeSticky(id, data, { stampUpdatedAt: true });
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
  return writeSticky(id, data, { stampUpdatedAt: false });
}

export async function deleteSticky(id: string): Promise<void> {
  const db = await getDb();
  await db.execute("DELETE FROM stickies WHERE id = $1", [id]);
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
