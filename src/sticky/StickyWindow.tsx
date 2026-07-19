import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import StickyDragHandle from "./StickyDragHandle";
import StickyEditor from "./StickyEditor";
import StickyToolbar from "./StickyToolbar";
import { useStickiesStore } from "../store/stickies";
import { parseDesktopIds } from "../lib/desktop-visibility";
import {
  moveStickyToDesktop,
  setStickyDesktops,
  getCurrentDesktopId,
} from "../lib/tauri-bridge";
import type { Sticky } from "../lib/tauri-bridge";
import "../styles/sticky.css";

interface StickyWindowProps {
  label: string;
}

function StickyWindow({ label }: StickyWindowProps) {
  const stickyId = label.replace("sticky-", "");
  const loadStickies = useStickiesStore((s) => s.loadStickies);
  const loaded = useStickiesStore((s) => s.loaded);
  const getSticky = useStickiesStore((s) => s.getSticky);
  const updateStickyMeta = useStickiesStore((s) => s.updateStickyMeta);
  const [sticky, setSticky] = useState<Sticky | undefined>();

  useEffect(() => {
    if (!loaded) loadStickies();
  }, [loaded, loadStickies]);

  useEffect(() => {
    if (loaded) setSticky(getSticky(stickyId));
  }, [loaded, stickyId, getSticky]);

  useEffect(() => {
    const unsub = useStickiesStore.subscribe((state) => {
      const updated = state.stickies.get(stickyId);
      if (updated) setSticky(updated);
    });
    return unsub;
  }, [stickyId]);

  // Handle desktop menu actions from the Rust global on_menu_event handler
  useEffect(() => {
    const unlisten = listen<{ sticky_id: string; desktop_id: string }>(
      "desktop-menu-action",
      async (event) => {
        const { sticky_id, desktop_id } = event.payload;
        if (sticky_id !== stickyId) return;

        const currentSticky = useStickiesStore.getState().stickies.get(stickyId);
        if (!currentSticky) return;

        const assigned = parseDesktopIds(currentSticky.desktop_id);
        const isAll = assigned.has("*");

        if (desktop_id === "*") {
          // Toggle "All desktops"
          if (isAll) {
            const currentId = await getCurrentDesktopId().catch(() => "");
            await updateStickyMeta(stickyId, { desktop_id: currentId });
            await setStickyDesktops(stickyId, []).catch(() => {});
          } else {
            await updateStickyMeta(stickyId, { desktop_id: "*" });
            await setStickyDesktops(stickyId, ["*"]).catch(() => {});
          }
        } else {
          // Toggle a specific desktop
          const newSet = new Set(assigned);
          newSet.delete("*");
          if (assigned.has(desktop_id) && !isAll) {
            newSet.delete(desktop_id);
          } else {
            newSet.add(desktop_id);
          }
          const newIds = Array.from(newSet);

          if (newIds.length === 0) {
            // Last desktop unselected → delete the sticky entirely
            const deleteSticky = useStickiesStore.getState().deleteSticky;
            await setStickyDesktops(stickyId, []).catch(() => {});
            await deleteSticky(stickyId);
            await getCurrentWindow().close();
            return;
          }

          await updateStickyMeta(stickyId, { desktop_id: newIds.join(",") });

          // If we just removed the current desktop, move window to a remaining one
          const currentDesktop = await getCurrentDesktopId().catch(() => "");
          if (currentDesktop && !newSet.has(currentDesktop)) {
            await moveStickyToDesktop(stickyId, newIds[0]).catch((err) =>
              console.error("moveStickyToDesktop:", err)
            );
          }

          await setStickyDesktops(stickyId, newIds.length > 1 ? newIds : []).catch(() => {});
        }
      }
    );
    return () => { unlisten.then((fn) => fn()); };
  }, [stickyId, updateStickyMeta]);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      if (!sticky) return;
      // Fire and forget — the menu shows natively, events come back via listener
      invoke("show_desktop_menu", {
        stickyId,
        currentDesktopId: sticky.desktop_id || "",
      }).catch((err) => console.error("show_desktop_menu:", err));
    },
    [sticky, stickyId]
  );

  if (!sticky) {
    return (
      <div className="sticky-window" style={{ backgroundColor: "#fff9c4" }}>
        <StickyDragHandle />
        <div className="sticky-editor" style={{ padding: 12, opacity: 0.5 }}>Loading...</div>
      </div>
    );
  }

  return (
    <div className="sticky-window" style={{ backgroundColor: sticky.color }} onContextMenu={handleContextMenu}>
      <StickyDragHandle />
      <StickyEditor stickyId={stickyId} initialContent={sticky.content} />
      <StickyToolbar stickyId={stickyId} currentColor={sticky.color} pinned={sticky.pinned} />
    </div>
  );
}

export default StickyWindow;
