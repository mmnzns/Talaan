//! The few strings only Rust shows: the menu bar menu, the "Summary ready" notification, recorder warnings and the
//! Notion export's headings. The screens have their own tables (src/lib/i18n); this mirrors its rules: every string in
//! English, French (Canadian) and Spanish, proper accents, `{name}` placeholders. The frontend resolves "Auto" against
//! the system language and tells us the result through `i18n_set_language`, so there is one answer for both sides.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En = 0,
    Fr = 1,
    Es = 2,
}

impl Lang {
    pub fn parse(code: &str) -> Lang {
        match code.split(['-', '_']).next().unwrap_or("") {
            "fr" => Lang::Fr,
            "es" => Lang::Es,
            _ => Lang::En,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::Fr,
        2 => Lang::Es,
        _ => Lang::En,
    }
}

/// (key, English, French, Spanish)
const TABLE: &[(&str, &str, &str, &str)] = &[
    ("tray.open", "Open Talaan", "Ouvrir Talaan", "Abrir Talaan"),
    ("tray.settings", "Settings…", "Paramètres…", "Ajustes…"),
    ("tray.quit", "Quit Talaan", "Quitter Talaan", "Salir de Talaan"),
    ("notify.summary_ready", "Summary ready", "Résumé prêt", "Resumen listo"),
    (
        "notice.no_system_audio",
        "Talaan can't hear the call audio, so only your side is recorded. Allow system audio recording in System Settings.",
        "Talaan n’entend pas l’audio de l’appel, donc seul votre côté est enregistré. Autorisez l’enregistrement de l’audio système dans les Réglages Système.",
        "Talaan no oye el audio de la llamada, así que solo se graba tu lado. Permite la grabación del audio del sistema en Ajustes del Sistema.",
    ),
    (
        "notice.no_call_audio",
        "No call audio is coming through. If the other people are talking, allow Talaan under Screen & System Audio Recording.",
        "Aucun audio d’appel n’est reçu. Si les autres personnes parlent, autorisez Talaan sous « Enregistrement de l’écran et de l’audio système ».",
        "No llega audio de la llamada. Si las demás personas están hablando, permite el acceso a Talaan en Grabación de pantalla y audio del sistema.",
    ),
    ("notice.mic_switched", "Switched to {name}. Recording continues.", "Passage à {name}. L’enregistrement continue.", "Se cambió a {name}. La grabación continúa."),
    (
        "notice.output_switched",
        "Sound now plays through {name}. Recording continues.",
        "Le son passe maintenant par {name}. L’enregistrement continue.",
        "El sonido ahora sale por {name}. La grabación continúa.",
    ),
    (
        "notice.mic_switch_failed",
        "Couldn't switch microphones: {error}",
        "Impossible de changer de microphone : {error}",
        "No se pudo cambiar de micrófono: {error}",
    ),
    (
        "notice.output_failed",
        "Couldn't follow the new output device: {error}",
        "Impossible de passer au nouveau périphérique de sortie : {error}",
        "No se pudo seguir el nuevo dispositivo de salida: {error}",
    ),
    // Windows: system audio is process loopback, which no permission blocks, so these point at Sound settings.
    (
        "notice.no_system_audio_windows",
        "Talaan can't hear the call audio, so only your side is recorded. Check your speakers or headset in Windows Settings › System › Sound.",
        "Talaan n’entend pas l’audio de l’appel, donc seul votre côté est enregistré. Vérifiez vos haut-parleurs ou votre casque dans Paramètres Windows › Système › Son.",
        "Talaan no oye el audio de la llamada, así que solo se graba tu lado. Revisa tus altavoces o audífonos en Configuración de Windows › Sistema › Sonido.",
    ),
    (
        "notice.no_call_audio_windows",
        "No call audio is coming through. If the other people are talking, check which speakers or headset the call app uses in Windows Settings › System › Sound.",
        "Aucun audio d’appel n’est reçu. Si les autres personnes parlent, vérifiez quels haut-parleurs ou quel casque l’application d’appel utilise dans Paramètres Windows › Système › Son.",
        "No llega audio de la llamada. Si las demás personas están hablando, revisa qué altavoces o audífonos usa la aplicación de llamadas en Configuración de Windows › Sistema › Sonido.",
    ),
    (
        "notice.call_audio_lost",
        "Call audio stopped coming through, so only your side is being recorded from here.",
        "L’audio de l’appel ne passe plus, donc seul votre côté est enregistré à partir de maintenant.",
        "El audio de la llamada dejó de llegar, así que desde ahora solo se graba tu lado.",
    ),
    ("tray.tooltip_recording", "Talaan · Recording {time}", "Talaan · Enregistrement {time}", "Talaan · Grabando {time}"),
    ("tray.tooltip_paused", "Talaan · Paused {time}", "Talaan · En pause {time}", "Talaan · En pausa {time}"),
    ("notion.recorded_with", "recorded with Talaan", "enregistré avec Talaan", "grabado con Talaan"),
    ("notion.recap", "Recap", "Récapitulatif", "Resumen breve"),
    ("notion.decisions", "Decisions", "Décisions", "Decisiones"),
    ("notion.action_items", "Action items", "Tâches", "Tareas pendientes"),
    ("notion.open_questions", "Open questions", "Questions en suspens", "Preguntas abiertas"),
    ("notion.transcript", "Transcript", "Transcription", "Transcripción"),
    ("notion.due", "due {due}", "échéance {due}", "vence {due}"),
];

fn pick(lang: Lang, key: &str) -> Option<&'static str> {
    TABLE.iter().find(|r| r.0 == key).map(|r| match lang {
        Lang::En => r.1,
        Lang::Fr => r.2,
        Lang::Es => r.3,
    })
}

/// The string for `key` in the current language. An unknown key comes back as the key itself.
pub fn tr(key: &str) -> String {
    tr_in(current(), key, &[])
}

/// Like `tr`, filling `{name}` placeholders.
pub fn tr_with(key: &str, vars: &[(&str, &str)]) -> String {
    tr_in(current(), key, vars)
}

pub fn tr_in(lang: Lang, key: &str, vars: &[(&str, &str)]) -> String {
    let mut s = pick(lang, key).or_else(|| pick(Lang::En, key)).unwrap_or(key).to_string();
    for (name, value) in vars {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(s: &str) -> Vec<String> {
        let mut out: Vec<String> = s.split('{').skip(1).filter_map(|p| p.split('}').next()).map(String::from).collect();
        out.sort();
        out
    }

    #[test]
    fn every_string_exists_in_all_languages_with_the_same_placeholders() {
        for (key, en, fr, es) in TABLE {
            for (lang, text) in [("fr", fr), ("es", es)] {
                assert!(!text.trim().is_empty(), "{lang} {key} is empty");
                assert_eq!(placeholders(text), placeholders(en), "{lang} {key} placeholders");
            }
        }
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<_> = TABLE.iter().map(|r| r.0).collect();
        keys.sort();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len());
    }

    #[test]
    fn spanish_questions_open_with_an_inverted_mark() {
        for (key, _, _, es) in TABLE {
            assert!(!es.contains('?') || es.contains('¿'), "es {key}");
        }
    }

    #[test]
    fn fills_placeholders_and_falls_back() {
        assert_eq!(tr_in(Lang::Fr, "notice.mic_switched", &[("name", "AirPods")]), "Passage à AirPods. L’enregistrement continue.");
        assert_eq!(tr_in(Lang::Es, "tray.quit", &[]), "Salir de Talaan");
        assert_eq!(tr_in(Lang::Fr, "no.such.key", &[]), "no.such.key");
    }

    #[test]
    fn language_codes_parse() {
        assert_eq!(Lang::parse("fr"), Lang::Fr);
        assert_eq!(Lang::parse("es-US"), Lang::Es);
        assert_eq!(Lang::parse("de"), Lang::En);
    }
}
