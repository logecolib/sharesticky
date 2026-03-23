import { getCurrentWindow } from '@tauri-apps/api/window';

interface StickyDragHandleProps {
  onClose?: () => void;
}

function StickyDragHandle({ onClose }: StickyDragHandleProps) {
  const handleMouseDown = async (e: React.MouseEvent) => {
    // Don't start drag if clicking the close button
    if ((e.target as HTMLElement).closest('.close-btn')) return;
    await getCurrentWindow().startDragging();
  };

  const handleClose = () => {
    if (onClose) {
      onClose();
    } else {
      getCurrentWindow().close();
    }
  };

  return (
    <div className="sticky-drag-handle" onMouseDown={handleMouseDown}>
      <div className="grip-dots">
        <span className="dot" />
        <span className="dot" />
        <span className="dot" />
        <span className="dot" />
        <span className="dot" />
      </div>
      <button className="close-btn" onClick={handleClose} title="Close">
        ✕
      </button>
    </div>
  );
}

export default StickyDragHandle;
