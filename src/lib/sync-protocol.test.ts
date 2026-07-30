import { describe, expect, it } from "vitest";
import * as Y from "yjs";
import { SyncEngine } from "./sync-protocol";

// A synchronous in-memory link between two engines. Each engine's outbound
// frames are queued as deliveries to the other; `flush` drains the queue until
// it stays empty, so a full handshake (step1 -> step2 -> live updates) settles
// in one call. The guard turns a non-converging protocol into a test failure
// instead of a hang.
function connect(docA: Y.Doc, docB: Y.Doc) {
  const pending: Array<() => void> = [];
  let a: SyncEngine;
  let b: SyncEngine;
  a = new SyncEngine(docA, (f) => pending.push(() => b.receive(f)));
  b = new SyncEngine(docB, (f) => pending.push(() => a.receive(f)));
  const flush = () => {
    let guard = 0;
    while (pending.length) {
      if (++guard > 10_000) throw new Error("sync did not converge");
      pending.shift()!();
    }
  };
  return { a, b, flush };
}

const body = (doc: Y.Doc) => doc.getText("body").toString();

// Two docs hold the same state iff their state vectors match AND their content
// matches — the state vector alone proves they have seen the same updates.
function expectConverged(docA: Y.Doc, docB: Y.Doc) {
  expect(body(docA)).toBe(body(docB));
  expect(Y.encodeStateVector(docA)).toEqual(Y.encodeStateVector(docB));
}

describe("SyncEngine", () => {
  it("converges two independently-edited docs when they connect", () => {
    const a = new Y.Doc();
    const b = new Y.Doc();
    a.getText("body").insert(0, "from A. ");
    b.getText("body").insert(0, "from B. ");

    const { a: ea, b: eb, flush } = connect(a, b);
    ea.start();
    eb.start();
    flush();

    expectConverged(a, b);
    // Both edits survive the merge (CRDT union, order-independent).
    expect(body(a)).toContain("from A.");
    expect(body(a)).toContain("from B.");
  });

  it("brings a fresh peer up to an existing doc (late-joiner catch-up)", () => {
    const a = new Y.Doc();
    const b = new Y.Doc(); // empty joiner
    a.getText("body").insert(0, "existing note");

    const { a: ea, b: eb, flush } = connect(a, b);
    ea.start();
    eb.start();
    flush();

    expect(body(b)).toBe("existing note");
    expectConverged(a, b);
  });

  it("propagates a local edit to the peer after the initial sync", () => {
    const a = new Y.Doc();
    const b = new Y.Doc();
    const { a: ea, b: eb, flush } = connect(a, b);
    ea.start();
    eb.start();
    flush();

    a.getText("body").insert(0, "typed live");
    flush();

    expect(body(b)).toBe("typed live");
    expectConverged(a, b);
  });

  it("propagates concurrent edits in both directions", () => {
    const a = new Y.Doc();
    const b = new Y.Doc();
    const { a: ea, b: eb, flush } = connect(a, b);
    ea.start();
    eb.start();
    flush();

    a.getText("body").insert(0, "A");
    b.getText("body").insert(0, "B");
    flush();

    expectConverged(a, b);
    expect(body(a)).toContain("A");
    expect(body(a)).toContain("B");
  });

  it("does not echo a remote update back to its origin", () => {
    const a = new Y.Doc();
    const b = new Y.Doc();
    // Count only frames B emits, using a real engine for A.
    let bFrames = 0;
    const pending: Array<() => void> = [];
    let ea: SyncEngine;
    let eb: SyncEngine;
    ea = new SyncEngine(a, (f) => pending.push(() => eb.receive(f)));
    eb = new SyncEngine(b, (f) => {
      bFrames += 1;
      pending.push(() => ea.receive(f));
    });
    const flush = () => {
      while (pending.length) pending.shift()!();
    };
    ea.start();
    eb.start();
    flush();

    const framesAfterHandshake = bFrames;
    a.getText("body").insert(0, "one edit on A");
    flush();

    // B applied A's update but must NOT rebroadcast it back (no ping-pong).
    expect(bFrames).toBe(framesAfterHandshake);
    expectConverged(a, b);
  });

  it("stops broadcasting local edits after destroy", () => {
    const a = new Y.Doc();
    const b = new Y.Doc();
    const { a: ea, b: eb, flush } = connect(a, b);
    ea.start();
    eb.start();
    flush();

    ea.destroy();
    a.getText("body").insert(0, "after destroy");
    flush();

    expect(body(b)).toBe("");
  });

  it("ignores an inbound frame after destroy without throwing", () => {
    // Capture a real update frame from a live engine.
    const a = new Y.Doc();
    const frames: Uint8Array[] = [];
    new SyncEngine(a, (f) => frames.push(f));
    a.getText("body").insert(0, "content");
    const updateFrame = frames[frames.length - 1];

    const b = new Y.Doc();
    const eb = new SyncEngine(b, () => {});
    eb.destroy();
    // A stray late frame (e.g. in flight when the window closed) is a no-op.
    expect(() => eb.receive(updateFrame)).not.toThrow();
    expect(body(b)).toBe("");
  });

  it("does not start the handshake after destroy", () => {
    let frames = 0;
    const engine = new SyncEngine(new Y.Doc(), () => {
      frames += 1;
    });
    engine.destroy();
    engine.start();
    expect(frames).toBe(0);
  });

  it("is safe to destroy twice", () => {
    const engine = new SyncEngine(new Y.Doc(), () => {});
    engine.destroy();
    expect(() => engine.destroy()).not.toThrow();
  });
});
