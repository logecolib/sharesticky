# ShareSticky — Project Brief

_Last reviewed: 2026-06-28_

## What it is

ShareSticky is a **desktop sticky-notes app for Windows**, built to be aware of
**Windows virtual desktops**. Notes live as small borderless windows on the
desktop and are managed from a central manager window that sits in the system
tray. The long-term vision is **peer-to-peer sharing** of notes (LAN first, then
cloud relay) with **end-to-end encryption** — hence the name "ShareSticky."

## Tech stack

- **Shell:** [Tauri 2](https://tauri.app/) (Rust backend + WebView2 frontend)
- **Frontend:** React 19 + TypeScript, Vite 6, Zustand (state), TipTap (rich-text editor)
- **Storage:** SQLite via `tauri-plugin-sql` (single `stickies` table)
- **Platform integration:** `windows` crate 0.58 (COM `IVirtualDesktopManager`, registry reads)
- **Planned sync:** Yjs / Yrs CRDTs, mDNS + TCP for LAN, AES-256-GCM encryption

## Architecture at a glance

```
src/                       React frontend
  manager/ManagerWindow    tray-driven list/manager of all stickies
  manager/ShareDialog      sharing UI (placeholder)
  sticky/StickyWindow      individual note window (editor, toolbar, drag)
  store/stickies.ts        Zustand store, talks to backend via tauri-bridge
  sync/*                   yjs-provider, webrtc-sync (STUBS)
src-tauri/src/             Rust backend
  lib.rs                   tray menu, desktop-monitor polling thread, command registry
  commands/                sticky (window open/close), desktop (VD ops), sharing (STUB)
  platform/windows.rs      VirtualDesktopService (COM + registry)
  storage/database.rs      SQLite schema (migration v1)
  storage/encryption.rs    AES encryption (STUB)
  sync/                    yrs_manager, lan, transport (STUBS)
```

## Current status

The project is in **Phase 2**, partly complete and not fully committed.

| Phase | Scope | Status |
|-------|-------|--------|
| **Phase 1** | Local sticky notes: create, edit (rich text), move, color, persist to SQLite | ✅ Working |
| **Phase 2** | Virtual desktop awareness: tag stickies per desktop, follow/pin across desktops, manager "this desktop" filter | 🟡 WIP — core works, uncommitted changes present |
| **Phase 3** | P2P sync (Yjs/Yrs CRDTs, LAN via mDNS+TCP) | ⛔ Not started — stub files only |
| **Phase 4** | WebRTC sync + AES-256-GCM encryption for shared notes | ⛔ Not started — stub files only |

**Uncommitted work** (3 files): the manager window's "This desktop" filter and
its CSS, plus a `lib.rs` change so the manager window follows the user across
all virtual desktops. See `git diff`.

### Phase 2 hard-won knowledge (don't relearn the hard way)

- VD tracking **breaks** if you use `skip_taskbar(true)`, `transparent(true)`,
  `WS_EX_TOOLWINDOW`, or owner windows — each unregisters/cloaks the window from
  the virtual-desktop system.
- The **working baseline** is a plain borderless window (`decorations(false)`,
  no transparency, no skip_taskbar).
- The app uses the **documented `IVirtualDesktopManager` COM API + registry
  reads** (not the `winvd` crate) for stability across Windows updates.
- Current desktop / desktop list are read from
  `HKCU\...\Explorer\VirtualDesktops\*` in the registry.
- An open Phase 2 problem: **hide stickies from the taskbar without breaking VD
  tracking** (still unsolved).

## Automated tests

**None.** There is no test runner configured:
- No `#[test]` / `#[cfg(test)]` in any Rust file.
- No vitest/jest in `package.json`; no test scripts.
- Verification is currently manual (`npm run tauri:dev`).

## How to run

```powershell
npm install
npm run tauri:dev      # dev build with the MSVC toolchain on PATH
npm run tauri:build    # release build
```

> The `tauri:dev` / `tauri:build` scripts prepend the Visual Studio 2022
> BuildTools MSVC path. If the MSVC version on this machine differs from
> `14.44.35207`, update the path in `package.json`.

## Suggested next tasks

1. **Finish & commit Phase 2** — review the 3 uncommitted files, confirm the
   "this desktop" filter and manager-follow behavior, then commit.
2. **Solve the taskbar problem** — hide sticky windows from the taskbar without
   losing virtual-desktop tracking (the main open Phase 2 issue).
3. **Wire up sharing command registry** — `commands/sharing.rs` exists and is
   declared but isn't registered in `lib.rs`'s `invoke_handler`.
4. **Start Phase 3** — implement `yrs_manager` (CRDT docs persisted to the
   `yjs_state` BLOB) before touching transport/LAN.
5. **Add a minimal test harness** — at least Rust unit tests for
   `platform::windows` desktop-id parsing and the (future) encryption module,
   since those are pure logic and easy to regression-test.

## Open risks / notes

- Heavy reliance on Windows-only APIs; `platform/macos.rs` and `linux.rs` are
  stubs, so the app is effectively Windows-only today.
- The desktop monitor is a **500ms polling thread** — fine for now, but a
  source of latency/CPU if scaled up.
- DB schema already includes `sharing_tier` / `share_key` columns anticipating
  Phases 3–4, but no sharing logic exists yet.
</content>
</invoke>
