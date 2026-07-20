use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickyWindowOptions {
    pub id: String,
    pub position_x: f64,
    pub position_y: f64,
    pub width: f64,
    pub height: f64,
    pub pinned: bool,
}

/// Make sure a sticky's window exists, without activating it.
///
/// Activation is kept separate on purpose. Windows grants a process the right
/// to call SetForegroundWindow only while it holds the input claim from the
/// user's last action, and **that claim is consumed by the first activation**.
/// So a flow that focuses on create and then focuses again after moving the
/// window has its second call silently refused - which looks exactly like
/// "activation does not switch desktops".
fn ensure_sticky_window(
    app: &tauri::AppHandle,
    options: &StickyWindowOptions,
) -> Result<tauri::WebviewWindow, String> {
    let label = format!("sticky-{}", options.id);

    if let Some(window) = app.get_webview_window(&label) {
        return Ok(window);
    }

    let url = tauri::WebviewUrl::App("index.html".into());

    // Absolute minimum: borderless window, NO transparency, NO skip_taskbar,
    // NO owner window hacks. This MUST stay on one virtual desktop.
    let window = tauri::WebviewWindowBuilder::new(app, &label, url)
        .title("Sticky Note")
        .decorations(false)
        .always_on_top(options.pinned)
        .inner_size(options.width, options.height)
        .position(options.position_x, options.position_y)
        .build()
        .map_err(|e| e.to_string())?;

    log::info!("Opened sticky window: {label} (no transparency, no skip_taskbar, no owner)");
    Ok(window)
}

#[tauri::command]
pub async fn open_sticky_window(
    app: tauri::AppHandle,
    options: StickyWindowOptions,
) -> Result<(), String> {
    let label = format!("sticky-{}", options.id);

    if let Some(window) = app.get_webview_window(&label) {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let url = tauri::WebviewUrl::App("index.html".into());

    // Absolute minimum: borderless window, NO transparency, NO skip_taskbar,
    // NO owner window hacks. This MUST stay on one virtual desktop.
    // If it does, we know the base case works and can add taskbar hiding later.
    let _window = tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title("Sticky Note")
        .decorations(false)
        .always_on_top(options.pinned)
        .inner_size(options.width, options.height)
        .position(options.position_x, options.position_y)
        .build()
        .map_err(|e| e.to_string())?;

    log::info!("Opened sticky window: {label} (no transparency, no skip_taskbar, no owner)");
    Ok(())
}

/// Put a sticky's window on a given virtual desktop, then activate it.
///
/// Windows exposes no documented call to switch the active virtual desktop -
/// `IVirtualDesktopManager` only moves windows. Activating a window that lives
/// on another desktop is what makes Windows follow, and it uses documented
/// calls only. Measured in `platform::probe`.
///
/// Note the activation depends on this process holding foreground rights, which
/// it does here because the user has just clicked the manager. A call made
/// without recent user input would be silently refused by the foreground lock.
#[tauri::command]
pub async fn place_and_focus_sticky(
    app: tauri::AppHandle,
    options: StickyWindowOptions,
    desktop_id: Option<String>,
) -> Result<(), String> {
    let label = format!("sticky-{}", options.id);
    // Creates without activating, so the single activation below is the one
    // that spends the input claim.
    let window = ensure_sticky_window(&app, &options)?;

    #[cfg(target_os = "windows")]
    if let Some(target) = desktop_id.as_deref() {
        use crate::platform::virtual_desktops::{VirtualDesktops, WindowHandle};

        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        let vds = app.state::<crate::platform::windows::VirtualDesktopService>();
        vds.move_window_to(WindowHandle(hwnd.0 as isize), target)
            .map_err(|e| e.to_string())?;
        log::info!("Moved {label} to desktop {target} before activating it");

        // Give the shell a moment to finish re-homing the window. Whether this
        // is required is unproven - it did not on its own make activation carry
        // the user across, see the open question on #11.
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    #[cfg(not(target_os = "windows"))]
    let _ = desktop_id;

    window.show().map_err(|e| e.to_string())?;

    // Activating is what carries the user to the window's desktop. Go straight
    // to SetForegroundWindow rather than through set_focus, so the return value
    // is visible - a refusal here is silent otherwise, and looks identical to
    // "the OS does not follow activation across desktops".
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        let brought = unsafe { SetForegroundWindow(HWND(hwnd.0 as *mut _)) };
        log::info!("SetForegroundWindow({label}) -> {}", brought.as_bool());
        if !brought.as_bool() {
            // Foreground was refused; fall back so the window at least shows.
            window.set_focus().map_err(|e| e.to_string())?;
        }
    }
    #[cfg(not(target_os = "windows"))]
    window.set_focus().map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn close_sticky_window(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let label = format!("sticky-{}", id);
    if let Some(window) = app.get_webview_window(&label) {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}
