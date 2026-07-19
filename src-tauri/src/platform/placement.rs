//! Where should a window live, given the desktops it is assigned to?
//!
//! This is the decision the desktop monitor makes on every poll. It used to be
//! written inline inside the polling loop, tangled with COM calls and a Tauri
//! `AppHandle`, which meant the only way to exercise a boolean was to switch
//! real virtual desktops on a real machine.
//!
//! Kept pure and platform-independent so it is testable anywhere. It must stay
//! in agreement with `src/lib/desktop-visibility.ts`, which answers the same
//! question on the frontend.

use std::collections::HashSet;

use super::desktop_id::DesktopId;

/// Marker meaning "this window belongs on every virtual desktop".
pub const ALL_DESKTOPS: &str = "*";

/// What the monitor should do with a window this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// The window is where it should be, or we have no claim on it.
    Leave,
    /// The window belongs on the active desktop but is not there.
    MoveToCurrentDesktop,
}

/// Does a window assigned to `assigned` belong on `current_desktop`?
///
/// A window pinned to all desktops belongs everywhere. A window with no
/// assignment belongs nowhere in particular and is left alone.
pub fn belongs_on(assigned: &HashSet<DesktopId>, current_desktop: &str) -> bool {
    if assigned.contains(ALL_DESKTOPS) {
        return true;
    }
    if current_desktop.is_empty() {
        return false;
    }
    assigned.contains(current_desktop)
}

/// Decide what to do with a window this tick.
///
/// `is_on_current` is what the OS reports right now; `assigned` is what the
/// user asked for. We only ever move a window *towards* the active desktop -
/// never away from it - so a window the user dragged elsewhere is not fought
/// over unless it is claimed by the current desktop.
pub fn placement_for(
    assigned: &HashSet<DesktopId>,
    current_desktop: &str,
    is_on_current: bool,
) -> Placement {
    if is_on_current {
        return Placement::Leave;
    }
    if belongs_on(assigned, current_desktop) {
        Placement::MoveToCurrentDesktop
    } else {
        Placement::Leave
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESKTOP_A: &str = "{3F9D399E-C0CF-41D2-9743-5A229563DEDA}";
    const DESKTOP_B: &str = "{0EDBDC61-DD54-40A1-B6D9-E36E5BA42B7A}";

    fn assigned(ids: &[&str]) -> HashSet<DesktopId> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    mod belongs_on {
        use super::*;

        #[test]
        fn a_window_pinned_to_all_desktops_belongs_on_any_of_them() {
            assert!(belongs_on(&assigned(&[ALL_DESKTOPS]), DESKTOP_A));
            assert!(belongs_on(&assigned(&[ALL_DESKTOPS]), DESKTOP_B));
        }

        #[test]
        fn a_window_assigned_to_this_desktop_belongs_on_it() {
            assert!(belongs_on(&assigned(&[DESKTOP_A]), DESKTOP_A));
        }

        #[test]
        fn a_window_assigned_elsewhere_does_not_belong_here() {
            assert!(!belongs_on(&assigned(&[DESKTOP_B]), DESKTOP_A));
        }

        #[test]
        fn a_window_assigned_to_several_desktops_belongs_on_each_of_them() {
            let both = assigned(&[DESKTOP_A, DESKTOP_B]);
            assert!(belongs_on(&both, DESKTOP_A));
            assert!(belongs_on(&both, DESKTOP_B));
        }

        #[test]
        fn an_unassigned_window_belongs_nowhere_in_particular() {
            assert!(!belongs_on(&assigned(&[]), DESKTOP_A));
        }
    }

    mod placement_for {
        use super::*;

        #[test]
        fn leaves_a_window_that_is_already_on_the_active_desktop() {
            assert_eq!(
                placement_for(&assigned(&[DESKTOP_A]), DESKTOP_A, true),
                Placement::Leave
            );
        }

        #[test]
        fn moves_a_window_that_belongs_here_but_has_been_left_behind() {
            assert_eq!(
                placement_for(&assigned(&[DESKTOP_A]), DESKTOP_A, false),
                Placement::MoveToCurrentDesktop
            );
        }

        // This is the manager window's behaviour: it is simply pinned to all
        // desktops, so it follows the user everywhere. There is no separate rule.
        #[test]
        fn follows_a_window_pinned_to_all_desktops_onto_whichever_desktop_is_active() {
            assert_eq!(
                placement_for(&assigned(&[ALL_DESKTOPS]), DESKTOP_B, false),
                Placement::MoveToCurrentDesktop
            );
        }

        #[test]
        fn leaves_a_window_that_belongs_only_to_another_desktop() {
            assert_eq!(
                placement_for(&assigned(&[DESKTOP_B]), DESKTOP_A, false),
                Placement::Leave
            );
        }

        #[test]
        fn leaves_an_unassigned_window_wherever_the_user_put_it() {
            assert_eq!(
                placement_for(&assigned(&[]), DESKTOP_A, false),
                Placement::Leave
            );
        }

        #[test]
        fn never_moves_a_window_away_from_the_active_desktop() {
            // Even when the window is not claimed by this desktop, being here
            // already is never a reason to act.
            assert_eq!(
                placement_for(&assigned(&[DESKTOP_B]), DESKTOP_A, true),
                Placement::Leave
            );
        }
    }
}
