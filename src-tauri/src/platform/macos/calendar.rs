//! The system calendar (EventKit): access, and events around a moment. Read-only; nothing is ever written.

use crate::calendar::Event;
use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventStatus, EKEventStore, EKParticipantStatus, EKParticipantType};
use objc2_foundation::{NSDate, NSError};
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Allowed,
    NotAsked,
    Denied,
}

pub fn access() -> Access {
    match unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) } {
        EKAuthorizationStatus::FullAccess => Access::Allowed,
        EKAuthorizationStatus::NotDetermined => Access::NotAsked,
        _ => Access::Denied,
    }
}

/// Shows the system's calendar prompt the first time (later calls answer from the saved choice).
/// Blocks until answered, up to two minutes.
pub fn request() -> Access {
    if access() != Access::NotAsked {
        return access();
    }
    let store = unsafe { EKEventStore::new() };
    let (tx, rx) = mpsc::channel::<bool>();
    let done = RcBlock::new(move |granted: Bool, _err: *mut NSError| {
        let _ = tx.send(granted.as_bool());
    });
    unsafe { store.requestFullAccessToEventsWithCompletion(&*done as *const _ as *mut _) };
    match rx.recv_timeout(Duration::from_secs(120)) {
        Ok(true) => Access::Allowed,
        _ => access(),
    }
}

/// Events overlapping [from_ms, to_ms] (Unix ms) in every calendar. Empty without access.
pub fn events(from_ms: i64, to_ms: i64) -> Vec<Event> {
    if access() != Access::Allowed {
        return vec![];
    }
    let secs = |ms: i64| ms as f64 / 1000.0;
    let ms = |d: &NSDate| (d.timeIntervalSince1970() * 1000.0) as i64;
    unsafe {
        let store = EKEventStore::new();
        let start = NSDate::dateWithTimeIntervalSince1970(secs(from_ms));
        let end = NSDate::dateWithTimeIntervalSince1970(secs(to_ms));
        let predicate = store.predicateForEventsWithStartDate_endDate_calendars(&start, &end, None);
        store
            .eventsMatchingPredicate(&predicate)
            .iter()
            .map(|e| {
                let everyone: Vec<_> = e.attendees().map(|list| list.iter().collect()).unwrap_or_default();
                let declined = everyone.iter().any(|p| p.isCurrentUser() && p.participantStatus() == EKParticipantStatus::Declined);
                // The other people, as the iCal path reads them: not those who declined, not rooms or equipment.
                let others: Vec<_> = everyone
                    .into_iter()
                    .filter(|p| !p.isCurrentUser())
                    .filter(|p| p.participantStatus() != EKParticipantStatus::Declined)
                    .filter(|p| ![EKParticipantType::Room, EKParticipantType::Resource].contains(&p.participantType()))
                    .collect();
                // The invite's link, location and notes, where Zoom, Meet and Teams put the join link.
                let fields: Vec<String> = [e.URL().and_then(|u| u.absoluteString()).map(|s| s.to_string()), e.location().map(|s| s.to_string()), e.notes().map(|s| s.to_string())]
                    .into_iter()
                    .flatten()
                    .collect();
                Event {
                    title: e.title().to_string(),
                    start_ms: ms(&e.startDate()),
                    end_ms: ms(&e.endDate()),
                    all_day: e.isAllDay(),
                    attendees: others.iter().filter_map(|p| p.name().map(|n| n.to_string())).filter(|n| !n.trim().is_empty()).collect(),
                    emails: others.iter().filter_map(|p| p.URL().absoluteString()).filter_map(|u| crate::calendar::email_of(&u.to_string())).collect(),
                    join_url: crate::calendar::join_link(&fields.iter().map(String::as_str).collect::<Vec<_>>()),
                    cancelled: e.status() == EKEventStatus::Canceled,
                    declined,
                }
            })
            .collect()
    }
}
