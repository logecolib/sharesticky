//! The port between our logic and the OS virtual-desktop system.
//!
//! Everything above this trait is ordinary testable Rust. Everything below it
//! is a thin adapter containing `unsafe` COM calls and no decisions at all -
//! see `platform::windows::Win32VirtualDesktops`.
//!
//! Deliberately no `windows`-crate types appear in these signatures. If `HWND`
//! or `GUID` leaked into the trait, every caller would transitively depend on
//! the OS crate and we would have moved the problem rather than solved it.

use std::collections::HashSet;

use super::availability::{resolve_current_desktop, Resolved};
use super::desktop_id::DesktopId;
use super::placement::{placement_for, Placement};

/// An opaque OS window handle. On Windows this wraps an `HWND`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowHandle(pub isize);

/// Why a virtual-desktop operation did not succeed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopError {
    /// The virtual-desktop subsystem is not available at all.
    ///
    /// Expected on machines with no interactive shell session - CI runners,
    /// service sessions. Callers should degrade gracefully rather than treat
    /// this as a fault.
    Unavailable(String),
    /// The subsystem is present but rejected this specific call.
    Failed(String),
}

impl std::fmt::Display for DesktopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(s) => write!(f, "virtual desktops unavailable: {s}"),
            Self::Failed(s) => write!(f, "virtual desktop operation failed: {s}"),
        }
    }
}

impl std::error::Error for DesktopError {}

/// Operations we need from the OS virtual-desktop system.
pub trait VirtualDesktops: Send + Sync {
    /// The desktop the user is currently looking at.
    fn current_desktop(&self) -> Result<DesktopId, DesktopError>;

    /// Whether a window is on the desktop the user is currently looking at.
    fn is_window_on_current(&self, window: WindowHandle) -> Result<bool, DesktopError>;

    /// Which desktop a window sits on.
    ///
    /// Note this only answers for a window that has been shown; an unshown
    /// window is not registered with the virtual-desktop system and the OS
    /// reports `0x8002802B` for it.
    fn desktop_of_window(&self, window: WindowHandle) -> Result<DesktopId, DesktopError>;

    /// Move a window onto a specific desktop.
    fn move_window_to(&self, window: WindowHandle, desktop: &str) -> Result<(), DesktopError>;
}

/// Work out the current desktop, falling back to COM when the registry cannot
/// answer.
///
/// `reference` is a window we own that has already been shown; it is what the
/// COM fallback queries. Without one, only the registry can answer.
pub fn current_desktop_with_fallback<V: VirtualDesktops + ?Sized>(
    desktops: &V,
    reference: Option<WindowHandle>,
) -> Result<Resolved, DesktopError> {
    resolve_current_desktop(desktops.current_desktop(), || match reference {
        Some(window) => desktops.desktop_of_window(window),
        None => Err(DesktopError::Unavailable(
            "no reference window to query".into(),
        )),
    })
}

/// How long to keep asking before giving up on a window settling.
const SETTLE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);
const SETTLE_POLL: std::time::Duration = std::time::Duration::from_millis(25);

/// Wait until the virtual-desktop system acknowledges a window.
///
/// A window that has just been shown is not registered immediately, and until
/// it is, moving it does nothing useful - the move reports success and the
/// later activation has nothing to follow. Sleeping a fixed guess makes that a
/// race; asking until the answer arrives does not.
///
/// `wanted` is the desktop we expect it to report, or `None` to accept any.
pub fn wait_until_registered<V: VirtualDesktops + ?Sized>(
    desktops: &V,
    window: WindowHandle,
    wanted: Option<&str>,
    now: impl Fn() -> std::time::Instant,
    sleep: impl Fn(std::time::Duration),
) -> Result<DesktopId, DesktopError> {
    let started = now();
    // Overwritten before it is ever read; kept so the timeout reports the most
    // recent reason rather than a generic one.
    let mut last;

    loop {
        match desktops.desktop_of_window(window) {
            Ok(id) => match wanted {
                // An all-zero id means the window is registered but assigned to
                // no desktop, which is what taskbar hiding does. Never settle
                // on it.
                _ if id.trim_matches(|c| c == '{' || c == '}').replace(['-', '0'], "").is_empty() => {
                    last = DesktopError::Failed(format!("window reports a null desktop ({id})"));
                }
                Some(target) if !id.eq_ignore_ascii_case(target) => {
                    last = DesktopError::Failed(format!("window is on {id}, waiting for {target}"));
                }
                _ => return Ok(id),
            },
            Err(e) => last = e,
        }

        if now().duration_since(started) >= SETTLE_TIMEOUT {
            return Err(last);
        }
        sleep(SETTLE_POLL);
    }
}

/// Bring one window in line with where it is supposed to be.
///
/// This is the whole body of the desktop monitor's inner loop, minus the sleep
/// and the window lookup - which is what makes it testable against a fake.
pub fn reconcile<V: VirtualDesktops + ?Sized>(
    desktops: &V,
    window: WindowHandle,
    assigned: &HashSet<DesktopId>,
    current_desktop: &str,
) -> Result<Placement, DesktopError> {
    let is_on_current = desktops.is_window_on_current(window)?;
    let action = placement_for(assigned, current_desktop, is_on_current);

    if action == Placement::MoveToCurrentDesktop {
        desktops.move_window_to(window, current_desktop)?;
    }

    Ok(action)
}

#[cfg(test)]
pub mod fake {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    /// An in-memory stand-in for the OS virtual-desktop system.
    pub struct FakeVirtualDesktops {
        current: Mutex<DesktopId>,
        /// Which desktop each window currently sits on.
        windows: Mutex<HashMap<WindowHandle, DesktopId>>,
        /// Every move that was performed, in order, for assertions.
        pub moves: Mutex<Vec<(WindowHandle, DesktopId)>>,
        /// When set, every call fails this way.
        unavailable: Mutex<Option<String>>,
        /// When set, `current_desktop` fails but window queries still work -
        /// the environment measured on a GitHub-hosted runner.
        no_registry: Mutex<bool>,
        /// How many more times a window should report itself unregistered,
        /// mimicking the delay after a window is first shown.
        unregistered_for: Mutex<HashMap<WindowHandle, u32>>,
    }

    impl FakeVirtualDesktops {
        pub fn new(current: &str) -> Self {
            Self {
                current: Mutex::new(current.to_string()),
                windows: Mutex::new(HashMap::new()),
                moves: Mutex::new(Vec::new()),
                unavailable: Mutex::new(None),
                no_registry: Mutex::new(false),
                unregistered_for: Mutex::new(HashMap::new()),
            }
        }

        /// Make a window report itself unregistered for the next `polls` asks,
        /// as a freshly shown window does.
        pub fn unregistered_for(self, window: WindowHandle, polls: u32) -> Self {
            self.unregistered_for.lock().unwrap().insert(window, polls);
            self
        }

        /// Simulate a window whose desktop has been cleared to null, which is
        /// what hiding it from the taskbar does.
        pub fn with_null_desktop(self, window: WindowHandle) -> Self {
            self.windows
                .lock()
                .unwrap()
                .insert(window, "{00000000-0000-0000-0000-000000000000}".to_string());
            self
        }

        /// Simulate an environment whose virtual-desktop registry keys are
        /// absent while COM still answers - measured on `windows-latest`.
        pub fn without_registry(self) -> Self {
            *self.no_registry.lock().unwrap() = true;
            self
        }

        /// Place a window on a desktop without recording it as a move.
        pub fn with_window_on(self, window: WindowHandle, desktop: &str) -> Self {
            self.windows.lock().unwrap().insert(window, desktop.to_string());
            self
        }

        /// Simulate a machine with no virtual-desktop support, e.g. a CI runner.
        pub fn unavailable(self, why: &str) -> Self {
            *self.unavailable.lock().unwrap() = Some(why.to_string());
            self
        }

        /// Simulate the user switching desktops.
        pub fn switch_to(&self, desktop: &str) {
            *self.current.lock().unwrap() = desktop.to_string();
        }

        pub fn desktop_of(&self, window: WindowHandle) -> Option<DesktopId> {
            self.windows.lock().unwrap().get(&window).cloned()
        }

        pub fn move_count(&self) -> usize {
            self.moves.lock().unwrap().len()
        }

        fn guard(&self) -> Result<(), DesktopError> {
            match self.unavailable.lock().unwrap().as_ref() {
                Some(why) => Err(DesktopError::Unavailable(why.clone())),
                None => Ok(()),
            }
        }
    }

    impl VirtualDesktops for FakeVirtualDesktops {
        fn current_desktop(&self) -> Result<DesktopId, DesktopError> {
            self.guard()?;
            if *self.no_registry.lock().unwrap() {
                return Err(DesktopError::Unavailable(
                    "The system cannot find the file specified. (0x80070002)".into(),
                ));
            }
            Ok(self.current.lock().unwrap().clone())
        }

        fn desktop_of_window(&self, window: WindowHandle) -> Result<DesktopId, DesktopError> {
            self.guard()?;

            // A freshly shown window is not registered straight away. Count
            // down so tests can exercise the wait rather than assuming it.
            {
                let mut pending = self.unregistered_for.lock().unwrap();
                if let Some(remaining) = pending.get_mut(&window) {
                    if *remaining > 0 {
                        *remaining -= 1;
                        return Err(DesktopError::Failed("Element not found. (0x8002802B)".into()));
                    }
                }
            }

            self.windows.lock().unwrap().get(&window).cloned().ok_or_else(|| {
                // What Windows reports for a window it has not registered.
                DesktopError::Failed("Element not found. (0x8002802B)".into())
            })
        }

        fn is_window_on_current(&self, window: WindowHandle) -> Result<bool, DesktopError> {
            self.guard()?;
            let current = self.current.lock().unwrap().clone();
            Ok(self.windows.lock().unwrap().get(&window).map(|d| *d == current).unwrap_or(false))
        }

        fn move_window_to(&self, window: WindowHandle, desktop: &str) -> Result<(), DesktopError> {
            self.guard()?;
            self.windows.lock().unwrap().insert(window, desktop.to_string());
            self.moves.lock().unwrap().push((window, desktop.to_string()));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeVirtualDesktops;
    use super::*;
    use crate::platform::placement::ALL_DESKTOPS;

    const DESKTOP_A: &str = "{3F9D399E-C0CF-41D2-9743-5A229563DEDA}";
    const DESKTOP_B: &str = "{0EDBDC61-DD54-40A1-B6D9-E36E5BA42B7A}";
    const MANAGER: WindowHandle = WindowHandle(0x6C1D10);

    fn assigned(ids: &[&str]) -> HashSet<DesktopId> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    // The behaviour that previously could only be checked by switching real
    // virtual desktops on the developer's machine.
    #[test]
    fn the_manager_window_follows_the_user_onto_a_new_desktop() {
        let desktops = FakeVirtualDesktops::new(DESKTOP_A).with_window_on(MANAGER, DESKTOP_A);
        let pinned = assigned(&[ALL_DESKTOPS]);

        // Given the manager is on the desktop we are looking at, nothing happens.
        let action = reconcile(&desktops, MANAGER, &pinned, DESKTOP_A).unwrap();
        assert_eq!(action, Placement::Leave);
        assert_eq!(desktops.move_count(), 0);

        // When the user switches desktop, the manager is left behind...
        desktops.switch_to(DESKTOP_B);
        assert!(!desktops.is_window_on_current(MANAGER).unwrap());

        // ...and the next tick brings it along.
        let action = reconcile(&desktops, MANAGER, &pinned, DESKTOP_B).unwrap();
        assert_eq!(action, Placement::MoveToCurrentDesktop);
        assert_eq!(desktops.desktop_of(MANAGER), Some(DESKTOP_B.to_string()));
    }

    #[test]
    fn a_window_already_in_place_is_never_moved() {
        let desktops = FakeVirtualDesktops::new(DESKTOP_A).with_window_on(MANAGER, DESKTOP_A);

        reconcile(&desktops, MANAGER, &assigned(&[DESKTOP_A]), DESKTOP_A).unwrap();

        assert_eq!(desktops.move_count(), 0);
    }

    #[test]
    fn a_window_belonging_to_another_desktop_is_left_alone() {
        let desktops = FakeVirtualDesktops::new(DESKTOP_A).with_window_on(MANAGER, DESKTOP_B);

        let action = reconcile(&desktops, MANAGER, &assigned(&[DESKTOP_B]), DESKTOP_A).unwrap();

        assert_eq!(action, Placement::Leave);
        assert_eq!(desktops.move_count(), 0);
        assert_eq!(desktops.desktop_of(MANAGER), Some(DESKTOP_B.to_string()));
    }

    #[test]
    fn a_window_assigned_to_several_desktops_is_pulled_onto_each_of_them() {
        let desktops = FakeVirtualDesktops::new(DESKTOP_B).with_window_on(MANAGER, DESKTOP_A);
        let both = assigned(&[DESKTOP_A, DESKTOP_B]);

        let action = reconcile(&desktops, MANAGER, &both, DESKTOP_B).unwrap();

        assert_eq!(action, Placement::MoveToCurrentDesktop);
        assert_eq!(desktops.desktop_of(MANAGER), Some(DESKTOP_B.to_string()));
    }

    #[test]
    fn repeated_ticks_do_not_keep_moving_a_settled_window() {
        let desktops = FakeVirtualDesktops::new(DESKTOP_B).with_window_on(MANAGER, DESKTOP_A);
        let pinned = assigned(&[ALL_DESKTOPS]);

        for _ in 0..5 {
            reconcile(&desktops, MANAGER, &pinned, DESKTOP_B).unwrap();
        }

        // Only the first tick had anything to do.
        assert_eq!(desktops.move_count(), 1);
    }

    mod waiting_for_registration {
        use super::*;
        use std::cell::Cell;
        use std::time::{Duration, Instant};

        /// A clock that only advances when the code under test sleeps, so the
        /// timeout is exercised without the test taking any real time.
        fn fake_clock() -> (impl Fn() -> Instant, impl Fn(Duration)) {
            let start = Instant::now();
            let elapsed = std::rc::Rc::new(Cell::new(Duration::ZERO));
            let read = elapsed.clone();
            (
                move || start + read.get(),
                move |d: Duration| elapsed.set(elapsed.get() + d),
            )
        }

        #[test]
        fn returns_straight_away_for_a_window_already_registered() {
            let desktops = FakeVirtualDesktops::new(DESKTOP_A).with_window_on(MANAGER, DESKTOP_A);
            let (now, sleep) = fake_clock();

            let id = wait_until_registered(&desktops, MANAGER, None, now, sleep).unwrap();

            assert_eq!(id, DESKTOP_A);
        }

        // The actual bug: a freshly shown window is not registered yet, and the
        // old code slept a fixed guess and hoped.
        #[test]
        fn waits_for_a_window_that_has_only_just_been_shown() {
            let desktops = FakeVirtualDesktops::new(DESKTOP_A)
                .with_window_on(MANAGER, DESKTOP_A)
                .unregistered_for(MANAGER, 5);
            let (now, sleep) = fake_clock();

            let id = wait_until_registered(&desktops, MANAGER, None, now, sleep).unwrap();

            assert_eq!(id, DESKTOP_A);
        }

        #[test]
        fn waits_until_the_window_reports_the_desktop_it_was_moved_to() {
            let desktops = FakeVirtualDesktops::new(DESKTOP_B).with_window_on(MANAGER, DESKTOP_A);
            let (now, sleep) = fake_clock();

            // Still on A, so waiting for B must time out rather than settle.
            let err = wait_until_registered(&desktops, MANAGER, Some(DESKTOP_B), now, sleep)
                .unwrap_err();

            assert!(err.to_string().contains("waiting for"));
        }

        #[test]
        fn settles_once_the_window_reports_the_expected_desktop() {
            let desktops = FakeVirtualDesktops::new(DESKTOP_B).with_window_on(MANAGER, DESKTOP_B);
            let (now, sleep) = fake_clock();

            let id =
                wait_until_registered(&desktops, MANAGER, Some(DESKTOP_B), now, sleep).unwrap();

            assert_eq!(id, DESKTOP_B);
        }

        #[test]
        fn gives_up_on_a_window_that_never_registers() {
            let desktops = FakeVirtualDesktops::new(DESKTOP_A);
            let (now, sleep) = fake_clock();

            let err = wait_until_registered(&desktops, MANAGER, None, now, sleep).unwrap_err();

            assert!(err.to_string().contains("8002802B"));
        }

        // A null desktop is what taskbar hiding leaves behind. It is a real
        // answer from the OS, but it is not a desktop, so never settle on it.
        #[test]
        fn refuses_to_settle_on_a_null_desktop() {
            let desktops = FakeVirtualDesktops::new(DESKTOP_A).with_null_desktop(MANAGER);
            let (now, sleep) = fake_clock();

            let err = wait_until_registered(&desktops, MANAGER, None, now, sleep).unwrap_err();

            assert!(err.to_string().contains("null desktop"));
        }
    }

    mod current_desktop_with_fallback {
        use super::*;
        use crate::platform::availability::DesktopSource;

        #[test]
        fn uses_the_registry_when_it_answers() {
            let desktops = FakeVirtualDesktops::new(DESKTOP_A).with_window_on(MANAGER, DESKTOP_A);

            let (id, source) = current_desktop_with_fallback(&desktops, Some(MANAGER)).unwrap();

            assert_eq!(id, DESKTOP_A);
            assert_eq!(source, DesktopSource::Registry);
        }

        // The environment measured on a GitHub-hosted runner: the VD registry
        // keys do not exist, but IVirtualDesktopManager answers normally.
        #[test]
        fn falls_back_to_com_where_the_registry_keys_do_not_exist() {
            let desktops = FakeVirtualDesktops::new(DESKTOP_A)
                .with_window_on(MANAGER, DESKTOP_A)
                .without_registry();

            let (id, source) = current_desktop_with_fallback(&desktops, Some(MANAGER)).unwrap();

            assert_eq!(id, DESKTOP_A);
            assert_eq!(source, DesktopSource::Com);
        }

        #[test]
        fn reports_unavailable_without_a_reference_window_to_fall_back_on() {
            let desktops = FakeVirtualDesktops::new(DESKTOP_A).without_registry();

            let err = current_desktop_with_fallback(&desktops, None).unwrap_err();

            assert!(matches!(err, DesktopError::Unavailable(_)));
        }

        #[test]
        fn reports_unavailable_when_the_reference_window_is_not_registered() {
            // Registry gone and the window has never been shown, so COM cannot
            // answer for it either.
            let desktops = FakeVirtualDesktops::new(DESKTOP_A).without_registry();

            let err = current_desktop_with_fallback(&desktops, Some(MANAGER)).unwrap_err();

            assert!(matches!(err, DesktopError::Unavailable(_)));
            assert!(err.to_string().contains("0x8002802B"));
        }
    }

    // On a CI runner or service session there is no shell hosting the
    // virtual-desktop system. Reconciling must report that plainly rather than
    // silently behaving as though every window were misplaced.
    #[test]
    fn reports_when_the_virtual_desktop_system_is_unavailable() {
        let desktops = FakeVirtualDesktops::new(DESKTOP_A).unavailable("no interactive session");

        let err = reconcile(&desktops, MANAGER, &assigned(&[ALL_DESKTOPS]), DESKTOP_A).unwrap_err();

        assert!(matches!(err, DesktopError::Unavailable(_)));
        assert_eq!(desktops.move_count(), 0);
    }
}
