// Phase 2: Virtual desktop Tauri commands.

use tauri::Manager;

#[cfg(target_os = "windows")]
use crate::platform::windows::{DesktopMonitorState, VirtualDesktopService};

/// List all virtual desktops with names and which is current.
#[tauri::command]
pub fn list_desktops() -> Result<Vec<serde_json::Value>, String> {
    #[cfg(target_os = "windows")]
    {
        use crate::platform::windows::list_desktops_from_registry;
        let desktops = list_desktops_from_registry()?;
        Ok(desktops
            .into_iter()
            .map(|d| {
                serde_json::json!({
                    "id": d.id,
                    "name": d.name,
                    "is_current": d.is_current,
                })
            })
            .collect())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(vec![])
    }
}

/// Get the GUID of the currently active virtual desktop.
#[tauri::command]
pub fn get_current_desktop_id() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        use crate::platform::windows::get_current_desktop_from_registry;
        get_current_desktop_from_registry()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(String::new())
    }
}

/// Get the virtual desktop GUID that a sticky's window is currently on.
#[tauri::command]
pub fn get_sticky_desktop_id(app: tauri::AppHandle, sticky_id: String) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let label = format!("sticky-{}", sticky_id);
        let window = app
            .get_webview_window(&label)
            .ok_or_else(|| format!("Window not found: {label}"))?;
        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        let vds = app.state::<VirtualDesktopService>();
        vds.get_desktop_id(hwnd.0 as isize)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, sticky_id);
        Ok(String::new())
    }
}

/// Move a sticky's window to a different virtual desktop.
#[tauri::command]
pub fn move_sticky_to_desktop(
    app: tauri::AppHandle,
    sticky_id: String,
    desktop_id: String,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let label = format!("sticky-{}", sticky_id);
        let window = app
            .get_webview_window(&label)
            .ok_or_else(|| format!("Window not found: {label}"))?;
        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        let vds = app.state::<VirtualDesktopService>();
        vds.move_to_desktop(hwnd.0 as isize, &desktop_id)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, sticky_id, desktop_id);
        Ok(())
    }
}

/// Set which desktops a sticky should appear on.
/// desktop_ids: list of desktop GUIDs, or ["*"] for all desktops.
/// Empty list means current desktop only (no monitoring).
#[tauri::command]
pub fn set_sticky_desktops(
    app: tauri::AppHandle,
    sticky_id: String,
    desktop_ids: Vec<String>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let state = app.state::<DesktopMonitorState>();
        let mut map = state.sticky_desktops.lock().map_err(|e| e.to_string())?;
        if desktop_ids.is_empty() {
            map.remove(&sticky_id);
        } else {
            let set: std::collections::HashSet<String> = desktop_ids.into_iter().collect();
            map.insert(sticky_id, set);
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, sticky_id, desktop_ids);
        Ok(())
    }
}

/// Show a native popup menu with virtual desktop choices.
/// The menu is built with Tauri's Menu API and shown via popup_menu().
/// Menu item clicks are handled by the global on_menu_event in lib.rs,
/// which emits a "desktop-menu-action" event to the frontend.
#[tauri::command]
pub async fn show_desktop_menu(
    app: tauri::AppHandle,
    sticky_id: String,
    current_desktop_id: String,
) -> Result<(), String> {
    use tauri::menu::{CheckMenuItemBuilder, MenuBuilder};

    #[cfg(target_os = "windows")]
    let desktops = {
        use crate::platform::windows::list_desktops_from_registry;
        list_desktops_from_registry().unwrap_or_default()
    };
    #[cfg(not(target_os = "windows"))]
    let desktops: Vec<crate::platform::windows::DesktopInfo> = vec![];

    // What the menu should offer, and what is ticked, is decided by
    // desktop_menu - which is pure and tested, and owns the item-id format that
    // lib.rs parses back.
    let choices: Vec<super::desktop_menu::DesktopChoice> = desktops
        .iter()
        .map(|d| super::desktop_menu::DesktopChoice {
            id: d.id.clone(),
            name: d.name.clone(),
            is_current: d.is_current,
        })
        .collect();
    let entries = super::desktop_menu::menu_entries(&sticky_id, &choices, &current_desktop_id);

    let label = format!("sticky-{}", sticky_id);
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("Window not found: {label}"))?;

    let mut menu_builder = MenuBuilder::new(&app);

    // The last entry is always "All desktops"; it gets a separator before it.
    let last = entries.len().saturating_sub(1);
    for (index, entry) in entries.iter().enumerate() {
        if index == last && last > 0 {
            menu_builder = menu_builder.separator();
        }
        let item = CheckMenuItemBuilder::with_id(&entry.id, &entry.label)
            .checked(entry.checked)
            .build(&app)
            .map_err(|e| e.to_string())?;
        menu_builder = menu_builder.item(&item);
    }

    let menu = menu_builder.build().map_err(|e| e.to_string())?;

    log::info!("Showing desktop menu for sticky {sticky_id}");
    window.popup_menu(&menu).map_err(|e| e.to_string())?;
    log::info!("Desktop menu closed for sticky {sticky_id}");

    Ok(())
}
