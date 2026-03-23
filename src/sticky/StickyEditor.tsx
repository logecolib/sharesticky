import { useEffect } from 'react';
import { useEditor, EditorContent } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import Placeholder from '@tiptap/extension-placeholder';
import { useStickiesStore } from '../store/stickies';

interface StickyEditorProps {
  stickyId: string;
  initialContent: string;
}

function StickyEditor({ stickyId, initialContent }: StickyEditorProps) {
  const updateStickyContent = useStickiesStore((s) => s.updateStickyContent);

  const editor = useEditor({
    extensions: [
      StarterKit,
      TaskList,
      TaskItem.configure({
        nested: true,
      }),
      Placeholder.configure({
        placeholder: 'Write something...',
      }),
    ],
    content: initialContent ? JSON.parse(initialContent) : undefined,
    onUpdate: ({ editor }) => {
      const json = JSON.stringify(editor.getJSON());
      updateStickyContent(stickyId, json);
    },
  });

  // Update editor content if it changes externally
  useEffect(() => {
    if (editor && initialContent) {
      try {
        const parsed = JSON.parse(initialContent);
        const currentJson = JSON.stringify(editor.getJSON());
        if (currentJson !== initialContent) {
          // Only update if content actually differs to avoid cursor jumps
        }
      } catch {
        // Invalid JSON, ignore
      }
    }
  }, [editor, initialContent]);

  return (
    <div className="sticky-editor">
      <EditorContent editor={editor} />
    </div>
  );
}

export default StickyEditor;
