# ShareSticky — Project Brief

_Last reviewed: 2026-07-19_

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
  commands/                sticky (windows), desktop (VD ops), desktop_menu (pure), sharing (STUB)
  platform/                port + fake, pure policy/encoding, probe; windows.rs is the COM shim
  storage/database.rs      SQLite schema (migrations v1, v2)
  storage/encryption.rs    AES encryption (STUB)
  sync/                    yrs_manager, lan, transport (STUBS)
```

## Current status

**Phase 2 is complete and merged.** Everything below is on `main` with CI green.

| Phase | Scope | Status |
|-------|-------|--------|
| **Phase 1** | Local sticky notes: create, edit (rich text), move, color, persist to SQLite | ✅ Working |
| **Phase 2** | Virtual desktop awareness: per-desktop tagging, follow/pin, manager filter and desktop labels, travel-to-desktop, session restore | ✅ Working |
| **Phase 3** | P2P sync (Yjs/Yrs CRDTs, LAN via mDNS+TCP) | ⛔ Not started — stub files only |
| **Phase 4** | WebRTC sync + AES-256-GCM encryption for shared notes | ⛔ Not started — stub files only |

What Phase 2 ended up covering: stickies tagged to one, several, or all
desktops; a manager that follows the user everywhere; a "This desktop" filter
with the desktop names shown on each card; clicking a note carries you to its
desktop; and open notes with their positions restored on restart.

### Phase 2 hard-won knowledge (don't relearn the hard way)

Everything here was **measured** with `platform/probe.rs`, not inferred.

- **`GetWindowDesktopId` returns `0x8002802B` for a window that has never been
  shown.** An unshown window is not registered with the virtual-desktop system
  at all. This is why a sticky must be shown *before* it is moved to another
  desktop — otherwise activating it cannot carry the user there.
- **Hiding from the taskbar has two distinct failure modes**, not one:
  - `WS_EX_TOOLWINDOW` — even applied after the window is shown — makes the
    window **unregistered** (`0x8002802B`).
  - `ITaskbarList::DeleteTab` (what `skip_taskbar(true)` uses) leaves the window
    registered but sets its desktop to **GUID_NULL** — which is why it appeared
    on every desktop. `MoveWindowToDesktop` afterwards **returns `Ok` and
    changes nothing**.
- The **working baseline** is a plain borderless window (`decorations(false)`,
  no transparency, no `skip_taskbar`). Sticky notes each get a taskbar button as
  a result, which is accepted — see the decision below.
- **The registry is the fragile half, not COM** — the opposite of this project's
  original assumption. On a GitHub-hosted runner the VD registry keys do not
  exist while `IVirtualDesktopManager` works fine, so `current_desktop()` reads
  the registry first and falls back to COM.
- Registry locations: `HKCU\...\Explorer\VirtualDesktops\{VirtualDesktopIDs,
  CurrentVirtualDesktop,Desktops\{GUID}\Name}`.
- Window geometry must be stored in **logical** pixels. `outerPosition` and
  `innerSize` report physical ones, so mixing them makes restored notes drift
  further across the screen on every restart.

**Decided, not open: stickies keep their taskbar buttons.** Both known ways to
hide them break per-desktop placement in the ways measured above, and the damage
cannot be repaired afterwards. Per-desktop placement is the whole point of the
app; a taskbar button is not worth losing it for. So the plain borderless window
stays as-is by choice, and `skip_taskbar` / `WS_EX_TOOLWINDOW` / transparency
should be treated as things that must not be reintroduced.

## Automated tests

Development is **test-first**. New behaviour starts with a failing test.

| Layer | Tool | Run with |
|-------|------|----------|
| Rust unit | `cargo test` | `cargo test --manifest-path src-tauri/Cargo.toml --lib` |
| Rust coverage | `cargo-llvm-cov` | see below |
| Frontend | Vitest 4 + RTL + jsdom | `npm test` |
| Frontend coverage | `@vitest/coverage-v8` | `npm run test:coverage` |

Both run on every pull request (`.github/workflows/ci.yml`, `windows-latest`).

```powershell
cargo llvm-cov --manifest-path src-tauri/Cargo.toml --lib `
  --ignore-filename-regex '(windows\.rs|probe\.rs|[\\/]lib\.rs|main\.rs)' `
  --fail-under-lines 65
```

### What is deliberately not covered

- `platform/windows.rs` — the `unsafe` COM shim. Each line is a passthrough to
  Windows, so a test there asserts Microsoft's behaviour, not ours; and
  windows-rs COM types cannot be mocked. Verified by review and by the probe.
- `platform/probe.rs` — `#[ignore]`d diagnostics whose *output* is the result.
- `lib.rs` / `main.rs` — bootstrap, tray wiring, polling plumbing.

`commands/` is **kept** in the denominator. Its pure parts (`desktop_menu`) are
fully covered; what remains is Tauri window/menu plumbing and COM calls, which
is shim in the same sense as `platform/windows.rs`.

100% is an explicit non-goal — the correlation between coverage and defects
disappears above roughly 70-80%.

### Testing OS behaviour

Virtual-desktop behaviour cannot be faked, so it is spiked rather than
mocked: `platform/probe.rs` asks the OS directly and its output is the
finding. Run it with
`cargo test --lib probe -- --ignored --nocapture`.

Three results from it are worth knowing:

- **The VD COM API works on a GitHub-hosted runner; the VD registry keys do
  not exist there.** The opposite of this codebase's original assumption, and
  why `current_desktop()` falls back from registry to COM.
- **`GetWindowDesktopId` returns `0x8002802B` for a window that has never been
  shown** — an unshown window is not registered with the virtual-desktop
  system. This is why a sticky must be shown *before* it is moved to another
  desktop.
- **Taskbar hiding fails in two different ways** — `WS_EX_TOOLWINDOW`
  unregisters the window outright, while `DeleteTab` leaves it registered with
  a null desktop. See the Phase 2 notes above; the distinction matters, because
  only the first shows up as an error.

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

1. **Start Phase 3** — implement `yrs_manager` (CRDT docs persisted to the
   `yjs_state` BLOB) before touching transport/LAN. The port-and-fake pattern
   used for virtual desktops applies directly to transport.
2. **Guard against a null desktop id** — a window whose desktop has been
   cleared to GUID_NULL reports `{00000000-...}`, which
   `get_sticky_desktop_id` would happily return and the frontend would store in
   `desktop_id`. Nothing rejects it today.
3. **Wire up sharing command registry** — `commands/sharing.rs` exists and is
   declared but isn't registered in `lib.rs`'s `invoke_handler`.
4. **Do not reintroduce taskbar hiding.** Measured and decided against - see
   the Phase 2 notes. If Windows ever exposes a supported way to drop a taskbar
   button without touching desktop assignment, the probe to re-check it already
   exists.

### Working agreements

- **GitHub flow**: issue → branch → granular commits → PR → green CI. Nothing
  lands on `main` directly.
- **Test-first for logic; spike-first at the OS boundary.** Where the answer
  depends on what Windows does, write a probe and measure — do not guess, and do
  not mock the OS into agreeing with you.

## Open risks / notes

- Heavy reliance on Windows-only APIs; `platform/macos.rs` and `linux.rs` are
  stubs, so the app is effectively Windows-only today.
- The desktop monitor is a **500ms polling thread** — fine for now, but a
  source of latency/CPU if scaled up.
- DB schema already includes `sharing_tier` / `share_key` columns anticipating
  Phases 3–4, but no sharing logic exists yet.
</content>
</invoke>
