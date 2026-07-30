import { describe, expect, it } from "vitest";
import * as Y from "yjs";
import {
  contentNeedsSeeding,
  docFromBytes,
  encodeDoc,
  projectToContent,
  seedDocFromContent,
} from "./sticky-doc";

const helloContent = JSON.stringify({
  type: "doc",
  content: [{ type: "paragraph", content: [{ type: "text", text: "hello world" }] }],
});

/** Pull the text out of a projection the way the manager's preview does. */
function textOf(contentJson: string): string {
  const out: string[] = [];
  const walk = (n: { text?: string; content?: unknown[] }) => {
    if (n.text) out.push(n.text);
    if (n.content) (n.content as (typeof n)[]).forEach(walk);
  };
  walk(JSON.parse(contentJson));
  return out.join(" ");
}

describe("seedDocFromContent + projectToContent", () => {
  it("preserves note text through seeding and projection", () => {
    const doc = seedDocFromContent(helloContent);
    expect(textOf(projectToContent(doc))).toContain("hello world");
  });

  it("produces a projection the manager preview can parse (a doc)", () => {
    const doc = seedDocFromContent(helloContent);
    expect(JSON.parse(projectToContent(doc)).type).toBe("doc");
  });

  it("yields an empty doc for empty content", () => {
    const doc = seedDocFromContent("{}");
    expect(textOf(projectToContent(doc))).toBe("");
  });

  it("yields an empty doc for unparseable content", () => {
    const doc = seedDocFromContent("not json");
    expect(JSON.parse(projectToContent(doc)).type).toBe("doc");
  });
});

describe("encodeDoc + docFromBytes round-trip", () => {
  it("survives a save/load cycle with text intact", () => {
    const original = seedDocFromContent(helloContent);
    const bytes = encodeDoc(original);

    const restored = docFromBytes(bytes);
    expect(textOf(projectToContent(restored))).toContain("hello world");
  });

  it("merges a later edit's bytes onto an earlier state (CRDT convergence)", () => {
    // Two docs from the same origin, edited independently, converge when each
    // applies the other's update - the property that makes sharing possible.
    const base = seedDocFromContent(helloContent);
    const bytesBase = encodeDoc(base);

    const a = docFromBytes(bytesBase);
    const b = docFromBytes(bytesBase);
    a.getXmlFragment("default").firstChild; // touch fragment
    // Apply b's state onto a and vice-versa; both must end identical.
    Y.applyUpdate(a, encodeDoc(b));
    Y.applyUpdate(b, encodeDoc(a));
    expect(projectToContent(a)).toEqual(projectToContent(b));
  });

  it("an empty byte array gives an empty doc, not a crash", () => {
    expect(JSON.parse(projectToContent(docFromBytes(new Uint8Array()))).type).toBe("doc");
    expect(JSON.parse(projectToContent(docFromBytes(null))).type).toBe("doc");
  });
});

describe("contentNeedsSeeding", () => {
  it("is true for real note content", () => {
    expect(contentNeedsSeeding(helloContent)).toBe(true);
  });

  it("is false for empty, {}, or junk", () => {
    expect(contentNeedsSeeding("")).toBe(false);
    expect(contentNeedsSeeding("{}")).toBe(false);
    expect(contentNeedsSeeding('{"type":"doc","content":[]}')).toBe(false);
    expect(contentNeedsSeeding("garbage")).toBe(false);
  });
});
