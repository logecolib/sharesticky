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

#[tauri::command]
pub async fn close_sticky_window(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let label = format!("sticky-{}", id);
    if let Some(window) = app.get_webview_window(&label) {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}
