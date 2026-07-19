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

    /// Move a window onto a specific desktop.
    fn move_window_to(&self, window: WindowHandle, desktop: &str) -> Result<(), DesktopError>;
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
    }

    impl FakeVirtualDesktops {
        pub fn new(current: &str) -> Self {
            Self {
                current: Mutex::new(current.to_string()),
                windows: Mutex::new(HashMap::new()),
                moves: Mutex::new(Vec::new()),
                unavailable: Mutex::new(None),
            }
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
            Ok(self.current.lock().unwrap().clone())
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
