import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import Database from "@tauri-apps/plugin-sql";

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
    `INSERT INTO stickies (id, doc_id, content, color, desktop_id, position_x, position_y, width, height, pinned, sharing_tier, share_key, created_at, updated_at)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)`,
    [id, docId, "{}", color, "", 100 + Math.random() * 200, 100 + Math.random() * 200, 250, 200, 0, 0, "", now, now]
  );

  const rows = await db.select<Sticky[]>("SELECT * FROM stickies WHERE id = $1", [id]);
  return rows[0];
}

export async function updateSticky(id: string, data: Partial<Sticky>): Promise<void> {
  const db = await getDb();
  const fields: string[] = [];
  const values: unknown[] = [];
  let paramIdx = 1;

  for (const [key, value] of Object.entries(data)) {
    if (key === "id") continue;
    fields.push(`${key} = $${paramIdx}`);
    values.push(value);
    paramIdx++;
  }

  if (fields.length === 0) return;

  fields.push(`updated_at = $${paramIdx}`);
  values.push(Date.now());
  paramIdx++;

  values.push(id);
  await db.execute(
    `UPDATE stickies SET ${fields.join(", ")} WHERE id = $${paramIdx}`,
    values
  );
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
