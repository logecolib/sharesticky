// Phase 2: Windows virtual desktop awareness.
//
// Uses the documented IVirtualDesktopManager COM interface for window operations
// and the Windows Registry for desktop listing/naming (stable across updates).

use std::sync::Mutex;

use serde::Serialize;
use windows::core::GUID;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    REG_BINARY, REG_SZ, REG_VALUE_TYPE,
};
use windows::Win32::UI::Shell::IVirtualDesktopManager;
use windows::core::{w, PCWSTR};

use super::desktop_id::{format_desktop_id, parse_desktop_id, DesktopId};
use super::virtual_desktops::{DesktopError, VirtualDesktops, WindowHandle};

/// Info about a virtual desktop, returned to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct DesktopInfo {
    pub id: String,
    pub name: String,
    pub is_current: bool,
}

/// Wraps the IVirtualDesktopManager COM interface.
pub struct VirtualDesktopService {
    manager: IVirtualDesktopManager,
}

// IVirtualDesktopManager is an MTA-safe COM object.
unsafe impl Send for VirtualDesktopService {}
unsafe impl Sync for VirtualDesktopService {}

// CLSID for VirtualDesktopManager coclass
const CLSID_VIRTUAL_DESKTOP_MANAGER: GUID = GUID {
    data1: 0xAA509086,
    data2: 0x5CA9,
    data3: 0x4C25,
    data4: [0x8F, 0x95, 0x58, 0x9D, 0x3C, 0x07, 0xB4, 0x8A],
};

impl VirtualDesktopService {
    /// Create a new service. COM must already be initialized on this thread.
    pub fn new() -> Result<Self, String> {
        unsafe {
            let manager: IVirtualDesktopManager =
                CoCreateInstance(&CLSID_VIRTUAL_DESKTOP_MANAGER, None, CLSCTX_ALL)
                    .map_err(|e| format!("Failed to create IVirtualDesktopManager: {e}"))?;
            Ok(Self { manager })
        }
    }

    /// Create a new service, initializing COM first (for background threads).
    pub fn new_with_com_init() -> Result<Self, String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        Self::new()
    }

    /// Check if a window is on the currently active virtual desktop.
    pub fn is_on_current_desktop(&self, hwnd: isize) -> Result<bool, String> {
        unsafe {
            let result = self
                .manager
                .IsWindowOnCurrentVirtualDesktop(HWND(hwnd as *mut _))
                .map_err(|e| format!("IsWindowOnCurrentVirtualDesktop failed: {e}"))?;
            Ok(result.as_bool())
        }
    }

    /// Get the virtual desktop GUID for a window.
    pub fn get_desktop_id(&self, hwnd: isize) -> Result<String, String> {
        unsafe {
            let guid = self
                .manager
                .GetWindowDesktopId(HWND(hwnd as *mut _))
                .map_err(|e| format!("GetWindowDesktopId failed: {e}"))?;
            Ok(guid_to_string(&guid))
        }
    }

    /// Move a window to a different virtual desktop by GUID string.
    pub fn move_to_desktop(&self, hwnd: isize, desktop_guid: &str) -> Result<(), String> {
        let guid = string_to_guid(desktop_guid)?;
        unsafe {
            self.manager
                .MoveWindowToDesktop(HWND(hwnd as *mut _), &guid)
                .map_err(|e| format!("MoveWindowToDesktop failed: {e}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Registry-based desktop listing (stable across Windows updates)
// ---------------------------------------------------------------------------

/// Read all virtual desktops from the registry, including names and which is current.
pub fn list_desktops_from_registry() -> Result<Vec<DesktopInfo>, String> {
    let current_guid = get_current_desktop_from_registry().unwrap_or_default();
    let guids = get_all_desktop_guids()?;

    let mut desktops = Vec::new();
    for (i, guid_str) in guids.iter().enumerate() {
        let name = get_desktop_name(guid_str)
            .unwrap_or_else(|_| format!("Desktop {}", i + 1));
        desktops.push(DesktopInfo {
            id: guid_str.clone(),
            name,
            is_current: *guid_str == current_guid,
        });
    }

    Ok(desktops)
}

/// Read the current virtual desktop GUID from the registry.
pub fn get_current_desktop_from_registry() -> Result<String, String> {
    unsafe {
        let mut key = HKEY::default();
        let path = w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops");
        RegOpenKeyExW(HKEY_CURRENT_USER, path, 0, KEY_READ, &mut key)
            .ok().map_err(|e| format!("Failed to open VirtualDesktops key: {e}"))?;

        let value_name = w!("CurrentVirtualDesktop");
        let mut buf = [0u8; 16];
        let mut buf_size = 16u32;
        let mut value_type = REG_VALUE_TYPE::default();

        let result = RegQueryValueExW(
            key,
            value_name,
            None,
            Some(&mut value_type),
            Some(buf.as_mut_ptr()),
            Some(&mut buf_size),
        );
        let _ = RegCloseKey(key);

        result.ok().map_err(|e| format!("Failed to read CurrentVirtualDesktop: {e}"))?;

        if value_type != REG_BINARY || buf_size != 16 {
            return Err("CurrentVirtualDesktop has unexpected format".into());
        }

        Ok(guid_to_string(&bytes_to_guid(&buf)))
    }
}

/// Read all virtual desktop GUIDs from the registry.
fn get_all_desktop_guids() -> Result<Vec<String>, String> {
    unsafe {
        let mut key = HKEY::default();
        let path = w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops");
        RegOpenKeyExW(HKEY_CURRENT_USER, path, 0, KEY_READ, &mut key)
            .ok().map_err(|e| format!("Failed to open VirtualDesktops key: {e}"))?;

        // First query to get buffer size
        let value_name = w!("VirtualDesktopIDs");
        let mut buf_size = 0u32;
        let mut value_type = REG_VALUE_TYPE::default();

        let _ = RegQueryValueExW(
            key,
            value_name,
            None,
            Some(&mut value_type),
            None,
            Some(&mut buf_size),
        );

        if buf_size == 0 || value_type != REG_BINARY {
            let _ = RegCloseKey(key);
            return Ok(vec![]);
        }

        let mut buf = vec![0u8; buf_size as usize];
        let result = RegQueryValueExW(
            key,
            value_name,
            None,
            None,
            Some(buf.as_mut_ptr()),
            Some(&mut buf_size),
        );
        let _ = RegCloseKey(key);

        result.ok().map_err(|e| format!("Failed to read VirtualDesktopIDs: {e}"))?;

        // Splitting the blob into ids is pure logic, and unit tested.
        Ok(super::desktop_id::parse_desktop_id_list(&buf))
    }
}

/// Read a desktop's custom name from the registry. Returns Err if no custom name set.
fn get_desktop_name(guid_str: &str) -> Result<String, String> {
    unsafe {
        let subkey = format!(
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops\\Desktops\\{}",
            guid_str
        );
        let subkey_wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();

        let mut key = HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey_wide.as_ptr()),
            0,
            KEY_READ,
            &mut key,
        );

        if result.ok().is_err() {
            return Err("Desktop subkey not found".into());
        }

        let value_name = w!("Name");
        let mut buf_size = 0u32;
        let mut value_type = REG_VALUE_TYPE::default();

        let _ = RegQueryValueExW(key, value_name, None, Some(&mut value_type), None, Some(&mut buf_size));

        if buf_size == 0 || value_type != REG_SZ {
            let _ = RegCloseKey(key);
            return Err("No Name value".into());
        }

        let mut buf = vec![0u8; buf_size as usize];
        let result = RegQueryValueExW(
            key,
            value_name,
            None,
            None,
            Some(buf.as_mut_ptr()),
            Some(&mut buf_size),
        );
        let _ = RegCloseKey(key);

        result.ok().map_err(|e| format!("Failed to read Name: {e}"))?;

        // Convert wide string bytes to String (UTF-16LE, null-terminated)
        let wide: Vec<u16> = buf
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let name = String::from_utf16_lossy(&wide)
            .trim_end_matches('\0')
            .to_string();

        if name.is_empty() {
            Err("Empty name".into())
        } else {
            Ok(name)
        }
    }
}

// ---------------------------------------------------------------------------
// GUID helpers
// ---------------------------------------------------------------------------

// The encoding itself lives in `platform::desktop_id`, which is pure and unit
// tested. These are conversions between that representation and the COM type.

/// Convert 16 raw bytes (mixed-endian GUID layout) into a COM GUID struct.
fn bytes_to_guid(bytes: &[u8; 16]) -> GUID {
    GUID {
        data1: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        data2: u16::from_le_bytes([bytes[4], bytes[5]]),
        data3: u16::from_le_bytes([bytes[6], bytes[7]]),
        data4: [
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15],
        ],
    }
}

/// Flatten a COM GUID struct back into its 16 raw bytes.
fn guid_to_bytes(guid: &GUID) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&guid.data1.to_le_bytes());
    bytes[4..6].copy_from_slice(&guid.data2.to_le_bytes());
    bytes[6..8].copy_from_slice(&guid.data3.to_le_bytes());
    bytes[8..16].copy_from_slice(&guid.data4);
    bytes
}

/// Format a GUID as a standard string: {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}
pub fn guid_to_string(guid: &GUID) -> String {
    format_desktop_id(&guid_to_bytes(guid))
}

/// Parse a GUID string back into a GUID struct.
pub fn string_to_guid(s: &str) -> Result<GUID, String> {
    parse_desktop_id(s)
        .map(|bytes| bytes_to_guid(&bytes))
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Adapter: the only place COM is reached, and it makes no decisions
// ---------------------------------------------------------------------------

impl VirtualDesktops for VirtualDesktopService {
    fn current_desktop(&self) -> Result<DesktopId, DesktopError> {
        get_current_desktop_from_registry().map_err(DesktopError::Unavailable)
    }

    fn is_window_on_current(&self, window: WindowHandle) -> Result<bool, DesktopError> {
        self.is_on_current_desktop(window.0).map_err(DesktopError::Failed)
    }

    fn desktop_of_window(&self, window: WindowHandle) -> Result<DesktopId, DesktopError> {
        self.get_desktop_id(window.0).map_err(DesktopError::Failed)
    }

    fn move_window_to(&self, window: WindowHandle, desktop: &str) -> Result<(), DesktopError> {
        self.move_to_desktop(window.0, desktop).map_err(DesktopError::Failed)
    }
}

// ---------------------------------------------------------------------------
// Desktop monitor shared state
// ---------------------------------------------------------------------------

/// Shared state for the desktop monitor.
/// Maps sticky_id -> set of desktop GUIDs the sticky should appear on.
/// A set containing "*" means all desktops.
/// An empty set means the sticky stays on its current desktop only (no monitoring).
pub struct DesktopMonitorState {
    pub sticky_desktops: Mutex<std::collections::HashMap<String, std::collections::HashSet<String>>>,
}

impl DesktopMonitorState {
    pub fn new() -> Self {
        Self {
            sticky_desktops: Mutex::new(std::collections::HashMap::new()),
        }
    }
}
