//! The call rules as a state machine. Fed once a second with the call apps holding the mic and whether Talaan
//! is recording; answers with what to do (nudge, start, prompt, stop). Pure: time comes in as milliseconds.

use std::collections::{HashMap, HashSet};

/// An app must hold the mic this long before Talaan reacts (a mic test or a notification sound doesn't).
pub const SETTLE_MS: u64 = 2_000;
/// The recorded app must let go of the mic this long before "Call ended?" (short drops on a flaky call don't).
pub const RELEASE_MS: u64 = 3_000;
/// "Call ended" stops the recording after this unless Keep recording.
pub const COUNTDOWN_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    Ask,
    Always,
    Never,
}

impl Rule {
    pub fn parse(s: &str) -> Rule {
        match s {
            "always" => Rule::Always,
            "never" => Rule::Never,
            _ => Rule::Ask,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Nudge { app: String },
    CloseNudge,
    /// Start recording this app's call and show the "Recording [app]" banner.
    AutoStart { app: String },
    CallEnded { app: String },
    ClosePrompt,
    AutoStop,
}

/// What the settings say, as the state machine needs it.
pub struct Settings<'a> {
    pub enabled: bool,
    pub rule: &'a dyn Fn(&str) -> Rule,
}

#[derive(Default)]
pub struct Detector {
    /// App → when it took the mic.
    holding: HashMap<String, u64>,
    nudged: Option<String>,
    /// Nudges the user dismissed, until that app lets go of the mic.
    dismissed: HashSet<String>,
    /// The call being recorded (started from detection or a nudge).
    recording_app: Option<String>,
    released_since: Option<u64>,
    /// "Call ended" countdown deadline.
    prompt_until: Option<u64>,
    /// Keep recording was chosen: no more prompts until the app takes the mic back and lets go again.
    kept: bool,
    /// Last tick's recording state, to notice the recording ending.
    was_recording: bool,
}

impl Detector {
    /// The user accepted a nudge (recording is being started for `app`).
    pub fn accepted(&mut self, app: &str) {
        self.nudged = None;
        self.recording_app = Some(app.to_string());
        self.released_since = None;
        self.kept = false;
    }

    pub fn dismissed(&mut self, app: &str) {
        self.dismissed.insert(app.to_string());
        if self.nudged.as_deref() == Some(app) {
            self.nudged = None;
        }
    }

    pub fn keep_recording(&mut self) {
        self.kept = true;
        self.prompt_until = None;
    }

    pub fn tick(&mut self, now: u64, mic_apps: &[&str], recording: bool, s: &Settings) -> Vec<Action> {
        let mut out = vec![];

        // Who holds the mic, and since when.
        self.holding.retain(|app, _| mic_apps.contains(&app.as_str()));
        for app in mic_apps {
            self.holding.entry(app.to_string()).or_insert(now);
        }
        self.dismissed.retain(|app| self.holding.contains_key(app));

        // The recording ended (Stop, Discard, auto-stop): forget the call.
        if self.was_recording && !recording {
            if self.prompt_until.take().is_some() {
                out.push(Action::ClosePrompt);
            }
            self.recording_app = None;
            self.released_since = None;
            self.kept = false;
        }
        self.was_recording = recording;

        if !s.enabled {
            if self.nudged.take().is_some() {
                out.push(Action::CloseNudge);
            }
            return out;
        }

        if recording {
            if self.nudged.take().is_some() {
                out.push(Action::CloseNudge);
            }
            let Some(app) = self.recording_app.clone() else { return out }; // manual recording: never touched
            if self.holding.contains_key(&app) {
                // On the call (or back on it).
                if self.released_since.take().is_some() {
                    self.kept = false;
                }
                if self.prompt_until.take().is_some() {
                    out.push(Action::ClosePrompt);
                }
            } else {
                let since = *self.released_since.get_or_insert(now);
                match self.prompt_until {
                    Some(deadline) if now >= deadline => {
                        self.prompt_until = None;
                        out.push(Action::AutoStop);
                    }
                    None if !self.kept && now - since >= RELEASE_MS => {
                        self.prompt_until = Some(now + COUNTDOWN_MS);
                        out.push(Action::CallEnded { app });
                    }
                    _ => {}
                }
            }
            return out;
        }

        // Not recording: a nudge whose app left goes away.
        if let Some(n) = &self.nudged {
            if !self.holding.contains_key(n) {
                self.nudged = None;
                out.push(Action::CloseNudge);
            }
        }
        // The first settled app that isn't dismissed decides. Sorted so the choice is stable.
        let mut settled: Vec<String> = self.holding.iter().filter(|(_, t)| now - **t >= SETTLE_MS).map(|(a, _)| a.clone()).filter(|a| !self.dismissed.contains(a)).collect();
        settled.sort();
        for app in settled {
            let rule = match ((s.rule)(&app), app.as_str()) {
                // Browsers can't be told apart from a YouTube tab until tab detection (M9): Always means Ask.
                (Rule::Always, "browser") => Rule::Ask,
                (r, _) => r,
            };
            match rule {
                Rule::Never => continue,
                Rule::Always => {
                    if self.nudged.take().is_some() {
                        out.push(Action::CloseNudge);
                    }
                    self.accepted(&app);
                    out.push(Action::AutoStart { app: app.clone() });
                    return out;
                }
                Rule::Ask => {
                    if self.nudged.as_deref() != Some(app.as_str()) {
                        self.nudged = Some(app.clone());
                        out.push(Action::Nudge { app: app.clone() });
                    }
                    return out;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(app: &str) -> Rule {
        match app {
            "zoom" => Rule::Always,
            "facetime" => Rule::Never,
            "browser" => Rule::Always,
            _ => Rule::Ask,
        }
    }

    struct Sim {
        d: Detector,
        now: u64,
        recording: bool,
        enabled: bool,
    }

    impl Sim {
        fn new() -> Self {
            Sim { d: Detector::default(), now: 0, recording: false, enabled: true }
        }
        /// Runs ticks every second for `secs` with these apps on the mic; returns every action.
        fn run(&mut self, secs: u64, apps: &[&str]) -> Vec<Action> {
            let mut all = vec![];
            for _ in 0..secs {
                self.now += 1_000;
                let s = Settings { enabled: self.enabled, rule: &rules };
                let acts = self.d.tick(self.now, apps, self.recording, &s);
                for a in &acts {
                    match a {
                        Action::AutoStart { .. } => self.recording = true,
                        Action::AutoStop => self.recording = false,
                        _ => {}
                    }
                }
                all.extend(acts);
            }
            all
        }
    }

    fn nudge(app: &str) -> Action {
        Action::Nudge { app: app.into() }
    }

    #[test]
    fn a_short_blip_does_nothing() {
        let mut s = Sim::new();
        assert!(s.run(1, &["slack"]).is_empty());
        assert!(s.run(5, &[]).is_empty());
    }

    #[test]
    fn ask_nudges_once_and_the_nudge_leaves_with_the_app() {
        let mut s = Sim::new();
        assert_eq!(s.run(5, &["slack"]), vec![nudge("slack")]);
        assert_eq!(s.run(1, &[]), vec![Action::CloseNudge]);
    }

    #[test]
    fn always_starts_recording_and_ending_the_call_stops_after_the_prompt() {
        let mut s = Sim::new();
        assert_eq!(s.run(3, &["zoom"]), vec![Action::AutoStart { app: "zoom".into() }]);
        assert!(s.recording);
        assert!(s.run(600, &["zoom"]).is_empty()); // a 10-minute call
        let ended = s.run(4, &[]);
        assert_eq!(ended, vec![Action::CallEnded { app: "zoom".into() }]);
        assert_eq!(s.run(31, &[]), vec![Action::AutoStop]);
        assert!(!s.recording);
        // The call is over: no prompt again, and Zoom closing doesn't start anything.
        assert!(s.run(10, &[]).is_empty());
    }

    #[test]
    fn rejoining_during_the_countdown_closes_the_prompt() {
        let mut s = Sim::new();
        s.run(3, &["zoom"]);
        s.run(5, &[]); // dropped, prompt shown
        assert_eq!(s.run(1, &["zoom"]), vec![Action::ClosePrompt]);
        assert!(s.recording);
    }

    #[test]
    fn keep_recording_means_no_more_prompts_until_the_app_comes_back_and_leaves_again() {
        let mut s = Sim::new();
        s.run(3, &["zoom"]);
        s.run(5, &[]);
        s.d.keep_recording();
        assert!(s.run(60, &[]).is_empty());
        assert!(s.recording);
        s.run(2, &["zoom"]);
        assert_eq!(s.run(4, &[]), vec![Action::CallEnded { app: "zoom".into() }]);
    }

    #[test]
    fn never_and_dismissed_stay_quiet() {
        let mut s = Sim::new();
        assert!(s.run(10, &["facetime"]).is_empty());
        assert_eq!(s.run(3, &["slack"]), vec![nudge("slack")]);
        s.d.dismissed("slack");
        assert!(s.run(30, &["slack"]).is_empty());
        // Next call in Slack asks again.
        s.run(1, &[]);
        assert_eq!(s.run(3, &["slack"]), vec![nudge("slack")]);
    }

    #[test]
    fn browsers_on_always_are_asked_until_tab_detection() {
        let mut s = Sim::new();
        assert_eq!(s.run(3, &["browser"]), vec![nudge("browser")]);
        assert!(!s.recording);
    }

    #[test]
    fn accepting_a_nudge_follows_that_call() {
        let mut s = Sim::new();
        s.run(3, &["slack"]);
        s.d.accepted("slack");
        s.recording = true;
        assert!(s.run(20, &["slack"]).is_empty());
        assert_eq!(s.run(4, &[]), vec![Action::CallEnded { app: "slack".into() }]);
    }

    #[test]
    fn a_manual_recording_is_never_prompted_or_stopped() {
        let mut s = Sim::new();
        s.recording = true;
        assert!(s.run(5, &["slack"]).is_empty()); // no nudge while recording
        assert!(s.run(60, &[]).is_empty());
        assert!(s.recording);
    }

    #[test]
    fn stopping_by_hand_during_the_countdown_closes_the_prompt() {
        let mut s = Sim::new();
        s.run(3, &["zoom"]);
        s.run(5, &[]);
        s.recording = false;
        assert_eq!(s.run(1, &[]), vec![Action::ClosePrompt]);
    }

    #[test]
    fn detection_off_closes_a_nudge_and_does_nothing_else() {
        let mut s = Sim::new();
        s.run(3, &["slack"]);
        s.enabled = false;
        assert_eq!(s.run(1, &["slack"]), vec![Action::CloseNudge]);
        assert!(s.run(10, &["zoom"]).is_empty());
    }

    /// What the calls loop does with mic users (the same shared step).
    fn apps_for(users: &[&str]) -> Vec<&'static str> {
        crate::calls::apps::call_app_ids(users.iter().map(|u| (*u, "")), &[])
    }

    #[test]
    fn windows_ids_drive_the_rules_end_to_end() {
        // zoom is Always, everything else Ask (see `rules`).
        let mut s = Sim::new();
        assert_eq!(s.run(3, &apps_for(&["zoom.exe"])), vec![Action::AutoStart { app: "zoom".into() }]);
        assert_eq!(s.run(4, &apps_for(&[])), vec![Action::CallEnded { app: "zoom".into() }]);
        assert_eq!(s.run(31, &apps_for(&[])), vec![Action::AutoStop]);

        let mut s = Sim::new();
        assert_eq!(s.run(5, &apps_for(&["MSTeams_8wekyb3d8bbwe"])), vec![nudge("teams")]);
        assert_eq!(s.run(1, &apps_for(&[])), vec![Action::CloseNudge]);

        // Parlato's dictation and a mic filter never prompt, even alongside each other.
        let mut s = Sim::new();
        assert!(s.run(60, &apps_for(&["parla.exe", "nvidia broadcast.exe"])).is_empty());
    }
}
