//! What a recording knows about itself: its clock, pauses, marks and notes. Pure; time is passed in.

use super::clock::Clock;
use crate::storage::model::{Mark, MeetingFile, PauseGap};

pub struct Session {
    pub meeting_id: String,
    pub clock: Clock,
    pub marks: Vec<Mark>,
    pub pauses: Vec<PauseGap>,
}

impl Session {
    pub fn new(meeting_id: String, start_ns: u64) -> Self {
        Session { meeting_id, clock: Clock::new(start_ns), marks: vec![], pauses: vec![] }
    }

    pub fn is_paused(&self) -> bool {
        self.clock.is_paused()
    }

    pub fn pause(&mut self, now_ns: u64) -> bool {
        self.clock.pause(now_ns).is_some()
    }

    pub fn resume(&mut self, now_ns: u64) -> bool {
        match self.clock.resume(now_ns) {
            Some((at, duration_ms)) => {
                self.pauses.push(PauseGap { at, duration_ms });
                true
            }
            None => false,
        }
    }

    /// A mark at the current moment, optionally with a note. Notes are marks with text.
    pub fn mark(&mut self, now_ns: u64, note: Option<String>) -> Mark {
        let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let mark = Mark { id: format!("k{}", self.marks.len()), at: self.clock.elapsed_ms(now_ns), note, later: false };
        self.marks.push(mark.clone());
        mark
    }

    pub fn notes(&self) -> usize {
        self.marks.iter().filter(|m| m.note.is_some()).count()
    }

    pub fn elapsed_ms(&self, now_ns: u64) -> u64 {
        self.clock.elapsed_ms(now_ns)
    }

    /// Copies what a crash must not lose into `meeting.json`.
    pub fn apply(&self, file: &mut MeetingFile, now_ns: u64) {
        file.duration_ms = self.elapsed_ms(now_ns);
        file.marks = self.marks.clone();
        file.pauses = self.pauses.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const S: u64 = 1_000_000_000;

    #[test]
    fn pauses_are_recorded_where_they_happened() {
        let mut s = Session::new("m".into(), 0);
        assert!(s.pause(48 * S));
        assert!(!s.pause(49 * S));
        assert!(s.resume(168 * S));
        assert_eq!(s.pauses, vec![PauseGap { at: 48_000, duration_ms: 120_000 }]);
        assert_eq!(s.elapsed_ms(170 * S), 50_000);
    }

    #[test]
    fn marks_and_notes_use_recording_time() {
        let mut s = Session::new("m".into(), 0);
        s.pause(10 * S);
        s.resume(20 * S);
        let m = s.mark(25 * S, None);
        assert_eq!(m.at, 15_000);
        let n = s.mark(30 * S, Some("  check with the data team ".into()));
        assert_eq!(n.note.as_deref(), Some("check with the data team"));
        assert_eq!(s.mark(31 * S, Some("   ".into())).note, None);
        assert_eq!(s.notes(), 1);
        assert_eq!(s.marks.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["k0", "k1", "k2"]);
    }
}
