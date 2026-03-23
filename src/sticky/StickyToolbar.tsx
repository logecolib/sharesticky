import { useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useStickiesStore } from "../store/stickies";

const STICKY_COLORS = [
  { name: "#fff9c4", label: "Yellow" },
  { name: "#f8bbd0", label: "Pink" },
  { name: "#c8e6c9", label: "Green" },
  { name: "#bbdefb", label: "Blue" },
  { name: "#e1bee7", label: "Purple" },
  { name: "#ffe0b2", label: "Orange" },
];

interface StickyToolbarProps {
  stickyId: string;
  currentColor: string;
  pinned: number;
}

function StickyToolbar({ stickyId, currentColor, pinned }: StickyToolbarProps) {
  const [isPinned, setIsPinned] = useState(pinned === 1);
  const updateStickyMeta = useStickiesStore((s) => s.updateStickyMeta);
  const deleteStickyAction = useStickiesStore((s) => s.deleteSticky);

  const handleColorChange = async (color: string) => {
    await updateStickyMeta(stickyId, { color });
  };

  const handlePinToggle = async () => {
    const newPinned = !isPinned;
    setIsPinned(newPinned);
    await getCurrentWindow().setAlwaysOnTop(newPinned);
    await updateStickyMeta(stickyId, { pinned: newPinned ? 1 : 0 });
  };

  const handleDelete = async () => {
    await deleteStickyAction(stickyId);
    await getCurrentWindow().close();
  };

  return (
    <div className="sticky-toolbar">
      <div className="color-picker">
        {STICKY_COLORS.map((c) => (
          <button
            key={c.name}
            className={`color-swatch ${currentColor === c.name ? "selected" : ""}`}
            style={{ backgroundColor: c.name }}
            onClick={() => handleColorChange(c.name)}
            title={c.label}
          />
        ))}
      </div>
      <button
        className={`toolbar-btn ${isPinned ? "active" : ""}`}
        onClick={handlePinToggle}
        title={isPinned ? "Unpin" : "Pin on top"}
      >
        {"\u{1F4CC}"}
      </button>
      <div className="toolbar-divider" />
      <button
        className="toolbar-btn"
        onClick={() => console.log("Share: Phase 3+")}
        title="Share (coming soon)"
      >
        {"\u{1F517}"}
      </button>
      <button className="toolbar-btn delete" onClick={handleDelete} title="Delete">
        {"\u{1F5D1}"}
      </button>
    </div>
  );
}

export default StickyToolbar;
