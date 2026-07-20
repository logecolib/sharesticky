import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Sticky } from "./tauri-bridge";
import { applyStickyUpdate, debounce, STICKY_UPDATED_EVENT } from "./sticky-sync";

// Each window is its own webview with its own store, so edits made in a sticky
// window reach the manager only as events. These helpers are what the manager
// uses to fold an incoming change into what it already has, without re-reading
// every sticky from SQLite.

function sticky(overrides: Partial<Sticky> = {}): Sticky {
  return {
    id: "sticky-1",
    doc_id: "doc-1",
    content: "{}",
    color: "#fff9c4",
    desktop_id: "",
    position_x: 0,
    position_y: 0,
    width: 250,
    height: 200,
    pinned: 0,
    sharing_tier: 0,
    share_key: "",
    created_at: 1000,
    updated_at: 1000,
    ...overrides,
  } as Sticky;
}

function mapOf(...items: Sticky[]): Map<string, Sticky> {
  return new Map(items.map((s) => [s.id, s]));
}

describe("STICKY_UPDATED_EVENT", () => {
  it("is distinct from the wholesale reload event", () => {
    expect(STICKY_UPDATED_EVENT).not.toBe("stickies-changed");
  });
});

describe("applyStickyUpdate", () => {
  it("merges the changed fields into the sticky it names", () => {
    const before = mapOf(sticky({ id: "a", content: "old" }));

    const after = applyStickyUpdate(before, {
      id: "a",
      changes: { content: "new", updated_at: 2000 },
    });

    expect(after.get("a")?.content).toBe("new");
    expect(after.get("a")?.updated_at).toBe(2000);
  });

  it("leaves fields the update did not mention alone", () => {
    const before = mapOf(sticky({ id: "a", color: "#abcdef", width: 400 }));

    const after = applyStickyUpdate(before, { id: "a", changes: { content: "new" } });

    expect(after.get("a")?.color).toBe("#abcdef");
    expect(after.get("a")?.width).toBe(400);
  });

  it("does not disturb other stickies", () => {
    const other = sticky({ id: "b", content: "untouched" });
    const before = mapOf(sticky({ id: "a" }), other);

    const after = applyStickyUpdate(before, { id: "a", changes: { content: "new" } });

    expect(after.get("b")).toBe(other);
  });

  it("does not mutate the map it was given", () => {
    const before = mapOf(sticky({ id: "a", content: "old" }));

    applyStickyUpdate(before, { id: "a", changes: { content: "new" } });

    expect(before.get("a")?.content).toBe("old");
  });

  it("returns a new map so subscribers actually re-render", () => {
    const before = mapOf(sticky({ id: "a" }));

    const after = applyStickyUpdate(before, { id: "a", changes: { content: "new" } });

    expect(after).not.toBe(before);
  });

  // An update can arrive for a sticky this window has never loaded - created on
  // another desktop, say. Inventing a half-populated entry from a partial patch
  // would put a broken card in the list.
  it("ignores an update for a sticky it does not know about", () => {
    const before = mapOf(sticky({ id: "a" }));

    const after = applyStickyUpdate(before, { id: "unknown", changes: { content: "x" } });

    expect(after.has("unknown")).toBe(false);
    expect(after).toBe(before);
  });

  it("ignores an update carrying no changes", () => {
    const before = mapOf(sticky({ id: "a" }));

    expect(applyStickyUpdate(before, { id: "a", changes: {} })).toBe(before);
  });
});

describe("debounce", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  // The editor calls through on every keystroke; without this the manager would
  // receive an event per character.
  it("does not call through until the delay has passed", () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 200);

    debounced("a");

    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(200);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("collapses a burst of calls into one", () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 200);

    debounced("a");
    vi.advanceTimersByTime(50);
    debounced("ab");
    vi.advanceTimersByTime(50);
    debounced("abc");
    vi.advanceTimersByTime(200);

    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("calls through with the most recent arguments", () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 200);

    debounced("a");
    debounced("ab");
    debounced("abc");
    vi.advanceTimersByTime(200);

    expect(fn).toHaveBeenCalledWith("abc");
  });

  it("runs again for a later, separate burst", () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 200);

    debounced("first");
    vi.advanceTimersByTime(200);
    debounced("second");
    vi.advanceTimersByTime(200);

    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("can be cancelled before it fires, so an unmount drops the pending call", () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 200);

    debounced("a");
    debounced.cancel();
    vi.advanceTimersByTime(500);

    expect(fn).not.toHaveBeenCalled();
  });
});
