import { describe, expect, it } from "vitest";
import type { Sticky } from "./tauri-bridge";
import { clampToVisible, restorePlanFor, type ScreenBounds } from "./session";

// Deciding what to put back on screen at startup: which notes were showing,
// and where they should go.

const DESKTOP_A = "{3F9D399E-C0CF-41D2-9743-5A229563DEDA}";
const DESKTOP_B = "{0EDBDC61-DD54-40A1-B6D9-E36E5BA42B7A}";

function sticky(overrides: Partial<Sticky> = {}): Sticky {
  return {
    id: "sticky-1",
    doc_id: "doc-1",
    content: "{}",
    color: "#fff9c4",
    desktop_id: "",
    position_x: 100,
    position_y: 100,
    width: 250,
    height: 200,
    pinned: 0,
    is_open: 0,
    sharing_tier: 0,
    share_key: "",
    created_at: 1000,
    updated_at: 1000,
    ...overrides,
  } as Sticky;
}

describe("restorePlanFor", () => {
  it("restores nothing when nothing was open", () => {
    expect(restorePlanFor([sticky({ id: "a" }), sticky({ id: "b" })])).toEqual([]);
  });

  it("restores a sticky that was open", () => {
    const open = sticky({ id: "a", is_open: 1 });

    expect(restorePlanFor([open])).toEqual([{ sticky: open, desktopId: undefined }]);
  });

  it("leaves closed stickies closed", () => {
    const open = sticky({ id: "a", is_open: 1 });
    const closed = sticky({ id: "b", is_open: 0 });

    const plan = restorePlanFor([open, closed]);

    expect(plan.map((p) => p.sticky.id)).toEqual(["a"]);
  });

  it("puts a sticky back on the desktop it was assigned to", () => {
    const open = sticky({ id: "a", is_open: 1, desktop_id: DESKTOP_B });

    expect(restorePlanFor([open])[0].desktopId).toBe(DESKTOP_B);
  });

  it("puts a multi-desktop sticky back on the first desktop it names", () => {
    const open = sticky({ id: "a", is_open: 1, desktop_id: `${DESKTOP_B},${DESKTOP_A}` });

    expect(restorePlanFor([open])[0].desktopId).toBe(DESKTOP_B);
  });

  // A sticky pinned everywhere has no particular home, and the desktop monitor
  // brings it along anyway. Naming a desktop here would fight that.
  it("does not pin an all-desktops sticky to one desktop", () => {
    const open = sticky({ id: "a", is_open: 1, desktop_id: "*" });

    expect(restorePlanFor([open])[0].desktopId).toBeUndefined();
  });

  it("leaves an unassigned sticky wherever it opens", () => {
    const open = sticky({ id: "a", is_open: 1, desktop_id: "" });

    expect(restorePlanFor([open])[0].desktopId).toBeUndefined();
  });
});

describe("clampToVisible", () => {
  // One 1920x1080 screen at the origin.
  const screen: ScreenBounds = { x: 0, y: 0, width: 1920, height: 1080 };

  it("leaves a sticky that is fully on screen alone", () => {
    expect(clampToVisible({ x: 300, y: 200, width: 250, height: 200 }, screen)).toEqual({
      x: 300,
      y: 200,
    });
  });

  // A note saved on a monitor that is no longer attached would otherwise be
  // restored somewhere unreachable, which looks exactly like losing it.
  it("pulls back a sticky saved beyond the right edge", () => {
    const { x } = clampToVisible({ x: 5000, y: 200, width: 250, height: 200 }, screen);

    expect(x).toBe(1920 - 250);
  });

  it("pulls back a sticky saved below the bottom edge", () => {
    const { y } = clampToVisible({ x: 300, y: 4000, width: 250, height: 200 }, screen);

    expect(y).toBe(1080 - 200);
  });

  it("pulls back a sticky saved at negative coordinates", () => {
    expect(clampToVisible({ x: -800, y: -600, width: 250, height: 200 }, screen)).toEqual({
      x: 0,
      y: 0,
    });
  });

  it("keeps a sticky wider than the screen at the origin rather than off it", () => {
    expect(clampToVisible({ x: 50, y: 50, width: 4000, height: 3000 }, screen)).toEqual({
      x: 0,
      y: 0,
    });
  });

  it("respects a screen that does not start at the origin", () => {
    const second: ScreenBounds = { x: 1920, y: 0, width: 1920, height: 1080 };

    expect(clampToVisible({ x: 0, y: 0, width: 250, height: 200 }, second)).toEqual({
      x: 1920,
      y: 0,
    });
  });
});
