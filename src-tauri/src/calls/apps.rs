//! Which app a mic-using process belongs to. Calls are usually captured by helper processes
//! (`us.zoom.caphost`, `com.microsoft.teams2.modulehost`, `com.google.Chrome.helper`), so apps are matched by
//! bundle-id prefix. Ignored apps (dictation, voice memos, other recorders) are checked first, so they can never
//! look like a call. Apps in neither table never trigger anything.

/// Call app ids match the `callApps` ids in settings.
pub const CALL_APPS: &[(&str, &str, &[&str])] = &[
    ("zoom", "Zoom", &["us.zoom."]),
    ("teams", "Microsoft Teams", &["com.microsoft.teams"]),
    ("slack", "Slack", &["com.tinyspeck.slackmacgap"]),
    ("facetime", "FaceTime", &["com.apple.avconferenced", "com.apple.FaceTime"]),
    ("webex", "Webex", &["com.cisco.webex", "com.webex."]),
    ("discord", "Discord", &["com.hnc.Discord"]),
    ("whatsapp", "WhatsApp", &["net.whatsapp.WhatsApp", "desktop.WhatsApp"]),
    (
        "browser",
        "Browser",
        &[
            "com.google.Chrome",
            "com.apple.WebKit.GPU",
            "com.apple.Safari",
            "company.thebrowser.Browser",
            "com.microsoft.edgemac",
            "com.brave.Browser",
            "org.mozilla.firefox",
            "ai.perplexity.comet",
            "com.operasoftware.Opera",
            "com.vivaldi.Vivaldi",
        ],
    ),
];

/// Ignored apps: never trigger Talaan, whatever they do with the mic. Names match `ignoredApps` in settings.
pub const IGNORED: &[(&str, &[&str])] = &[
    ("Talaan", &["com.craftconceptsdigital.talaan"]),
    ("Parlato", &["com.craftconceptsdigital.parlato"]),
    ("VoiceInk", &["com.prakashjoshipax.VoiceInk"]),
    ("Superwhisper", &["com.superduper.superwhisper"]),
    ("Wispr Flow", &["com.electron.wispr-flow", "ai.wispr."]),
    ("MacWhisper", &["com.goodsnooze.MacWhisper", "com.goodsnooze.macwhisper"]),
    ("Apple Dictation", &["com.apple.CoreSpeech", "com.apple.DictationIM", "com.apple.SpeechRecognitionCore", "com.apple.assistantd", "com.apple.SiriNCService"]),
    ("Voice Memos", &["com.apple.VoiceMemos"]),
    ("Fathom", &["video.fathom."]),
];

fn matches(bundle_id: &str, prefixes: &[&str]) -> bool {
    !bundle_id.is_empty() && prefixes.iter().any(|p| bundle_id.starts_with(p))
}

/// The ignored-app name, when this process belongs to one. `extra` holds user-added names matched against the
/// bundle id and path (case-insensitive).
pub fn ignored(bundle_id: &str, path: &str, extra: &[String]) -> Option<String> {
    if let Some((name, _)) = IGNORED.iter().find(|(_, prefixes)| matches(bundle_id, prefixes)) {
        return Some(name.to_string());
    }
    let hay = format!("{bundle_id} {path}").to_lowercase().replace(' ', "");
    extra.iter().find(|n| !n.trim().is_empty() && hay.contains(&n.to_lowercase().replace(' ', ""))).cloned()
}

/// The call app (id, label) for a process, unless it's ignored or unknown.
pub fn identify(bundle_id: &str, path: &str, extra_ignored: &[String]) -> Option<(&'static str, &'static str)> {
    if ignored(bundle_id, path, extra_ignored).is_some() {
        return None;
    }
    CALL_APPS.iter().find(|(_, _, prefixes)| matches(bundle_id, prefixes)).map(|(id, label, _)| (*id, *label))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_apps_are_found_through_their_helpers() {
        assert_eq!(identify("us.zoom.caphost", "", &[]), Some(("zoom", "Zoom")));
        assert_eq!(identify("us.zoom.xos", "", &[]), Some(("zoom", "Zoom")));
        assert_eq!(identify("com.microsoft.teams2.modulehost", "", &[]), Some(("teams", "Microsoft Teams")));
        assert_eq!(identify("com.tinyspeck.slackmacgap.helper", "", &[]), Some(("slack", "Slack")));
        assert_eq!(identify("com.apple.avconferenced", "", &[]), Some(("facetime", "FaceTime")));
        assert_eq!(identify("com.hnc.Discord.helper", "", &[]), Some(("discord", "Discord")));
    }

    #[test]
    fn browsers_are_browser_calls() {
        for b in ["com.google.Chrome.helper", "com.apple.WebKit.GPU", "company.thebrowser.Browser.helper", "com.brave.Browser.helper"] {
            assert_eq!(identify(b, "", &[]).map(|a| a.0), Some("browser"), "{b}");
        }
    }

    #[test]
    fn dictation_and_recorders_never_count() {
        for b in ["com.craftconceptsdigital.parlato", "com.prakashjoshipax.VoiceInk", "com.apple.CoreSpeech", "video.fathom.electron.helper", "com.apple.VoiceMemos"] {
            assert_eq!(identify(b, "", &[]), None, "{b}");
            assert!(ignored(b, "", &[]).is_some(), "{b}");
        }
    }

    #[test]
    fn unknown_apps_and_tools_never_trigger() {
        assert_eq!(identify("com.example.recorder", "", &[]), None);
        assert_eq!(identify("", "/usr/local/bin/sox", &[]), None);
    }

    #[test]
    fn user_added_ignored_apps_win_over_call_apps() {
        // Someone who dictates in Slack through a tool Talaan doesn't know can still silence it by name.
        assert_eq!(identify("com.tinyspeck.slackmacgap", "", &["Slack".into()]), None);
        assert_eq!(ignored("com.acme.MyDictate", "/Applications/My Dictate.app/Contents/MacOS/x", &["my dictate".into()]).as_deref(), Some("my dictate"));
    }
}
