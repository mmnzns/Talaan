//! Which app a mic-using process belongs to. Calls are usually captured by helper processes
//! (`us.zoom.caphost`, `com.microsoft.teams2.modulehost`, `com.google.Chrome.helper`), so apps are matched by
//! bundle-id prefix. Ignored apps (dictation, voice memos, other recorders) are checked first, so they can never
//! look like a call. Apps in neither table never trigger anything.
//! On Windows the ids are the exe file name in lower case (`zoom.exe`) or the packaged app's family-name prefix
//! (`MSTeams_`), as `platform::windows::mic_users` reports them.

/// Call app ids match the `callApps` ids in settings.
pub const CALL_APPS: &[(&str, &str, &[&str])] = &[
    ("zoom", "Zoom", &["us.zoom.", "zoom.exe"]),
    ("teams", "Microsoft Teams", &["com.microsoft.teams", "MSTeams_", "ms-teams.exe", "teams.exe"]),
    ("slack", "Slack", &["com.tinyspeck.slackmacgap", "slack.exe", "91750D7E.Slack_"]),
    ("facetime", "FaceTime", &["com.apple.avconferenced", "com.apple.FaceTime"]),
    ("webex", "Webex", &["com.cisco.webex", "com.webex.", "ciscocollabhost.exe", "webexmta.exe", "atmgr.exe", "webex.exe"]),
    ("discord", "Discord", &["com.hnc.Discord", "discord.exe", "discordptb.exe", "discordcanary.exe"]),
    ("whatsapp", "WhatsApp", &["net.whatsapp.WhatsApp", "desktop.WhatsApp", "whatsapp.exe", "5319275A.WhatsAppDesktop_"]),
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
            "chrome.exe",
            "msedge.exe",
            "brave.exe",
            "firefox.exe",
            "opera.exe",
            "vivaldi.exe",
            "comet.exe",
            "TheBrowserCompany.Arc_",
        ],
    ),
];

/// Ignored apps: never trigger Talaan, whatever they do with the mic. Names match `ignoredApps` in settings.
pub const IGNORED: &[(&str, &[&str])] = &[
    ("Talaan", &["com.craftconceptsdigital.talaan", "talaan.exe"]),
    ("Parlato", &["com.craftconceptsdigital.parlato", "parla.exe", "parlato.exe"]),
    ("VoiceInk", &["com.prakashjoshipax.VoiceInk"]),
    ("Superwhisper", &["com.superduper.superwhisper"]),
    ("Wispr Flow", &["com.electron.wispr-flow", "ai.wispr.", "wispr flow.exe", "wisprflow.exe"]),
    ("MacWhisper", &["com.goodsnooze.MacWhisper", "com.goodsnooze.macwhisper"]),
    ("Apple Dictation", &["com.apple.CoreSpeech", "com.apple.DictationIM", "com.apple.SpeechRecognitionCore", "com.apple.assistantd", "com.apple.SiriNCService"]),
    ("Voice Memos", &["com.apple.VoiceMemos"]),
    ("Fathom", &["video.fathom."]),
    ("Windows voice typing", &["MicrosoftWindows.Client.CBS_"]),
    ("Sound Recorder", &["Microsoft.WindowsSoundRecorder_"]),
    ("NVIDIA Broadcast", &["nvidia broadcast.exe"]),
    ("SteelSeries Sonar", &["steelseriessonar.exe"]),
    ("Voicemod", &["voicemod.exe"]),
    ("Krisp", &["krisp.exe"]),
    ("Xbox Game Bar", &["Microsoft.XboxGamingOverlay_"]),
    ("FxSound", &["fxsound.exe"]),
    ("Windows audio engine", &["audiodg.exe"]),
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

/// The call apps behind a set of mic users, given as (bundle id, path): identified, sorted, each id once. This is
/// the calls loop's step from mic users to the ids the rules see.
pub fn call_app_ids<'a>(users: impl IntoIterator<Item = (&'a str, &'a str)>, extra_ignored: &[String]) -> Vec<&'static str> {
    let mut apps: Vec<&'static str> = users.into_iter().filter_map(|(bundle_id, path)| identify(bundle_id, path, extra_ignored).map(|(id, _)| id)).collect();
    apps.sort();
    apps.dedup();
    apps
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
    fn windows_call_apps_are_found() {
        assert_eq!(identify("zoom.exe", "", &[]), Some(("zoom", "Zoom")));
        assert_eq!(identify("MSTeams_8wekyb3d8bbwe", "", &[]), Some(("teams", "Microsoft Teams")));
        assert_eq!(identify("ms-teams.exe", "", &[]), Some(("teams", "Microsoft Teams")));
        assert_eq!(identify("slack.exe", "", &[]), Some(("slack", "Slack")));
        assert_eq!(identify("91750D7E.Slack_8she8kybcnzg4", "", &[]), Some(("slack", "Slack")));
        assert_eq!(identify("5319275A.WhatsAppDesktop_cv1g1gvanyjgm", "", &[]), Some(("whatsapp", "WhatsApp")));
        assert_eq!(identify("discord.exe", "", &[]), Some(("discord", "Discord")));
        assert_eq!(identify("ciscocollabhost.exe", "", &[]), Some(("webex", "Webex")));
        for b in ["chrome.exe", "msedge.exe", "brave.exe", "firefox.exe", "comet.exe", "TheBrowserCompany.Arc_ttt1ap7aakyb4"] {
            assert_eq!(identify(b, "", &[]).map(|a| a.0), Some("browser"), "{b}");
        }
    }

    #[test]
    fn windows_dictation_and_mic_processors_never_count() {
        for b in ["parla.exe", "talaan.exe", "MicrosoftWindows.Client.CBS_cw5n1h2txyewy", "Microsoft.WindowsSoundRecorder_8wekyb3d8bbwe", "nvidia broadcast.exe", "steelseriessonar.exe", "voicemod.exe", "krisp.exe", "wispr flow.exe", "Microsoft.XboxGamingOverlay_8wekyb3d8bbwe"] {
            assert_eq!(identify(b, "", &[]), None, "{b}");
            assert!(ignored(b, "", &[]).is_some(), "{b}");
        }
        // Audio plumbing that may hold a capture session: an output enhancer and the Windows audio engine.
        for b in ["fxsound.exe", "audiodg.exe"] {
            assert_eq!(identify(b, "", &[]), None, "{b}");
            assert!(ignored(b, "", &[]).is_some(), "{b}");
        }
        // Games and other apps that use the mic are in neither table.
        assert_eq!(identify("bf6.exe", r"D:\SteamLibrary\steamapps\common\Battlefield 6\bf6.exe", &[]), None);
    }

    #[test]
    fn mic_users_become_sorted_unique_call_app_ids() {
        let users = [("zoom.exe", ""), ("MSTeams_8wekyb3d8bbwe", ""), ("chrome.exe", ""), ("zoom.exe", r"C:\z\Zoom.exe"), ("parla.exe", ""), ("bf6.exe", ""), ("ms-teams.exe", "")];
        assert_eq!(call_app_ids(users, &[]), vec!["browser", "teams", "zoom"]);
        assert_eq!(call_app_ids(users, &["zoom".into()]), vec!["browser", "teams"]);
        assert!(call_app_ids([], &[]).is_empty());
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
