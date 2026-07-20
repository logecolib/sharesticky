import { beforeEach, describe, expect, it, vi } from "vitest";

// Database.load is stubbed rather than mocking SQLite: what is worth checking
// is the statement and parameters this builds, not that SQLite works.
const execute = vi.fn(() => Promise.resolve());
const select = vi.fn(() => Promise.resolve([]));

vi.mock("@tauri-apps/plugin-sql", () => ({
  default: { load: vi.fn(() => Promise.resolve({ execute, select })) },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => Promise.resolve()) }));

import { deleteSticky, updateSticky } from "./tauri-bridge";

beforeEach(() => {
  execute.mockClear();
  select.mockClear();
});

/** The SQL and bound values from the most recent execute call. */
function lastStatement(): { sql: string; values: unknown[] } {
  const calls = execute.mock.calls;
  const [sql, values] = calls[calls.length - 1] as unknown as [string, unknown[]];
  return { sql, values };
}

describe("updateSticky", () => {
  it("updates only the field it was given", async () => {
    await updateSticky("abc", { color: "#c8e6c9" });

    const { sql, values } = lastStatement();
    expect(sql).toContain("color = $1");
    expect(values[0]).toBe("#c8e6c9");
  });

  it("numbers its parameters in order across several fields", async () => {
    await updateSticky("abc", { position_x: 10, position_y: 20 });

    const { sql, values } = lastStatement();
    expect(sql).toContain("position_x = $1");
    expect(sql).toContain("position_y = $2");
    expect(values[0]).toBe(10);
    expect(values[1]).toBe(20);
  });

  // Every write bumps updated_at, which is what the manager sorts by.
  it("stamps updated_at without being asked", async () => {
    await updateSticky("abc", { color: "#fff" });

    const { sql, values } = lastStatement();
    expect(sql).toContain("updated_at = $2");
    expect(typeof values[1]).toBe("number");
  });

  it("matches on the sticky's id as the final parameter", async () => {
    await updateSticky("abc", { color: "#fff" });

    const { sql, values } = lastStatement();
    expect(sql).toContain("WHERE id = $3");
    expect(values[values.length - 1]).toBe("abc");
  });

  // Letting id through would renumber the parameters and update the wrong row.
  it("refuses to overwrite the primary key even if handed one", async () => {
    await updateSticky("abc", { id: "somebody-else", color: "#fff" } as never);

    const { sql, values } = lastStatement();
    expect(sql).not.toContain("id = $1");
    expect(values[values.length - 1]).toBe("abc");
  });

  it("does nothing at all when given no fields", async () => {
    await updateSticky("abc", {});

    expect(execute).not.toHaveBeenCalled();
  });

  it("writes a zero rather than treating it as absent", async () => {
    await updateSticky("abc", { pinned: 0 });

    const { sql, values } = lastStatement();
    expect(sql).toContain("pinned = $1");
    expect(values[0]).toBe(0);
  });

  it("writes an empty string rather than treating it as absent", async () => {
    await updateSticky("abc", { desktop_id: "" });

    const { sql, values } = lastStatement();
    expect(sql).toContain("desktop_id = $1");
    expect(values[0]).toBe("");
  });
});

describe("deleteSticky", () => {
  it("deletes only the sticky it names", async () => {
    await deleteSticky("abc");

    const { sql, values } = lastStatement();
    expect(sql).toContain("DELETE FROM stickies");
    expect(sql).toContain("WHERE id = $1");
    expect(values).toEqual(["abc"]);
  });
});
