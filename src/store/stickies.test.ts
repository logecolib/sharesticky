import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Sticky } from "../lib/tauri-bridge";

// The bridge is mocked rather than the IPC layer underneath it. mockIPC matches
// on command-name strings, so renaming a Rust command would leave every test
// green while the app was broken; mocking our own typed module means that
// mismatch fails to compile instead.
vi.mock("../lib/tauri-bridge", () => ({
  getAllStickies: vi.fn(),
  createSticky: vi.fn(),
  updateSticky: vi.fn(),
  deleteSticky: vi.fn(),
  openStickyWindow: vi.fn(),
  getCurrentDesktopId: vi.fn(),
  setStickyDesktops: vi.fn(),
  updateStickyWindowState: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(() => Promise.resolve()) }));

import { emit } from "@tauri-apps/api/event";
import {
  createSticky as bridgeCreateSticky,
  deleteSticky as bridgeDeleteSticky,
  getAllStickies,
  getCurrentDesktopId,
  openStickyWindow,
  setStickyDesktops,
  updateSticky as bridgeUpdateSticky,
  updateStickyWindowState as bridgeUpdateWindowState,
} from "../lib/tauri-bridge";
import { useStickiesStore } from "./stickies";
import { STICKY_UPDATED_EVENT } from "../lib/sticky-sync";

const DESKTOP_A = "{3F9D399E-C0CF-41D2-9743-5A229563DEDA}";
const DESKTOP_B = "{0EDBDC61-DD54-40A1-B6D9-E36E5BA42B7A}";

function sticky(overrides: Partial<Sticky> = {}): Sticky {
  return {
    id: "a",
    doc_id: "doc-a",
    content: "{}",
    color: "#fff9c4",
    desktop_id: "",
    position_x: 10,
    position_y: 20,
    width: 250,
    height: 200,
    pinned: 0,
    is_open: 0,
    sharing_tier: 0,
    share_key: "",
    created_at: 1,
    updated_at: 1,
    ...overrides,
  } as Sticky;
}

beforeEach(() => {
  vi.clearAllMocks();
  // Zustand stores are module-level singletons, so state survives between
  // tests unless it is put back.
  useStickiesStore.setState({ stickies: new Map(), loaded: false });
  vi.mocked(getCurrentDesktopId).mockResolvedValue("");
  vi.mocked(getAllStickies).mockResolvedValue([]);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("loadStickies", () => {
  it("makes the stored stickies available by id", async () => {
    vi.mocked(getAllStickies).mockResolvedValue([sticky({ id: "a" }), sticky({ id: "b" })]);

    await useStickiesStore.getState().loadStickies();

    expect(useStickiesStore.getState().getSticky("a")?.id).toBe("a");
    expect(useStickiesStore.getState().stickies.size).toBe(2);
  });

  it("marks itself loaded so windows stop waiting", async () => {
    await useStickiesStore.getState().loadStickies();

    expect(useStickiesStore.getState().loaded).toBe(true);
  });

  // A failure here used to leave every window stuck on "Loading...".
  it("still marks itself loaded when the database cannot be read", async () => {
    vi.mocked(getAllStickies).mockRejectedValue(new Error("database is locked"));

    await useStickiesStore.getState().loadStickies();

    expect(useStickiesStore.getState().loaded).toBe(true);
    expect(useStickiesStore.getState().stickies.size).toBe(0);
  });

  describe("re-registering stickies with the desktop monitor", () => {
    it("registers a sticky that lives on several desktops", async () => {
      vi.mocked(getAllStickies).mockResolvedValue([
        sticky({ id: "a", desktop_id: `${DESKTOP_A},${DESKTOP_B}` }),
      ]);

      await useStickiesStore.getState().loadStickies();

      expect(setStickyDesktops).toHaveBeenCalledWith("a", [DESKTOP_A, DESKTOP_B]);
    });

    it("registers a sticky pinned to all desktops", async () => {
      vi.mocked(getAllStickies).mockResolvedValue([sticky({ id: "a", desktop_id: "*" })]);

      await useStickiesStore.getState().loadStickies();

      expect(setStickyDesktops).toHaveBeenCalledWith("a", ["*"]);
    });

    // A sticky on exactly one desktop needs no monitoring: it simply stays put.
    it("does not register a sticky that lives on a single desktop", async () => {
      vi.mocked(getAllStickies).mockResolvedValue([sticky({ id: "a", desktop_id: DESKTOP_A })]);

      await useStickiesStore.getState().loadStickies();

      expect(setStickyDesktops).not.toHaveBeenCalled();
    });

    it("does not register an unassigned sticky", async () => {
      vi.mocked(getAllStickies).mockResolvedValue([sticky({ id: "a", desktop_id: "" })]);

      await useStickiesStore.getState().loadStickies();

      expect(setStickyDesktops).not.toHaveBeenCalled();
    });
  });
});

describe("createSticky", () => {
  beforeEach(() => {
    vi.mocked(bridgeCreateSticky).mockImplementation(async (color?: string) =>
      sticky({ id: "new", color: color ?? "#fff9c4" }),
    );
  });

  it("tags a new sticky with the desktop the user is on", async () => {
    vi.mocked(getCurrentDesktopId).mockResolvedValue(DESKTOP_A);

    const created = await useStickiesStore.getState().createSticky("#bbdefb");

    expect(created.desktop_id).toBe(DESKTOP_A);
    expect(bridgeUpdateSticky).toHaveBeenCalledWith("new", { desktop_id: DESKTOP_A });
  });

  it("opens the new sticky's window", async () => {
    await useStickiesStore.getState().createSticky();

    expect(openStickyWindow).toHaveBeenCalled();
  });

  it("keeps it in the store so the manager shows it immediately", async () => {
    await useStickiesStore.getState().createSticky();

    expect(useStickiesStore.getState().getSticky("new")).toBeDefined();
  });

  // Desktop detection is unavailable on machines without the VD registry keys,
  // and an untagged note is much better than no note.
  it("still creates the sticky when the desktop cannot be determined", async () => {
    vi.mocked(getCurrentDesktopId).mockRejectedValue(new Error("unavailable"));

    const created = await useStickiesStore.getState().createSticky();

    expect(created.id).toBe("new");
    expect(useStickiesStore.getState().getSticky("new")).toBeDefined();
  });

  it("does not tag the sticky when no desktop is reported", async () => {
    vi.mocked(getCurrentDesktopId).mockResolvedValue("");

    await useStickiesStore.getState().createSticky();

    expect(bridgeUpdateSticky).not.toHaveBeenCalled();
  });
});

describe("updateStickyContent", () => {
  beforeEach(async () => {
    vi.mocked(getAllStickies).mockResolvedValue([sticky({ id: "a", content: "old" })]);
    await useStickiesStore.getState().loadStickies();
    vi.clearAllMocks();
  });

  it("shows the new content straight away, without waiting for the write", async () => {
    await useStickiesStore.getState().updateStickyContent("a", "new");

    expect(useStickiesStore.getState().getSticky("a")?.content).toBe("new");
  });

  it("persists the change", async () => {
    await useStickiesStore.getState().updateStickyContent("a", "new");

    expect(bridgeUpdateSticky).toHaveBeenCalledWith("a", { content: "new" });
  });

  // The editor fires on every keystroke, so announcing each one would have the
  // manager redrawing constantly.
  it("does not announce every keystroke immediately", async () => {
    vi.useFakeTimers();

    await useStickiesStore.getState().updateStickyContent("a", "n");
    await useStickiesStore.getState().updateStickyContent("a", "ne");
    await useStickiesStore.getState().updateStickyContent("a", "new");

    expect(emit).not.toHaveBeenCalled();
  });

  it("announces the settled content once typing stops", async () => {
    vi.useFakeTimers();

    await useStickiesStore.getState().updateStickyContent("a", "n");
    await useStickiesStore.getState().updateStickyContent("a", "new");
    vi.advanceTimersByTime(300);

    expect(emit).toHaveBeenCalledTimes(1);
    expect(emit).toHaveBeenCalledWith(
      STICKY_UPDATED_EVENT,
      expect.objectContaining({ id: "a", changes: expect.objectContaining({ content: "new" }) }),
    );
  });
});

describe("updateStickyMeta", () => {
  beforeEach(async () => {
    vi.mocked(getAllStickies).mockResolvedValue([sticky({ id: "a", color: "#fff9c4" })]);
    await useStickiesStore.getState().loadStickies();
    vi.clearAllMocks();
  });

  it("applies the change locally", async () => {
    await useStickiesStore.getState().updateStickyMeta("a", { color: "#c8e6c9" });

    expect(useStickiesStore.getState().getSticky("a")?.color).toBe("#c8e6c9");
  });

  // Metadata changes are occasional, so they are not worth debouncing - and
  // delaying them would leave the manager showing a stale colour.
  it("announces the change immediately", async () => {
    await useStickiesStore.getState().updateStickyMeta("a", { color: "#c8e6c9" });

    expect(emit).toHaveBeenCalledWith(
      STICKY_UPDATED_EVENT,
      expect.objectContaining({ id: "a", changes: expect.objectContaining({ color: "#c8e6c9" }) }),
    );
  });
});

describe("deleteSticky", () => {
  beforeEach(async () => {
    vi.mocked(getAllStickies).mockResolvedValue([sticky({ id: "a" }), sticky({ id: "b" })]);
    await useStickiesStore.getState().loadStickies();
    vi.clearAllMocks();
  });

  it("removes it from the store", async () => {
    await useStickiesStore.getState().deleteSticky("a");

    expect(useStickiesStore.getState().getSticky("a")).toBeUndefined();
    expect(useStickiesStore.getState().getSticky("b")).toBeDefined();
  });

  it("deletes it from the database", async () => {
    await useStickiesStore.getState().deleteSticky("a");

    expect(bridgeDeleteSticky).toHaveBeenCalledWith("a");
  });

  // A delta cannot express a removal, so other windows are told to reload.
  it("tells other windows to reload rather than sending a delta", async () => {
    await useStickiesStore.getState().deleteSticky("a");

    expect(emit).toHaveBeenCalledWith("stickies-changed");
  });
});

describe("applyRemoteUpdate", () => {
  beforeEach(async () => {
    vi.mocked(getAllStickies).mockResolvedValue([sticky({ id: "a", content: "old" })]);
    await useStickiesStore.getState().loadStickies();
    vi.clearAllMocks();
  });

  it("folds in a change made in another window", () => {
    useStickiesStore.getState().applyRemoteUpdate({ id: "a", changes: { content: "edited" } });

    expect(useStickiesStore.getState().getSticky("a")?.content).toBe("edited");
  });

  // Otherwise the two windows would bounce the same edit back and forth.
  it("does not write the change back to the database", () => {
    useStickiesStore.getState().applyRemoteUpdate({ id: "a", changes: { content: "edited" } });

    expect(bridgeUpdateSticky).not.toHaveBeenCalled();
  });

  it("ignores an update for a sticky this window does not have", () => {
    useStickiesStore.getState().applyRemoteUpdate({ id: "ghost", changes: { content: "x" } });

    expect(useStickiesStore.getState().getSticky("ghost")).toBeUndefined();
  });
});

// The manager sorts by updated_at, so window state must not touch it. Otherwise
// opening or dragging a note jumps its card to the top of the list, under the
// cursor of whoever just clicked it.
describe("updateWindowState", () => {
  beforeEach(async () => {
    vi.mocked(getAllStickies).mockResolvedValue([sticky({ id: "a", updated_at: 1000 })]);
    await useStickiesStore.getState().loadStickies();
    vi.clearAllMocks();
  });

  it("does not change updated_at when a note is opened", async () => {
    await useStickiesStore.getState().updateWindowState("a", { is_open: 1 });

    expect(useStickiesStore.getState().getSticky("a")?.updated_at).toBe(1000);
  });

  it("does not change updated_at when a note is dragged", async () => {
    await useStickiesStore.getState().updateWindowState("a", { position_x: 500, position_y: 600 });

    expect(useStickiesStore.getState().getSticky("a")?.updated_at).toBe(1000);
  });

  it("still applies the change locally", async () => {
    await useStickiesStore.getState().updateWindowState("a", { position_x: 500, is_open: 1 });

    const s = useStickiesStore.getState().getSticky("a");
    expect(s?.position_x).toBe(500);
    expect(s?.is_open).toBe(1);
  });

  it("writes through the path that leaves updated_at alone", async () => {
    await useStickiesStore.getState().updateWindowState("a", { is_open: 1 });

    expect(bridgeUpdateWindowState).toHaveBeenCalledWith("a", { is_open: 1 });
    expect(bridgeUpdateSticky).not.toHaveBeenCalled();
  });

  // No other window shows a note's position or open state.
  it("does not announce window state to other windows", async () => {
    await useStickiesStore.getState().updateWindowState("a", { is_open: 1 });

    expect(emit).not.toHaveBeenCalled();
  });
});
