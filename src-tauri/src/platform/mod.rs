// Platform-specific virtual desktop detection.
//
// Each platform module provides desktop awareness:
// - Windows: IVirtualDesktopManager COM interface
// - macOS: CGSSpace / NSWorkspace APIs
// - Linux: X11 _NET_CURRENT_DESKTOP / Wayland ext-workspace

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;
