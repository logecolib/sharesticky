//! The virtual-desktop context menu: what it offers, and how its items are named.
//!
//! The item id is produced here and parsed back in `lib.rs`'s menu event
//! handler. Keeping both halves in one tested module stops them drifting
//! apart, which is the failure the manager's desktop filter had: one rule
//! written two different ways in two files, quietly disagreeing.
//!
//! Pure and platform-independent, so it is testable anywhere.

use crate::platform::placement::ALL_DESKTOPS;

/// Prefix marking a menu item as a virtual-desktop choice.
const MENU_PREFIX: &str = "vd:";

/// A desktop the user can pick from the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopChoice {
    pub id: String,
    pub name: String,
    pub is_current: bool,
}

/// One entry in the popup menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    /// Item id, round-tripped through `parse_menu_id`.
    pub id: String,
    pub label: String,
    pub checked: bool,
}

/// Name a menu item for a (sticky, desktop) pair.
pub fn menu_id(sticky_id: &str, desktop_id: &str) -> String {
    format!("{MENU_PREFIX}{sticky_id}:{desktop_id}")
}

/// Recover the (sticky, desktop) pair from a menu item id.
///
/// Returns `None` for any id that is not one of ours.
pub fn parse_menu_id(id: &str) -> Option<(&str, &str)> {
    // Splits on the first colon, which is safe because sticky ids are UUIDs.
    // The test suite pins that assumption.
    id.strip_prefix(MENU_PREFIX)?.split_once(':')
}

/// Build the menu for a sticky, given the desktops available.
///
/// `assigned` is the sticky's stored `desktop_id` field: empty, `"*"`, or a
/// comma-separated list.
pub fn menu_entries(sticky_id: &str, desktops: &[DesktopChoice], assigned: &str) -> Vec<MenuEntry> {
    let assigned: std::collections::HashSet<&str> =
        assigned.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();

    // "All desktops" is a choice in its own right, not shorthand for ticking
    // every desktop, so it suppresses the individual ticks rather than adding
    // to them.
    let pinned_everywhere = assigned.contains(ALL_DESKTOPS);

    let mut entries: Vec<MenuEntry> = desktops
        .iter()
        .map(|desktop| MenuEntry {
            id: menu_id(sticky_id, &desktop.id),
            label: if desktop.is_current {
                format!("{} (current)", desktop.name)
            } else {
                desktop.name.clone()
            },
            checked: !pinned_everywhere && assigned.contains(desktop.id.as_str()),
        })
        .collect();

    entries.push(MenuEntry {
        id: menu_id(sticky_id, ALL_DESKTOPS),
        label: "All desktops".to_string(),
        checked: pinned_everywhere,
    });

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    const STICKY: &str = "89d6241c-e537-4167-ac34-ed5893338e43";
    const DESK_A: &str = "{3F9D399E-C0CF-41D2-9743-5A229563DEDA}";
    const DESK_B: &str = "{0EDBDC61-DD54-40A1-B6D9-E36E5BA42B7A}";

    fn desktops() -> Vec<DesktopChoice> {
        vec![
            DesktopChoice { id: DESK_A.into(), name: "Work".into(), is_current: true },
            DesktopChoice { id: DESK_B.into(), name: "Personal".into(), is_current: false },
        ]
    }

    mod ids {
        use super::*;

        #[test]
        fn round_trips_a_sticky_and_desktop_pair() {
            let id = menu_id(STICKY, DESK_A);

            assert_eq!(parse_menu_id(&id), Some((STICKY, DESK_A)));
        }

        #[test]
        fn round_trips_the_all_desktops_marker() {
            let id = menu_id(STICKY, ALL_DESKTOPS);

            assert_eq!(parse_menu_id(&id), Some((STICKY, ALL_DESKTOPS)));
        }

        // The decoder splits on the first colon, so it relies on sticky ids
        // never containing one. UUIDs do not - this pins that assumption so a
        // change of id format fails here rather than silently misrouting menus.
        #[test]
        fn a_sticky_id_containing_no_colon_is_recovered_whole() {
            assert!(!STICKY.contains(':'), "sticky ids must not contain a colon");
            let id = menu_id(STICKY, DESK_A);
            let (sticky, _) = parse_menu_id(&id).unwrap();
            assert_eq!(sticky, STICKY);
        }

        #[test]
        fn ignores_a_menu_item_that_is_not_ours() {
            assert_eq!(parse_menu_id("quit"), None);
            assert_eq!(parse_menu_id("show_manager"), None);
        }

        #[test]
        fn ignores_a_malformed_item_with_no_desktop_part() {
            assert_eq!(parse_menu_id("vd:only-a-sticky-id"), None);
        }
    }

    mod entries {
        use super::*;

        #[test]
        fn offers_every_desktop_plus_an_all_desktops_choice() {
            let entries = menu_entries(STICKY, &desktops(), "");

            assert_eq!(entries.len(), 3);
            assert_eq!(entries[2].label, "All desktops");
        }

        #[test]
        fn marks_the_desktop_the_user_is_on() {
            let entries = menu_entries(STICKY, &desktops(), "");

            assert_eq!(entries[0].label, "Work (current)");
            assert_eq!(entries[1].label, "Personal");
        }

        #[test]
        fn names_each_entry_so_it_can_be_routed_back() {
            let entries = menu_entries(STICKY, &desktops(), "");

            assert_eq!(parse_menu_id(&entries[0].id), Some((STICKY, DESK_A)));
            assert_eq!(parse_menu_id(&entries[1].id), Some((STICKY, DESK_B)));
            assert_eq!(parse_menu_id(&entries[2].id), Some((STICKY, ALL_DESKTOPS)));
        }

        #[test]
        fn checks_nothing_for_an_unassigned_sticky() {
            let entries = menu_entries(STICKY, &desktops(), "");

            assert!(entries.iter().all(|e| !e.checked));
        }

        #[test]
        fn checks_the_desktop_a_sticky_is_assigned_to() {
            let entries = menu_entries(STICKY, &desktops(), DESK_B);

            assert!(!entries[0].checked);
            assert!(entries[1].checked);
        }

        #[test]
        fn checks_every_desktop_a_sticky_is_assigned_to() {
            let entries = menu_entries(STICKY, &desktops(), &format!("{DESK_A},{DESK_B}"));

            assert!(entries[0].checked);
            assert!(entries[1].checked);
        }

        // "All desktops" is a single choice, not shorthand for ticking each one.
        #[test]
        fn checks_only_the_all_desktops_entry_when_pinned_everywhere() {
            let entries = menu_entries(STICKY, &desktops(), ALL_DESKTOPS);

            assert!(entries[2].checked, "the all-desktops entry should be ticked");
            assert!(!entries[0].checked);
            assert!(!entries[1].checked);
        }

        #[test]
        fn still_offers_the_all_desktops_choice_when_no_desktops_are_known() {
            let entries = menu_entries(STICKY, &[], "");

            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].label, "All desktops");
        }

        #[test]
        fn ignores_whitespace_in_a_stored_assignment() {
            let entries = menu_entries(STICKY, &desktops(), &format!(" {DESK_A} , {DESK_B} "));

            assert!(entries[0].checked);
            assert!(entries[1].checked);
        }
    }
}
