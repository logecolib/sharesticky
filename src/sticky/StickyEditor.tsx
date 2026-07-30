import { useEditor, EditorContent } from "@tiptap/react";
import Collaboration from "@tiptap/extension-collaboration";
import type * as Y from "yjs";
import { editorExtensions } from "./editor-extensions";

interface StickyEditorProps {
  /** The note's Yjs document — the source of truth for its content. */
  doc: Y.Doc;
}

function StickyEditor({ doc }: StickyEditorProps) {
  // Content comes from the Y.Doc (via Collaboration), not a prop. StarterKit's
  // history is off in editorExtensions so Yjs owns undo/redo. Persistence and
  // the preview projection are handled by StickyWindow, which owns the doc.
  const editor = useEditor(
    {
      extensions: [...editorExtensions, Collaboration.configure({ document: doc })],
    },
    [doc],
  );

  return (
    <div className="sticky-editor">
      <EditorContent editor={editor} />
    </div>
  );
}

export default StickyEditor;
