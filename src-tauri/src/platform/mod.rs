// Platform-specific virtual desktop detection.
//
// Each platform module provides desktop awareness:
// - Windows: IVirtualDesktopManager COM interface
// - macOS: CGSSpace / NSWorkspace APIs
// - Linux: X11 _NET_CURRENT_DESKTOP / Wayland ext-workspace

// Pure desktop-id encoding. Platform-independent on purpose so it can be
// unit tested anywhere; the windows-crate types stay in the adapter below.
pub mod desktop_id;
pub mod placement;
pub mod virtual_desktops;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;
