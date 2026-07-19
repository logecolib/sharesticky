mod commands;
mod platform;
mod storage;
mod sync;

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager,
};

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_sql::Builder::new()
                .add_migrations(
                    "sqlite:sharesticky.db",
                    vec![tauri_plugin_sql::Migration {
                        version: 1,
                        description: "create stickies table",
                        sql: storage::database::MIGRATION_V1,
                        kind: tauri_plugin_sql::MigrationKind::Up,
                    }],
                )
                .build(),
        )
        .setup(|app| {
            // Build tray menu
            let new_sticky = MenuItemBuilder::with_id("new_sticky", "New Sticky").build(app)?;
            let show_manager =
                MenuItemBuilder::with_id("show_manager", "Show Manager").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&new_sticky)
                .item(&show_manager)
                .separator()
                .item(&quit)
                .build()?;

            // Build tray icon
            let _tray = TrayIconBuilder::new()
                .tooltip("ShareSticky")
                .icon(app.default_window_icon().cloned().unwrap())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "new_sticky" => {
                        // Emit event to frontend to create a new sticky via SQL plugin
                        let _ = app.emit("tray-new-sticky", ());
                    }
                    "show_manager" => {
                        if let Some(window) = app.get_webview_window("manager") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("manager") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Show manager window on startup
            if let Some(window) = app.get_webview_window("manager") {
                let _ = window.show();
            }

            // Phase 2: Initialize virtual desktop service (monitor disabled for testing)
            #[cfg(target_os = "windows")]
            {
                setup_desktop_monitor(app)?;
                // NOTE: monitor thread is started inside setup_desktop_monitor
                // but won't do anything harmful with plain windows
            }

            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().0.to_string();
            // Handle virtual desktop menu items: "vd:<sticky_id>:<desktop_guid_or_*>"
            if let Some(rest) = id.strip_prefix("vd:") {
                if let Some((sticky_id, desktop_id)) = rest.split_once(':') {
                    log::info!("Desktop menu action: sticky={sticky_id} desktop={desktop_id}");
                    let _ = app.emit("desktop-menu-action", serde_json::json!({
                        "sticky_id": sticky_id,
                        "desktop_id": desktop_id,
                    }));
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::sticky::open_sticky_window,
            commands::sticky::close_sticky_window,
            commands::desktop::list_desktops,
            commands::desktop::get_current_desktop_id,
            commands::desktop::get_sticky_desktop_id,
            commands::desktop::move_sticky_to_desktop,
            commands::desktop::set_sticky_desktops,
            commands::desktop::show_desktop_menu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ShareSticky");
}

/// Initialize the Windows virtual desktop service and spawn a background
/// monitor thread that moves pinned stickies to the active desktop.
#[cfg(target_os = "windows")]
fn setup_desktop_monitor(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use platform::windows::{DesktopMonitorState, VirtualDesktopService};

    // Create VDS on the main thread (COM already initialized by Tauri/WebView2)
    let vds = VirtualDesktopService::new()
        .map_err(|e| format!("Desktop service init failed: {e}"))?;

    let monitor_state = DesktopMonitorState::new();

    // Register managed state so commands can access them
    app.manage(vds);
    app.manage(monitor_state);

    // Spawn the polling monitor thread
    let app_handle = app.handle().clone();
    std::thread::spawn(move || {
        desktop_monitor_loop(app_handle);
    });

    Ok(())
}

/// Background loop that polls every 500ms. Reads the current desktop from the
/// registry (fast, no temp windows). For "all desktops" stickies, moves them
/// to follow the user. Emits `desktop-changed` events.
#[cfg(target_os = "windows")]
fn desktop_monitor_loop(app: tauri::AppHandle) {
    use platform::placement::ALL_DESKTOPS;
    use platform::virtual_desktops::{reconcile, VirtualDesktops, WindowHandle};
    use platform::windows::{DesktopMonitorState, VirtualDesktopService};

    // The manager window is treated as a sticky pinned to every desktop.
    let follows_everywhere: std::collections::HashSet<String> =
        std::iter::once(ALL_DESKTOPS.to_string()).collect();

    // Each thread needs its own COM init + VDS instance for MoveWindowToDesktop
    let vds = match VirtualDesktopService::new_with_com_init() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[desktop-monitor] Failed to init: {e}");
            return;
        }
    };

    let mut last_desktop_id = String::new();

    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Asked through the port rather than the registry directly, so the loop
        // depends on the trait and not on how Windows happens to store this.
        let current_id = match vds.current_desktop() {
            Ok(id) => id,
            Err(_) => continue,
        };

        // Emit event if desktop changed
        if current_id != last_desktop_id {
            if !last_desktop_id.is_empty() {
                let _ = app.emit("desktop-changed", &current_id);
            }
            last_desktop_id = current_id.clone();

            // The manager is simply pinned to all desktops, so it follows the
            // user. The decision itself lives in platform::placement and is
            // unit tested against a fake; this only supplies the window.
            if let Some(manager) = app.get_webview_window("manager") {
                if let Ok(h) = manager.hwnd() {
                    let _ = reconcile(
                        &vds,
                        WindowHandle(h.0 as isize),
                        &follows_everywhere,
                        &current_id,
                    );
                }
            }
        }

        // Move stickies assigned to the current desktop (or all desktops)
        let state = app.state::<DesktopMonitorState>();
        let sticky_map: Vec<(String, std::collections::HashSet<String>)> = match state.sticky_desktops.lock() {
            Ok(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            Err(_) => continue,
        };

        for (sticky_id, allowed_desktops) in &sticky_map {
            // Check if this sticky should be on the current desktop
            let should_be_here = allowed_desktops.contains("*")
                || allowed_desktops.contains(&current_id);

            if !should_be_here {
                continue;
            }

            let label = format!("sticky-{}", sticky_id);
            let window = match app.get_webview_window(&label) {
                Some(w) => w,
                None => continue,
            };

            let hwnd = match window.hwnd() {
                Ok(h) => h.0 as isize,
                Err(_) => continue,
            };

            let on_current = match vds.is_on_current_desktop(hwnd) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if !on_current {
                let _ = vds.move_to_desktop(hwnd, &current_id);
            }
        }
    }
}
