import { beforeEach, describe, expect, it, vi } from "vitest";

// The bridge is now a thin invoke() layer over Rust commands. What's worth
// checking here is the command name and argument shape; the SQL behaviour it
// used to build (column whitelist, updated_at stamping, zero/empty handling)
// moved into Rust and is tested there (src-tauri storage::repo).
// vi.mock is hoisted above the file, so the mock fn must be hoisted too.
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  createSticky,
  deleteSticky,
  getAllStickies,
  updateSticky,
  updateStickyWindowState,
} from "./tauri-bridge";

beforeEach(() => invoke.mockClear());

function lastCall(): [string, unknown] {
  const calls = invoke.mock.calls;
  return calls[calls.length - 1] as unknown as [string, unknown];
}

describe("getAllStickies", () => {
  it("calls the list command", async () => {
    await getAllStickies();
    expect(lastCall()[0]).toBe("list_stickies");
  });
});

describe("createSticky", () => {
  it("passes the colour to the create command", async () => {
    await createSticky("#c8e6c9");
    expect(lastCall()).toEqual(["create_sticky", { color: "#c8e6c9" }]);
  });

  it("defaults the colour when none is given", async () => {
    await createSticky();
    const [cmd, args] = lastCall();
    expect(cmd).toBe("create_sticky");
    expect((args as { color: string }).color).toBeTruthy();
  });
});

describe("updateSticky", () => {
  it("sends the edit as a patch under the sticky id", async () => {
    await updateSticky("abc", { color: "#fff", content: "hi" });
    expect(lastCall()).toEqual([
      "update_sticky",
      { id: "abc", patch: { color: "#fff", content: "hi" } },
    ]);
  });
});

describe("updateStickyWindowState", () => {
  // A distinct command so Rust knows not to stamp updated_at (which would
  // reorder the manager on open/drag).
  it("uses the window-state command, not the edit command", async () => {
    await updateStickyWindowState("abc", { position_x: 5, is_open: 1 });
    expect(lastCall()).toEqual([
      "update_sticky_window_state",
      { id: "abc", patch: { position_x: 5, is_open: 1 } },
    ]);
  });
});

describe("deleteSticky", () => {
  it("names the sticky to delete", async () => {
    await deleteSticky("abc");
    expect(lastCall()).toEqual(["delete_sticky", { id: "abc" }]);
  });
});
