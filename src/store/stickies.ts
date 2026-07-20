import { create } from "zustand";
import { emit } from "@tauri-apps/api/event";
import {
  type Sticky,
  getAllStickies,
  createSticky as bridgeCreateSticky,
  updateSticky as bridgeUpdateSticky,
  deleteSticky as bridgeDeleteSticky,
  openStickyWindow,
  getCurrentDesktopId,
  setStickyDesktops,
  updateStickyWindowState as bridgeUpdateWindowState,
} from "../lib/tauri-bridge";

/** The parts of a sticky that describe its window rather than its content. */
export type WindowState = Partial<
  Pick<Sticky, "position_x" | "position_y" | "width" | "height" | "is_open">
>;
import {
  applyStickyUpdate,
  debounce,
  STICKY_UPDATED_EVENT,
  type StickyUpdatePayload,
} from "../lib/sticky-sync";

interface StickiesState {
  stickies: Map<string, Sticky>;
  loaded: boolean;
  loadStickies: () => Promise<void>;
  createSticky: (color?: string) => Promise<Sticky>;
  updateStickyContent: (id: string, content: string) => Promise<void>;
  updateStickyMeta: (id: string, partial: Partial<Sticky>) => Promise<void>;
  /** Window state - position, size, openness. Does not count as an edit. */
  updateWindowState: (id: string, partial: WindowState) => Promise<void>;
  deleteSticky: (id: string) => Promise<void>;
  getSticky: (id: string) => Sticky | undefined;
  /** Fold in a change made in another window. */
  applyRemoteUpdate: (payload: StickyUpdatePayload) => void;
}

/**
 * Content changes are announced on a delay: the editor fires on every
 * keystroke, and one event per character would have the manager redrawing
 * constantly. Each sticky window edits exactly one sticky, so a single
 * debouncer per window is enough.
 */
const announceContentChange = debounce((payload: StickyUpdatePayload) => {
  emit(STICKY_UPDATED_EVENT, payload).catch(() => {});
}, 250);

export const useStickiesStore = create<StickiesState>((set, get) => ({
  stickies: new Map(),
  loaded: false,

  loadStickies: async () => {
    try {
      const list = await getAllStickies();
      const map = new Map<string, Sticky>();
      for (const s of list) {
        map.set(s.id, s);
        // Restore multi-desktop state in the monitor thread
        if (s.desktop_id && s.desktop_id !== "") {
          const ids = s.desktop_id.split(",");
          if (ids.length > 1 || ids[0] === "*") {
            setStickyDesktops(s.id, ids).catch(() => {});
          }
        }
      }
      set({ stickies: map, loaded: true });
    } catch (err) {
      console.error("Failed to load stickies:", err);
      set({ loaded: true });
    }
  },

  createSticky: async (color = "#fff9c4") => {
    const sticky = await bridgeCreateSticky(color);

    // Phase 2: Tag the sticky with the current virtual desktop
    try {
      const desktopId = await getCurrentDesktopId();
      if (desktopId) {
        sticky.desktop_id = desktopId;
        await bridgeUpdateSticky(sticky.id, { desktop_id: desktopId });
      }
    } catch {
      // Non-fatal: desktop detection may be unavailable
    }

    set((state) => {
      const next = new Map(state.stickies);
      next.set(sticky.id, sticky);
      return { stickies: next };
    });
    await openStickyWindow(sticky);
    return sticky;
  },

  updateStickyContent: async (id: string, content: string) => {
    const updated_at = Date.now();
    set((state) => {
      const next = new Map(state.stickies);
      const existing = next.get(id);
      if (existing) {
        next.set(id, { ...existing, content, updated_at });
      }
      return { stickies: next };
    });
    await bridgeUpdateSticky(id, { content });
    announceContentChange({ id, changes: { content, updated_at } });
  },

  updateStickyMeta: async (id: string, partial: Partial<Sticky>) => {
    const updated_at = Date.now();
    set((state) => {
      const next = new Map(state.stickies);
      const existing = next.get(id);
      if (existing) {
        next.set(id, { ...existing, ...partial, updated_at });
      }
      return { stickies: next };
    });
    await bridgeUpdateSticky(id, partial);
    // Metadata changes are occasional, so they go out immediately.
    emit(STICKY_UPDATED_EVENT, {
      id,
      changes: { ...partial, updated_at },
    } satisfies StickyUpdatePayload).catch(() => {});
  },

  deleteSticky: async (id: string) => {
    set((state) => {
      const next = new Map(state.stickies);
      next.delete(id);
      return { stickies: next };
    });
    await bridgeDeleteSticky(id);
    // Notify other windows (manager) to refresh
    await emit("stickies-changed");
  },

  getSticky: (id: string) => {
    return get().stickies.get(id);
  },

  applyRemoteUpdate: (payload: StickyUpdatePayload) => {
    set((state) => ({ stickies: applyStickyUpdate(state.stickies, payload) }));
  },

  updateWindowState: async (id: string, partial: WindowState) => {
    set((state) => {
      const next = new Map(state.stickies);
      const existing = next.get(id);
      if (existing) {
        // No updated_at: where a note sits and whether it is open are not
        // edits, and the manager sorts by updated_at.
        next.set(id, { ...existing, ...partial });
      }
      return { stickies: next };
    });
    await bridgeUpdateWindowState(id, partial);
    // Not announced either: no other window displays this.
  },
}));
