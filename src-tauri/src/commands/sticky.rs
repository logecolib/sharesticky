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
        // Created unfocused on purpose. A window that is *already* foreground
        // cannot be activated, and activation is the only documented way to
        // carry the user to another virtual desktop - so a window that grabs
        // focus on creation makes the later SetForegroundWindow a silent no-op.
        // This is what made travelling to a sticky's desktop work only on a
        // second click, once the window existed and no longer had focus.
        .focused(false)
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

    // Show *before* moving. A window that has never been displayed is not
    // registered with the virtual-desktop system at all - the OS reports
    // 0x8002802B for it - so moving it first re-homes something the shell
    // barely knows about, and the later activation has nothing to follow. This
    // is what made the first click on a sticky fail while the second worked:
    // by then the window already existed, shown, on the target desktop.
    window.show().map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    if let Some(target) = desktop_id.as_deref() {
        use crate::platform::virtual_desktops::{
            wait_until_registered, VirtualDesktops, WindowHandle,
        };

        let hwnd = WindowHandle(window.hwnd().map_err(|e| e.to_string())?.0 as isize);
        let vds = app.state::<crate::platform::windows::VirtualDesktopService>();
        let now = std::time::Instant::now;
        let sleep: fn(std::time::Duration) = std::thread::sleep;

        // Ask until the shell acknowledges the window rather than sleeping a
        // guess at how long that takes. Moving a window the virtual-desktop
        // system does not yet know about reports success and achieves nothing -
        // which is what made this work only on a second click.
        if let Err(e) = wait_until_registered(&*vds, hwnd, None, now, sleep) {
            log::warn!("{label} never registered with the desktop system: {e}");
        }

        vds.move_window_to(hwnd, target).map_err(|e| e.to_string())?;

        // And wait for the move to actually take, so the activation below has
        // somewhere to carry the user to.
        match wait_until_registered(&*vds, hwnd, Some(target), now, sleep) {
            Ok(id) => log::info!("Moved {label} to desktop {id}, activating"),
            Err(e) => log::warn!("{label} did not settle on {target}: {e}"),
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = desktop_id;

    // Activating is what carries the user to the window's desktop. Go straight
    // to SetForegroundWindow rather than through set_focus, so the return value
    // is visible - a refusal here is silent otherwise, and looks identical to
    // "the OS does not follow activation across desktops".
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

        use crate::platform::windows::get_current_desktop_from_registry;

        let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0 as *mut _);

        // Activating is what carries the user across desktops. It does not
        // always take on the first attempt, so check whether it actually
        // happened rather than assuming, and try again if not.
        let attempts = if desktop_id.is_some() { 3 } else { 1 };
        for attempt in 1..=attempts {
            let already_foreground = unsafe { GetForegroundWindow() } == hwnd;
            let brought = unsafe { SetForegroundWindow(hwnd) };

            let landed = match desktop_id.as_deref() {
                None => true,
                Some(target) => {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    get_current_desktop_from_registry()
                        .map(|now| now.eq_ignore_ascii_case(target))
                        .unwrap_or(false)
                }
            };

            log::info!(
                "activate {label} attempt {attempt}/{attempts}: \
                 already_foreground={already_foreground} setfg={} landed={landed}",
                brought.as_bool()
            );

            if landed {
                break;
            }
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
