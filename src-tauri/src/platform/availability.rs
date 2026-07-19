//! Working out which desktop is current, and noticing when we cannot.
//!
//! The registry is the cheap way to ask (no window handle needed), but its keys
//! do not exist in every environment - measured absent on a GitHub-hosted
//! runner where the COM interface answered perfectly well. So the registry is
//! an optimisation, not the source of truth, and COM is the fallback.
//!
//! Kept pure: the callers supply the two mechanisms, this decides between them.

use super::desktop_id::DesktopId;
use super::virtual_desktops::DesktopError;

/// Which mechanism actually answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopSource {
    /// `HKCU\...\Explorer\VirtualDesktops\CurrentVirtualDesktop`.
    Registry,
    /// `IVirtualDesktopManager::GetWindowDesktopId` against a window we own.
    Com,
}

/// A resolved current desktop, and how we found it.
pub type Resolved = (DesktopId, DesktopSource);

/// Ask the registry first, then COM.
///
/// The COM closure is only invoked if the registry could not answer, since it
/// needs a shown window and is the more expensive call.
pub fn resolve_current_desktop<F>(
    from_registry: Result<DesktopId, DesktopError>,
    from_com: F,
) -> Result<Resolved, DesktopError>
where
    F: FnOnce() -> Result<DesktopId, DesktopError>,
{
    // A present-but-empty value is not an answer.
    let registry_problem = match from_registry {
        Ok(id) if !id.is_empty() => return Ok((id, DesktopSource::Registry)),
        Ok(_) => "registry: value present but empty".to_string(),
        Err(e) => format!("registry: {e}"),
    };

    match from_com() {
        Ok(id) if !id.is_empty() => Ok((id, DesktopSource::Com)),
        Ok(_) => Err(DesktopError::Unavailable(format!(
            "{registry_problem}; com: returned an empty desktop id"
        ))),
        Err(e) => Err(DesktopError::Unavailable(format!(
            "{registry_problem}; com: {e}"
        ))),
    }
}

/// What changed about our ability to see virtual desktops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// We could see desktops and now cannot.
    BecameUnavailable(String),
    /// We could not see desktops and now can.
    Recovered(DesktopSource),
}

/// Reports only when availability *changes*.
///
/// The monitor polls every 500ms. Without this, an environment with no
/// virtual-desktop support would emit a log line twice a second forever, and an
/// environment with support would emit nothing at all - so the interesting
/// event would be buried either way.
#[derive(Debug, Default)]
pub struct AvailabilityTracker {
    /// `None` until the first observation.
    available: Option<bool>,
}

impl AvailabilityTracker {
    pub fn new() -> Self {
        Self { available: None }
    }

    /// Record an outcome, returning a transition worth reporting.
    pub fn observe(&mut self, outcome: &Result<Resolved, DesktopError>) -> Option<Transition> {
        let now_available = outcome.is_ok();
        let was_available = self.available.replace(now_available);

        match (was_available, outcome) {
            // Unchanged, including the very first observation of a working
            // system - there is nothing to tell anyone.
            (Some(prev), _) if prev == now_available => None,
            (None, Ok(_)) => None,

            (_, Err(e)) => Some(Transition::BecameUnavailable(e.to_string())),
            (_, Ok((_, source))) => Some(Transition::Recovered(*source)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const DESKTOP_A: &str = "{3F9D399E-C0CF-41D2-9743-5A229563DEDA}";
    const DESKTOP_B: &str = "{0EDBDC61-DD54-40A1-B6D9-E36E5BA42B7A}";

    fn missing_key() -> DesktopError {
        DesktopError::Unavailable("The system cannot find the file specified. (0x80070002)".into())
    }

    mod resolve_current_desktop {
        use super::*;

        #[test]
        fn uses_the_registry_when_it_answers() {
            let resolved = resolve_current_desktop(Ok(DESKTOP_A.to_string()), || {
                panic!("COM must not be consulted when the registry answered")
            })
            .unwrap();

            assert_eq!(resolved, (DESKTOP_A.to_string(), DesktopSource::Registry));
        }

        #[test]
        fn does_not_pay_for_the_com_call_when_the_registry_answers() {
            let called = Cell::new(false);

            let _ = resolve_current_desktop(Ok(DESKTOP_A.to_string()), || {
                called.set(true);
                Ok(DESKTOP_B.to_string())
            });

            assert!(!called.get());
        }

        // The case measured on a GitHub-hosted runner: registry keys absent,
        // COM working normally.
        #[test]
        fn falls_back_to_com_when_the_registry_keys_are_missing() {
            let resolved =
                resolve_current_desktop(Err(missing_key()), || Ok(DESKTOP_B.to_string())).unwrap();

            assert_eq!(resolved, (DESKTOP_B.to_string(), DesktopSource::Com));
        }

        // A present-but-empty value is not an answer, and previously would have
        // been passed through as a desktop id of "".
        #[test]
        fn falls_back_to_com_when_the_registry_answers_with_nothing() {
            let resolved =
                resolve_current_desktop(Ok(String::new()), || Ok(DESKTOP_B.to_string())).unwrap();

            assert_eq!(resolved, (DESKTOP_B.to_string(), DesktopSource::Com));
        }

        #[test]
        fn reports_unavailable_when_neither_mechanism_answers() {
            let err = resolve_current_desktop(Err(missing_key()), || {
                Err(DesktopError::Failed("Element not found. (0x8002802B)".into()))
            })
            .unwrap_err();

            assert!(matches!(err, DesktopError::Unavailable(_)));
        }

        #[test]
        fn explains_both_failures_when_neither_mechanism_answers() {
            let err = resolve_current_desktop(Err(missing_key()), || {
                Err(DesktopError::Failed("Element not found. (0x8002802B)".into()))
            })
            .unwrap_err();

            let message = err.to_string();
            assert!(message.contains("0x80070002"), "should mention the registry failure: {message}");
            assert!(message.contains("0x8002802B"), "should mention the COM failure: {message}");
        }

        #[test]
        fn treats_an_empty_answer_from_both_as_unavailable() {
            let err = resolve_current_desktop(Ok(String::new()), || Ok(String::new())).unwrap_err();

            assert!(matches!(err, DesktopError::Unavailable(_)));
        }
    }

    mod availability_tracker {
        use super::*;

        fn ok() -> Result<Resolved, DesktopError> {
            Ok((DESKTOP_A.to_string(), DesktopSource::Registry))
        }

        fn via_com() -> Result<Resolved, DesktopError> {
            Ok((DESKTOP_A.to_string(), DesktopSource::Com))
        }

        fn unavailable() -> Result<Resolved, DesktopError> {
            Err(DesktopError::Unavailable("nothing answered".into()))
        }

        #[test]
        fn says_nothing_when_desktops_work_from_the_start() {
            let mut tracker = AvailabilityTracker::new();
            assert_eq!(tracker.observe(&ok()), None);
        }

        #[test]
        fn reports_the_first_time_desktops_become_unavailable() {
            let mut tracker = AvailabilityTracker::new();

            assert!(matches!(
                tracker.observe(&unavailable()),
                Some(Transition::BecameUnavailable(_))
            ));
        }

        // The point of the tracker: a 500ms poll must not log twice a second.
        #[test]
        fn stays_quiet_while_desktops_remain_unavailable() {
            let mut tracker = AvailabilityTracker::new();
            tracker.observe(&unavailable());

            for _ in 0..10 {
                assert_eq!(tracker.observe(&unavailable()), None);
            }
        }

        #[test]
        fn stays_quiet_while_desktops_keep_working() {
            let mut tracker = AvailabilityTracker::new();
            tracker.observe(&ok());

            for _ in 0..10 {
                assert_eq!(tracker.observe(&ok()), None);
            }
        }

        #[test]
        fn reports_when_desktops_come_back() {
            let mut tracker = AvailabilityTracker::new();
            tracker.observe(&unavailable());

            assert_eq!(
                tracker.observe(&via_com()),
                Some(Transition::Recovered(DesktopSource::Com))
            );
        }

        #[test]
        fn reports_each_time_availability_flips() {
            let mut tracker = AvailabilityTracker::new();

            assert!(tracker.observe(&unavailable()).is_some());
            assert!(tracker.observe(&ok()).is_some());
            assert!(tracker.observe(&unavailable()).is_some());
            assert!(tracker.observe(&ok()).is_some());
        }

        #[test]
        fn does_not_report_merely_because_the_source_changed() {
            let mut tracker = AvailabilityTracker::new();
            tracker.observe(&ok());

            // Registry stopped answering and COM took over. Still available, so
            // this is not a transition worth waking anyone up for.
            assert_eq!(tracker.observe(&via_com()), None);
        }
    }
}
