//! Diagnostic probe: does the Windows virtual-desktop system work *here*?
//!
//! Not a test of our code. It asks the operating system a question we could not
//! answer from documentation: whether virtual desktops function in an
//! environment with no interactive shell session, such as a CI runner.
//!
//! It is `#[ignore]`d so it never runs in the normal suite, and it never fails -
//! its output *is* the result. Run it with:
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml --lib probe -- --ignored --nocapture
//! ```
//!
//! Run it on a developer machine first to establish the control, then on the
//! target environment, and compare.

#![cfg(target_os = "windows")]

#[cfg(test)]
mod tests {
    use windows::core::w;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::IVirtualDesktopManager;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetShellWindow, ShowWindow, SW_SHOWNA, WINDOW_EX_STYLE,
        WS_OVERLAPPEDWINDOW,
    };

    use crate::platform::windows::{get_current_desktop_from_registry, list_desktops_from_registry};

    fn report(label: &str, outcome: Result<String, String>) {
        match outcome {
            Ok(v) => println!("  [ ok ] {label}: {v}"),
            Err(e) => println!("  [FAIL] {label}: {e}"),
        }
    }

    #[test]
    #[ignore = "diagnostic probe; run explicitly with --ignored --nocapture"]
    fn probe_virtual_desktop_availability() {
        println!("\n=== Virtual desktop availability probe ===");
        println!(
            "host: {} | session: {:?}",
            std::env::var("COMPUTERNAME").unwrap_or_else(|_| "?".into()),
            std::env::var("SESSIONNAME").ok()
        );
        println!("CI env var present: {}", std::env::var("CI").is_ok());

        // 1. Is there a shell at all? Virtual desktops are implemented by the
        //    shell, so no shell window is a strong signal on its own.
        println!("\n-- shell --");
        let shell = unsafe { GetShellWindow() };
        if shell.0.is_null() {
            println!("  [FAIL] GetShellWindow: null (no shell hosting this session)");
        } else {
            println!("  [ ok ] GetShellWindow: {:?}", shell.0);
        }

        // 2. Registry, which is how we read the desktop list and current desktop.
        println!("\n-- registry --");
        report(
            "CurrentVirtualDesktop",
            get_current_desktop_from_registry(),
        );
        report(
            "VirtualDesktopIDs",
            list_desktops_from_registry()
                .map(|d| format!("{} desktop(s): {:?}", d.len(), d.iter().map(|x| &x.id).collect::<Vec<_>>())),
        );

        // 3. COM. Does the coclass even instantiate without a shell?
        println!("\n-- COM IVirtualDesktopManager --");
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let manager: Option<IVirtualDesktopManager> = unsafe {
            match CoCreateInstance(
                &windows::core::GUID::from_u128(0xAA509086_5CA9_4C25_8F95_589D3C07B48A),
                None,
                CLSCTX_ALL,
            ) {
                Ok(m) => {
                    println!("  [ ok ] CoCreateInstance succeeded");
                    Some(m)
                }
                Err(e) => {
                    println!("  [FAIL] CoCreateInstance: {e}");
                    None
                }
            }
        };

        // 4. The decisive call: does GetWindowDesktopId work on a real window?
        //    A coclass that instantiates but rejects every window would look
        //    healthy in steps 1-3 and still be useless to us.
        println!("\n-- window queries --");
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("vd-probe"),
                WS_OVERLAPPEDWINDOW,
                0,
                0,
                100,
                100,
                None,
                None,
                None,
                None,
            )
        };

        match (hwnd, manager.as_ref()) {
            (Ok(h), Some(m)) if !h.0.is_null() => {
                println!("  [ ok ] CreateWindowExW: {:?}", h.0);

                // A window that has never been shown is not registered with the
                // virtual-desktop system, so query it both ways to tell
                // "unregistered window" apart from "no VD system here".
                println!("  (before ShowWindow)");
                probe_window(m, h);

                let _ = unsafe { ShowWindow(h, SW_SHOWNA) };
                std::thread::sleep(std::time::Duration::from_millis(250));
                println!("  (after ShowWindow)");
                probe_window(m, h);

                let _ = unsafe { DestroyWindow(h) };
            }
            (Ok(h), None) => {
                println!("  [ ok ] CreateWindowExW: {:?} (no COM manager to query with)", h.0);
                let _ = unsafe { DestroyWindow(h) };
            }
            (Err(e), _) => println!("  [FAIL] CreateWindowExW: {e}"),
            (Ok(_), Some(_)) => println!("  [FAIL] CreateWindowExW returned a null handle"),
        }

        // Also try the shell window, which definitely exists on a real desktop.
        if let (Some(m), false) = (manager.as_ref(), shell.0.is_null()) {
            println!("\n-- window queries (shell window) --");
            probe_window(m, shell);
        }

        println!("\n=== end probe ===\n");
    }

    /// Does activating a window that lives on another virtual desktop make
    /// Windows switch to that desktop?
    ///
    /// This matters because there is **no documented API to switch the active
    /// virtual desktop**. `IVirtualDesktopManager` only moves windows.
    /// Activation-follows is the only route that does not require the
    /// undocumented `IVirtualDesktopManagerInternal`.
    ///
    /// Moves the screen between desktops while running, and puts it back.
    #[test]
    #[ignore = "diagnostic probe; switches virtual desktops. Run with --ignored --nocapture"]
    fn probe_whether_activating_a_window_switches_desktop() {
        use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, SwitchToThisWindow};

        use crate::platform::windows::VirtualDesktopService;

        println!("\n=== Does activating a window switch virtual desktop? ===");

        let vds = match VirtualDesktopService::new_with_com_init() {
            Ok(v) => v,
            Err(e) => {
                println!("  [SKIP] no virtual desktop service: {e}");
                return;
            }
        };

        let start = match get_current_desktop_from_registry() {
            Ok(id) => id,
            Err(e) => {
                println!("  [SKIP] cannot read current desktop: {e}");
                return;
            }
        };
        let desktops = list_desktops_from_registry().unwrap_or_default();
        let target = match desktops.iter().find(|d| d.id != start) {
            Some(d) => d.id.clone(),
            None => {
                println!("  [SKIP] need at least two desktops; found {}", desktops.len());
                return;
            }
        };

        println!("  start desktop:  {start}");
        println!("  target desktop: {target}");

        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("vd-switch-probe"),
                WS_OVERLAPPEDWINDOW,
                100,
                100,
                300,
                200,
                None,
                None,
                None,
                None,
            )
        };
        let hwnd = match hwnd {
            Ok(h) if !h.0.is_null() => h,
            _ => {
                println!("  [FAIL] could not create a probe window");
                return;
            }
        };

        // Must be shown, or it is not registered with the VD system at all.
        let _ = unsafe { ShowWindow(hwnd, SW_SHOWNA) };
        std::thread::sleep(std::time::Duration::from_millis(300));

        if let Err(e) = vds.move_to_desktop(hwnd.0 as isize, &target) {
            println!("  [FAIL] MoveWindowToDesktop: {e}");
            let _ = unsafe { DestroyWindow(hwnd) };
            return;
        }
        println!("  [ ok ] moved probe window to the target desktop");
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Windows only grants SetForegroundWindow to a process that already owns
        // the foreground or received the last input event. A test binary run
        // from a terminal has neither, and the call is silently refused - which
        // would look identical to "activation does not switch desktops".
        //
        // Synthesising a harmless keypress gives this process the input claim,
        // putting it in the same position as the real app when the user has
        // just clicked the manager window.
        unsafe {
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                keybd_event, KEYEVENTF_KEYUP, VK_MENU,
            };
            keybd_event(VK_MENU.0 as u8, 0, Default::default(), 0);
            keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        }
        std::thread::sleep(std::time::Duration::from_millis(120));

        // Attempt 1: the documented activation call.
        let brought = unsafe { SetForegroundWindow(hwnd) };
        std::thread::sleep(std::time::Duration::from_millis(900));
        let after_setfg = get_current_desktop_from_registry().unwrap_or_default();
        println!(
            "  SetForegroundWindow returned {} -> current desktop is now {}",
            brought.as_bool(),
            after_setfg
        );
        println!(
            "    => {}",
            if after_setfg == target {
                "SWITCHED to the window's desktop"
            } else {
                "did NOT switch"
            }
        );

        // Attempt 2: only if the documented call did not do it.
        if after_setfg != target {
            unsafe { SwitchToThisWindow(hwnd, true) };
            std::thread::sleep(std::time::Duration::from_millis(900));
            let after_switch = get_current_desktop_from_registry().unwrap_or_default();
            println!("  SwitchToThisWindow -> current desktop is now {after_switch}");
            println!(
                "    => {}",
                if after_switch == target {
                    "SWITCHED (but via a call Microsoft documents as not for general use)"
                } else {
                    "did NOT switch"
                }
            );
        }

        // Put the machine back where we found it.
        println!("\n  restoring the starting desktop...");
        let _ = vds.move_to_desktop(hwnd.0 as isize, &start);
        std::thread::sleep(std::time::Duration::from_millis(300));
        // The input claim is consumed by the first activation, so take a fresh
        // one - otherwise this call is refused and the machine is left on the
        // target desktop.
        unsafe {
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                keybd_event, KEYEVENTF_KEYUP, VK_MENU,
            };
            keybd_event(VK_MENU.0 as u8, 0, Default::default(), 0);
            keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
        let _ = unsafe { SetForegroundWindow(hwnd) };
        std::thread::sleep(std::time::Duration::from_millis(900));
        let _ = unsafe { DestroyWindow(hwnd) };
        std::thread::sleep(std::time::Duration::from_millis(300));

        let ended_on = get_current_desktop_from_registry().unwrap_or_default();
        println!(
            "  ended on {} ({})",
            ended_on,
            if ended_on == start { "restored" } else { "NOT restored - switch back manually" }
        );

        println!("=== end switch probe ===\n");
    }

    fn probe_window(manager: &IVirtualDesktopManager, hwnd: HWND) {
        unsafe {
            match manager.GetWindowDesktopId(hwnd) {
                Ok(guid) => println!("  [ ok ] GetWindowDesktopId: {:?}", guid),
                Err(e) => println!("  [FAIL] GetWindowDesktopId: {e}"),
            }
            match manager.IsWindowOnCurrentVirtualDesktop(hwnd) {
                Ok(v) => println!("  [ ok ] IsWindowOnCurrentVirtualDesktop: {}", v.as_bool()),
                Err(e) => println!("  [FAIL] IsWindowOnCurrentVirtualDesktop: {e}"),
            }
        }
    }
}
