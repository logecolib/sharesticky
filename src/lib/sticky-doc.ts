// The Yjs side of a note. Each note is a Y.Doc; this module is the pure
// data-path between that doc, the bytes we persist (`yjs_state`), and the
// TipTap-JSON `content` projection the manager preview reads.
//
// No editor/DOM here — just document data — so it is unit-testable.

import * as Y from "yjs";
import { prosemirrorJSONToYDoc, yDocToProsemirrorJSON } from "y-prosemirror";
import { getSchema } from "@tiptap/core";
import { editorExtensions } from "../sticky/editor-extensions";

/**
 * The Yjs XML fragment name. **Must match TipTap Collaboration's `field`**
 * (which defaults to "default"). y-prosemirror's own default is "prosemirror",
 * so we pass this explicitly everywhere — a mismatch loads notes blank.
 */
export const DOC_FIELD = "default";

const schema = getSchema(editorExtensions);

/** Encode a doc's full state to the bytes we persist in `yjs_state`. */
export function encodeDoc(doc: Y.Doc): Uint8Array {
  return Y.encodeStateAsUpdate(doc);
}

/** Rebuild a doc from persisted bytes (empty bytes -> empty doc). */
export function docFromBytes(bytes: Uint8Array | null | undefined): Y.Doc {
  const doc = new Y.Doc();
  if (bytes && bytes.length > 0) {
    Y.applyUpdate(doc, bytes);
  }
  return doc;
}

/**
 * Seed a fresh doc from a stored TipTap-JSON `content` string — the one-time
 * migration for notes that predate the Y.Doc era. Empty / `{}` / unparseable
 * content yields an empty doc.
 */
export function seedDocFromContent(content: string): Y.Doc {
  const json = parseProseMirrorJson(content);
  if (!json) return new Y.Doc();
  return prosemirrorJSONToYDoc(schema, json, DOC_FIELD);
}

/**
 * The TipTap-JSON projection of a doc — what gets written back to `content` and
 * parsed by the manager's `extractPreviewText`. Same shape as the editor's
 * `getJSON()`, so the preview walker is unchanged.
 */
export function projectToContent(doc: Y.Doc): string {
  return JSON.stringify(yDocToProsemirrorJSON(doc, DOC_FIELD));
}

/** Does this stored content need seeding into a doc (i.e. is it real text)? */
export function contentNeedsSeeding(content: string): boolean {
  return parseProseMirrorJson(content) !== null;
}

function parseProseMirrorJson(content: string): unknown | null {
  if (!content) return null;
  try {
    const json = JSON.parse(content) as { type?: string; content?: unknown[] };
    // A real ProseMirror doc has a type ("doc") and some content. `{}` or an
    // empty doc is treated as nothing to seed.
    if (!json || json.type !== "doc" || !json.content || json.content.length === 0) {
      return null;
    }
    return json;
  } catch {
    return null;
  }
}
