import { useEffect, useState } from "react";
import { useStickiesStore } from "../store/stickies";
import { listen } from "@tauri-apps/api/event";
import {
  openStickyWindow,
  getCurrentDesktopId,
  onDesktopChanged,
  placeAndFocusSticky,
} from "../lib/tauri-bridge";
import type { Sticky } from "../lib/tauri-bridge";
import { isOnDesktop, isStickyOnCurrentDesktop, navigationFor } from "../lib/desktop-visibility";
import { STICKY_UPDATED_EVENT, type StickyUpdatePayload } from "../lib/sticky-sync";
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

function StickyCard({
  sticky,
  isOnCurrentDesktop,
  currentDesktopId,
}: {
  sticky: Sticky;
  isOnCurrentDesktop: boolean;
  currentDesktopId: string;
}) {
  const handleClick = async () => {
    const destination = navigationFor(sticky.desktop_id, currentDesktopId);

    // One call does create-if-needed, move, then activate. Activating is what
    // carries us to the window's desktop, and Windows only permits it once per
    // user action - so this must be the only thing that focuses.
    await placeAndFocusSticky(
      sticky,
      destination.kind === "travel" ? destination.desktopId : undefined,
    ).catch((err) => console.error("place_and_focus_sticky:", err));
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
  const applyRemoteUpdate = useStickiesStore((s) => s.applyRemoteUpdate);
  const [currentDesktopId, setCurrentDesktopId] = useState("");
  const [thisDesktopOnly, setThisDesktopOnly] = useState(true);

  useEffect(() => {
    if (!loaded) {
      loadStickies();
    }
  }, [loaded, loadStickies]);

  // Reload when stickies are added or removed elsewhere.
  useEffect(() => {
    const unlisten = listen("stickies-changed", () => {
      loadStickies();
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [loadStickies]);

  // Edits arrive as deltas, so a sticky being typed into does not cost a reload
  // of every sticky from SQLite.
  useEffect(() => {
    const unlisten = listen<StickyUpdatePayload>(STICKY_UPDATED_EVENT, (event) => {
      applyRemoteUpdate(event.payload);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [applyRemoteUpdate]);

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

  const stickiesList = Array.from(stickies.values())
    .filter((s) => {
      if (!thisDesktopOnly || !currentDesktopId) return true;
      return isOnDesktop(s.desktop_id, currentDesktopId);
    })
    .sort((a, b) => b.updated_at - a.updated_at);

  const handleNewSticky = async () => {
    const color = COLORS[Math.floor(Math.random() * COLORS.length)];
    await createSticky(color);
  };

  return (
    <div className="manager-window">
      <div className="manager-header">
        <h1>ShareSticky</h1>
        <div className="manager-header-actions">
          <label className="desktop-filter-checkbox">
            <input
              type="checkbox"
              checked={thisDesktopOnly}
              onChange={(e) => setThisDesktopOnly(e.target.checked)}
            />
            <span className="filter-label">This desktop</span>
          </label>
          <button className="new-sticky-btn" onClick={handleNewSticky}>
            + New Sticky
          </button>
        </div>
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
                isOnCurrentDesktop={isStickyOnCurrentDesktop(sticky, currentDesktopId)}
                currentDesktopId={currentDesktopId}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export default ManagerWindow;
