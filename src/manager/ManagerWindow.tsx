import { useEffect, useState } from "react";
import { useStickiesStore } from "../store/stickies";
import { openStickyWindow, getCurrentDesktopId, onDesktopChanged } from "../lib/tauri-bridge";
import type { Sticky } from "../lib/tauri-bridge";
import "../styles/manager.css";

function extractPreviewText(content: string): string {
  try {
    const doc = JSON.parse(content);
    const texts: string[] = [];
    function walk(node: { text?: string; content?: unknown[] }) {
      if (node.text) texts.push(node.text);
      if (node.content) (node.content as typeof node[]).forEach(walk);
    }
    walk(doc);
    return texts.join(" ").slice(0, 100) || "Empty note";
  } catch {
    return "Empty note";
  }
}

function formatDate(timestamp: number): string {
  if (!timestamp) return "";
  const d = new Date(timestamp);
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function StickyCard({ sticky, isOnCurrentDesktop }: { sticky: Sticky; isOnCurrentDesktop: boolean }) {
  const handleClick = async () => {
    await openStickyWindow(sticky);
  };

  return (
    <div className={`sticky-card ${!isOnCurrentDesktop && sticky.desktop_id ? "other-desktop" : ""}`} onClick={handleClick}>
      <div className="card-header">
        <div className="card-color-dot" style={{ backgroundColor: sticky.color }} />
        {sticky.pinned === 1 && <span className="card-pin-badge" title="Pinned: follows across desktops">{"\u{1F4CC}"}</span>}
        <span className="card-date">{formatDate(sticky.updated_at)}</span>
      </div>
      <div className="card-preview">{extractPreviewText(sticky.content)}</div>
    </div>
  );
}

const COLORS = ["#fff9c4", "#f8bbd0", "#c8e6c9", "#bbdefb", "#e1bee7", "#ffe0b2"];

function ManagerWindow() {
  const stickies = useStickiesStore((s) => s.stickies);
  const loaded = useStickiesStore((s) => s.loaded);
  const loadStickies = useStickiesStore((s) => s.loadStickies);
  const createSticky = useStickiesStore((s) => s.createSticky);
  const [currentDesktopId, setCurrentDesktopId] = useState("");

  useEffect(() => {
    if (!loaded) {
      loadStickies();
    }
  }, [loaded, loadStickies]);

  // Phase 2: Track current virtual desktop
  useEffect(() => {
    getCurrentDesktopId()
      .then((id) => setCurrentDesktopId(id))
      .catch(() => {});

    let unlisten: (() => void) | undefined;
    onDesktopChanged((desktopId) => {
      setCurrentDesktopId(desktopId);
    }).then((fn) => { unlisten = fn; });

    return () => { unlisten?.(); };
  }, []);

  const stickiesList = Array.from(stickies.values()).sort(
    (a, b) => b.updated_at - a.updated_at,
  );

  const handleNewSticky = async () => {
    const color = COLORS[Math.floor(Math.random() * COLORS.length)];
    await createSticky(color);
  };

  return (
    <div className="manager-window">
      <div className="manager-header">
        <h1>ShareSticky</h1>
        <button className="new-sticky-btn" onClick={handleNewSticky}>
          + New Sticky
        </button>
      </div>

      <div className="manager-content">
        {stickiesList.length === 0 && loaded ? (
          <div className="manager-empty">
            <p>No sticky notes yet. Create one to get started!</p>
          </div>
        ) : (
          <div className="stickies-grid">
            {stickiesList.map((sticky) => (
              <StickyCard
                key={sticky.id}
                sticky={sticky}
                isOnCurrentDesktop={!currentDesktopId || !sticky.desktop_id || sticky.desktop_id === currentDesktopId || sticky.pinned === 1}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export default ManagerWindow;
