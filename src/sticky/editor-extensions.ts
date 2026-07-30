import StarterKit from "@tiptap/starter-kit";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Placeholder from "@tiptap/extension-placeholder";
import type { Extensions } from "@tiptap/core";

/**
 * The note editor's extensions, shared by the live editor and the Yjs schema.
 *
 * StarterKit's history is **off**: with Collaboration, Yjs owns undo/redo, and
 * running both throws. The editor adds `Collaboration` on top of these; the Yjs
 * schema derives from these alone (Collaboration is not a schema extension).
 *
 * Both consumers MUST use this same list, or a note seeded from one schema won't
 * load correctly in an editor built from another.
 */
export const editorExtensions: Extensions = [
  StarterKit.configure({ history: false }),
  TaskList,
  TaskItem.configure({ nested: true }),
  Placeholder.configure({ placeholder: "Write something..." }),
];
