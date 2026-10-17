//! The strings only Rust shows: the menu bar menu, the "Summary ready" notification, recorder warnings, the Notion
//! export's headings, and the errors and refusals the app shows (`tr_err` keeps the technical detail in English). The screens have their own tables (src/lib/i18n); this mirrors its rules: every string in
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
    // Windows calendar (its iCal address): errors shown under the address field in Settings › Connections.
    (
        "calendar.bad_address",
        "That doesn't look like a calendar address. Copy the whole address, starting with https://.",
        "Cela ne ressemble pas à une adresse de calendrier. Copiez l’adresse au complet, à partir de https://.",
        "Eso no parece una dirección de calendario. Copia la dirección completa, desde https://.",
    ),
    (
        "calendar.too_slow",
        "The calendar didn't finish downloading within 15 seconds. Check your connection and try again.",
        "Le téléchargement du calendrier ne s’est pas terminé en 15 secondes. Vérifiez votre connexion et réessayez.",
        "El calendario no terminó de descargarse en 15 segundos. Revisa tu conexión e inténtalo de nuevo.",
    ),
    (
        "calendar.unreachable",
        "Couldn't reach the calendar address. Check your connection and the address.",
        "Impossible de joindre l’adresse du calendrier. Vérifiez votre connexion et l’adresse.",
        "No se pudo acceder a la dirección del calendario. Revisa tu conexión y la dirección.",
    ),
    (
        "calendar.http_failed",
        "The calendar address didn't work (HTTP {status}). Copy it again from your calendar.",
        "L’adresse du calendrier n’a pas fonctionné (HTTP {status}). Copiez-la de nouveau depuis votre calendrier.",
        "La dirección del calendario no funcionó (HTTP {status}). Cópiala otra vez desde tu calendario.",
    ),
    (
        "calendar.not_ics",
        "That address didn't return a calendar. Copy the address in iCal format (it ends in .ics).",
        "Cette adresse n’a pas renvoyé de calendrier. Copiez l’adresse au format iCal (elle se termine par .ics).",
        "Esa dirección no devolvió un calendario. Copia la dirección en formato iCal (termina en .ics).",
    ),
    ("calendar.interrupted", "The download was interrupted. Try again.", "Le téléchargement a été interrompu. Réessayez.", "La descarga se interrumpió. Inténtalo de nuevo."),
    (
        "calendar.too_large",
        "This calendar is larger than 20 MB, which Talaan doesn't read.",
        "Ce calendrier dépasse 20 Mo, une taille que Talaan ne lit pas.",
        "Este calendario pesa más de 20 MB, un tamaño que Talaan no lee.",
    ),
    ("notion.recorded_with", "recorded with Talaan", "enregistré avec Talaan", "grabado con Talaan"),
    ("notion.recap", "Recap", "Récapitulatif", "Resumen breve"),
    ("notion.decisions", "Decisions", "Décisions", "Decisiones"),
    ("notion.action_items", "Action items", "Tâches", "Tareas pendientes"),
    ("notion.open_questions", "Open questions", "Questions en suspens", "Preguntas abiertas"),
    ("notion.transcript", "Transcript", "Transcription", "Transcripción"),
    ("notion.due", "due {due}", "échéance {due}", "vence {due}"),
    // Errors and refusals the app shows (toasts, Settings results, meeting banners). `{error}` is the system's own
    // technical detail and stays in English.
    ("error.cancelled", "Cancelled.", "Annulé.", "Cancelado."),
    ("error.open_folder", "Couldn't open the folder: {error}", "Impossible d’ouvrir le dossier : {error}", "No se pudo abrir la carpeta: {error}"),
    (
        "error.no_log",
        "Talaan couldn't write its log file on this computer, so there is no log folder to open.",
        "Talaan n’a pas pu écrire son fichier journal sur cet ordinateur, donc il n’y a pas de dossier de journal à ouvrir.",
        "Talaan no pudo escribir su archivo de registro en esta computadora, así que no hay carpeta de registro que abrir.",
    ),
    ("error.save_file", "Couldn't save the file: {error}", "Impossible d’enregistrer le fichier : {error}", "No se pudo guardar el archivo: {error}"),
    ("error.wait_meeting", "Wait for this meeting to finish first.", "Attendez d’abord que cette réunion soit terminée.", "Primero espera a que termine esta reunión."),
    (
        "error.audio_gone",
        "The audio for this call is gone, so it can't be transcribed again.",
        "L’audio de cet appel a été supprimé, il ne peut donc pas être transcrit de nouveau.",
        "El audio de esta llamada ya no está, así que no se puede volver a transcribir.",
    ),
    (
        "error.audio_needed",
        "The audio is still needed for the transcript.",
        "L’audio est encore nécessaire pour la transcription.",
        "El audio todavía se necesita para la transcripción.",
    ),
    (
        "error.download_speech_model",
        "Download {name} in Settings › Speech models first.",
        "Téléchargez d’abord {name} dans Paramètres › Modèles vocaux.",
        "Primero descarga {name} en Ajustes › Modelos de voz.",
    ),
    ("error.delete_model", "Couldn't delete the model: {error}", "Impossible de supprimer le modèle : {error}", "No se pudo eliminar el modelo: {error}"),
    ("error.save_transcript", "Couldn't save the transcript.", "Impossible d’enregistrer la transcription.", "No se pudo guardar la transcripción."),
    ("error.save_summary", "Couldn't save the summary.", "Impossible d’enregistrer le résumé.", "No se pudo guardar el resumen."),
    (
        "error.no_transcript_yet",
        "There's no transcript to summarize yet.",
        "Il n’y a pas encore de transcription à résumer.",
        "Todavía no hay una transcripción para resumir.",
    ),
    (
        "error.wait_transcript",
        "Wait for the transcript to finish first.",
        "Attendez d’abord que la transcription soit terminée.",
        "Primero espera a que termine la transcripción.",
    ),
    ("error.summary_empty", "There's no transcript to summarize.", "Il n’y a pas de transcription à résumer.", "No hay una transcripción para resumir."),
    (
        "error.summary_too_short",
        "This recording is too short to summarize, so only the transcript is saved.",
        "Cet enregistrement est trop court pour être résumé, donc seule la transcription est enregistrée.",
        "Esta grabación es demasiado corta para resumirla, así que solo se guarda la transcripción.",
    ),
    (
        "error.summary_bad_output",
        "The summary AI gave an answer Talaan couldn't use ({error}).",
        "L’IA de résumé a donné une réponse que Talaan n’a pas pu utiliser ({error}).",
        "La IA de resumen dio una respuesta que Talaan no pudo usar ({error}).",
    ),
    // Summary AI setup and the AI services' answers.
    (
        "ai.server_no_models",
        "Ollama or LM Studio has no models yet. Download one there first.",
        "Ollama ou LM Studio n’a pas encore de modèle. Téléchargez-en un d’abord dans cette application.",
        "Ollama o LM Studio todavía no tiene modelos. Primero descarga uno ahí.",
    ),
    (
        "ai.add_key",
        "Add your {provider} API key in Settings › Summary AI.",
        "Ajoutez votre clé d’API {provider} dans Paramètres › IA de résumé.",
        "Agrega tu clave de API de {provider} en Ajustes › IA de resumen.",
    ),
    (
        "ai.no_suitable_model",
        "Couldn't find a suitable {provider} model for this key. Pick one in Settings › Summary AI.",
        "Aucun modèle {provider} adapté n’a été trouvé pour cette clé. Choisissez-en un dans Paramètres › IA de résumé.",
        "No se encontró un modelo de {provider} adecuado para esta clave. Elige uno en Ajustes › IA de resumen.",
    ),
    (
        "ai.download_local",
        "Download the local AI model ({name}, {size} GB) in Settings › Summary AI.",
        "Téléchargez le modèle d’IA local ({name}, {size} Go) dans Paramètres › IA de résumé.",
        "Descarga el modelo de IA local ({name}, {size} GB) en Ajustes › IA de resumen.",
    ),
    ("ai.connected", "Connected.", "Connecté.", "Conectado."),
    ("ai.connected_model", "Connected. Summaries will use {model}.", "Connecté. Les résumés utiliseront {model}.", "Conectado. Los resúmenes usarán {model}."),
    (
        "ai.key_rejected",
        "{provider} didn't accept the API key. Check it in Settings › Summary AI.",
        "{provider} n’a pas accepté la clé d’API. Vérifiez-la dans Paramètres › IA de résumé.",
        "{provider} no aceptó la clave de API. Revísala en Ajustes › IA de resumen.",
    ),
    (
        "ai.no_such_model",
        "{provider} doesn't have that model. Pick another in Settings › Summary AI. {detail}",
        "{provider} n’a pas ce modèle. Choisissez-en un autre dans Paramètres › IA de résumé. {detail}",
        "{provider} no tiene ese modelo. Elige otro en Ajustes › IA de resumen. {detail}",
    ),
    (
        "ai.rate_limited",
        "{provider} is rate limiting or out of credit. Try again later. {detail}",
        "{provider} limite les requêtes ou n’a plus de crédit. Réessayez plus tard. {detail}",
        "{provider} está limitando las solicitudes o se quedó sin crédito. Inténtalo más tarde. {detail}",
    ),
    ("ai.http_error", "{provider} returned an error ({status}). {detail}", "{provider} a renvoyé une erreur ({status}). {detail}", "{provider} devolvió un error ({status}). {detail}"),
    (
        "ai.unreachable",
        "Couldn't reach {provider} at {url}. Is it running, and is this computer online?",
        "Impossible de joindre {provider} à {url}. Est-il ouvert, et cet ordinateur est-il en ligne ?",
        "No se pudo conectar con {provider} en {url}. ¿Está abierto y esta computadora tiene conexión?",
    ),
    ("ai.too_slow", "{provider} took too long to answer.", "{provider} a mis trop de temps à répondre.", "{provider} tardó demasiado en responder."),
    ("ai.failed", "Couldn't reach {provider}: {error}", "Impossible de joindre {provider} : {error}", "No se pudo conectar con {provider}: {error}"),
    ("ai.empty_reply", "{provider} sent an empty reply", "{provider} a envoyé une réponse vide", "{provider} envió una respuesta vacía"),
    // The local AI (Talaan's own helper process)
    ("local_ai.no_start", "The local AI didn't start.", "L’IA locale n’a pas démarré.", "La IA local no se inició."),
    ("local_ai.too_slow", "The local AI took too long.", "L’IA locale a mis trop de temps.", "La IA local tardó demasiado."),
    (
        "local_ai.crashed",
        "The local AI stopped unexpectedly. Try again.",
        "L’IA locale s’est arrêtée de façon inattendue. Réessayez.",
        "La IA local se detuvo de forma inesperada. Vuelve a intentarlo.",
    ),
    (
        "local_ai.out_of_memory",
        "The local AI ran out of memory. Try the smaller model, or close other apps.",
        "L’IA locale a manqué de mémoire. Essayez le modèle plus petit ou fermez d’autres applications.",
        "La IA local se quedó sin memoria. Prueba el modelo más pequeño o cierra otras apps.",
    ),
    // Ask
    ("ask.stopped", "Ask stopped unexpectedly: {error}", "Demander s’est arrêté de façon inattendue : {error}", "Preguntar se detuvo de forma inesperada: {error}"),
    ("ask.empty", "Type a question first.", "Tapez d’abord une question.", "Primero escribe una pregunta."),
    ("ask.needs_ai", "Ask uses your summary AI. {message}", "Demander utilise votre IA de résumé. {message}", "Preguntar usa tu IA de resumen. {message}"),
    (
        "ask.unreadable",
        "The summary AI's answer couldn't be read ({error}). Try again.",
        "La réponse de l’IA de résumé n’a pas pu être lue ({error}). Réessayez.",
        "No se pudo leer la respuesta de la IA de resumen ({error}). Inténtalo de nuevo.",
    ),
    // Updates
    ("update.check_failed", "Couldn't check for updates: {error}", "Impossible de vérifier les mises à jour : {error}", "No se pudo buscar actualizaciones: {error}"),
    (
        "update.offline",
        "Couldn't check for updates. Is this computer online? ({error})",
        "Impossible de vérifier les mises à jour. Cet ordinateur est-il en ligne ? ({error})",
        "No se pudo buscar actualizaciones. ¿Esta computadora tiene conexión? ({error})",
    ),
    (
        "update.stop_recording",
        "Stop the recording first, then update.",
        "Arrêtez d’abord l’enregistrement, puis faites la mise à jour.",
        "Primero detén la grabación y luego actualiza.",
    ),
    ("update.up_to_date", "Talaan is up to date.", "Talaan est à jour.", "Talaan está actualizado."),
    ("update.install_failed", "The update didn't install: {error}", "La mise à jour ne s’est pas installée : {error}", "La actualización no se instaló: {error}"),
    // Notion
    (
        "notion.connect_first",
        "Connect Notion in Settings › Connections first.",
        "Connectez d’abord Notion dans Paramètres › Connexions.",
        "Primero conecta Notion en Ajustes › Conexiones.",
    ),
    ("notion.stopped", "Notion stopped unexpectedly: {error}", "Notion s’est arrêté de façon inattendue : {error}", "Notion se detuvo de forma inesperada: {error}"),
    ("notion.paste_secret", "Paste your integration secret first.", "Collez d’abord le secret de votre intégration.", "Primero pega el secreto de tu integración."),
    (
        "notion.offline",
        "Couldn't reach Notion. Is this computer online?",
        "Impossible de joindre Notion. Cet ordinateur est-il en ligne ?",
        "No se pudo conectar con Notion. ¿Esta computadora tiene conexión?",
    ),
    ("notion.unreachable", "Couldn't reach Notion: {error}", "Impossible de joindre Notion : {error}", "No se pudo conectar con Notion: {error}"),
    (
        "notion.bad_secret",
        "Notion didn't accept that secret. Copy it again from your integration's page in Notion.",
        "Notion n’a pas accepté ce secret. Copiez-le de nouveau depuis la page de votre intégration dans Notion.",
        "Notion no aceptó ese secreto. Cópialo otra vez desde la página de tu integración en Notion.",
    ),
    (
        "notion.no_access",
        "Talaan can't see that page. In Notion, open it, then ••• › Connections › add your Talaan integration.",
        "Talaan ne voit pas cette page. Dans Notion, ouvrez-la, puis ••• › Connexions › ajoutez votre intégration Talaan.",
        "Talaan no puede ver esa página. En Notion, ábrela y luego ••• › Conexiones › agrega tu integración de Talaan.",
    ),
    ("notion.busy", "Notion is busy. Try again in a minute.", "Notion est occupé. Réessayez dans une minute.", "Notion está ocupado. Inténtalo de nuevo en un minuto."),
    ("notion.said", "Notion said: {message}", "Notion a répondu : {message}", "Notion respondió: {message}"),
    ("notion.no_page", "Notion didn't return the new page.", "Notion n’a pas renvoyé la nouvelle page.", "Notion no devolvió la página nueva."),
    // Importing an audio file
    ("import.file_gone", "That file isn't there anymore.", "Ce fichier n’est plus là.", "Ese archivo ya no está."),
    ("import.save_failed", "Couldn't save the imported audio: {error}", "Impossible d’enregistrer l’audio importé : {error}", "No se pudo guardar el audio importado: {error}"),
    (
        "import.unreadable",
        "Talaan can't read this file ({error}). Try an M4A, MP3, WAV or FLAC file.",
        "Talaan ne peut pas lire ce fichier ({error}). Essayez un fichier M4A, MP3, WAV ou FLAC.",
        "Talaan no puede leer este archivo ({error}). Prueba con un archivo M4A, MP3, WAV o FLAC.",
    ),
    ("import.open_failed", "Couldn't open the file: {error}", "Impossible d’ouvrir le fichier : {error}", "No se pudo abrir el archivo: {error}"),
    ("import.no_audio", "The file has no audio in it.", "Ce fichier ne contient pas d’audio.", "El archivo no tiene audio."),
    ("import.write_failed", "Couldn't write the audio: {error}", "Impossible d’écrire l’audio : {error}", "No se pudo escribir el audio: {error}"),
    // Secure key storage (Keychain on the Mac, Credential Manager on Windows)
    ("keys.open_failed", "Couldn't open secure key storage: {error}", "Impossible d’ouvrir le stockage sécurisé des clés : {error}", "No se pudo abrir el almacén seguro de claves: {error}"),
    (
        "keys.save_failed",
        "Couldn't save the key in secure key storage: {error}",
        "Impossible d’enregistrer la clé dans le stockage sécurisé des clés : {error}",
        "No se pudo guardar la clave en el almacén seguro de claves: {error}",
    ),
    (
        "keys.remove_failed",
        "Couldn't remove the key from secure key storage: {error}",
        "Impossible de retirer la clé du stockage sécurisé des clés : {error}",
        "No se pudo quitar la clave del almacén seguro de claves: {error}",
    ),
    // Recorder
    ("recorder.already_recording", "Already recording", "Enregistrement déjà en cours", "Ya se está grabando"),
    ("recorder.not_recording", "Not recording", "Aucun enregistrement en cours", "No se está grabando"),
    ("recorder.no_mic_found", "No microphone found", "Aucun microphone trouvé", "No se encontró ningún micrófono"),
    ("recorder.no_mic", "No microphone", "Aucun microphone", "Sin micrófono"),
    // Model downloads (shown as a warning notice)
    ("download.network", "Couldn't download {name}: {error}", "Impossible de télécharger {name} : {error}", "No se pudo descargar {name}: {error}"),
    ("download.corrupt", "{name} was damaged in transit. Try again.", "{name} a été endommagé pendant le téléchargement. Réessayez.", "{name} se dañó durante la descarga. Inténtalo de nuevo."),
    ("download.save_failed", "Couldn't save the model: {error}", "Impossible d’enregistrer le modèle : {error}", "No se pudo guardar el modelo: {error}"),
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

/// A number with one decimal, written the way the current language writes it ("2.7" / "2,7").
pub fn decimal(n: f64) -> String {
    let s = format!("{n:.1}");
    if current() == Lang::Fr { s.replace('.', ",") } else { s }
}

/// Like `tr`, with the technical detail of an error (left in English) in `{error}`.
pub fn tr_err(key: &str, error: impl std::fmt::Display) -> String {
    tr_with(key, &[("error", &error.to_string())])
}

/// Which message a finished string came from, in any language: the key whose template matches it with every
/// `{placeholder}` standing for any text. The log records this key instead of the message, because placeholders
/// can carry what a provider, Notion or a model said. None for text that isn't from the table.
pub fn key_for(message: &str) -> Option<&'static str> {
    let message = message.trim();
    TABLE.iter().find(|r| [r.1, r.2, r.3].iter().any(|t| matches_template(t, message))).map(|r| r.0)
}

fn matches_template(template: &str, message: &str) -> bool {
    // Literal pieces between placeholders, trimmed (an empty `{detail}` leaves its spaces behind, then gets trimmed).
    let mut pieces = vec![];
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        pieces.push(rest[..open].trim());
        rest = &rest[open + close + 1..];
    }
    pieces.push(rest.trim());
    let literal: usize = pieces.iter().map(|p| p.len()).sum();
    if literal < 8 {
        return false; // too little fixed text to tell messages apart
    }
    let (first, last) = (pieces[0], pieces[pieces.len() - 1]);
    if pieces.len() == 1 {
        return message == first;
    }
    if !message.starts_with(first) || !message.ends_with(last) || message.len() < first.len() + last.len() {
        return false;
    }
    let mut at = first.len();
    let end = message.len() - last.len();
    for p in &pieces[1..pieces.len() - 1] {
        if at > end {
            return false;
        }
        match message[at..end].find(p) {
            Some(i) => at += i + p.len(),
            None => return false,
        }
    }
    true
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
    fn every_key_the_code_asks_for_exists() {
        fn walk(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let text = std::fs::read_to_string(&p).unwrap();
                    for call in ["tr(\"", "tr_with(\"", "tr_err(\""] {
                        for (at, _) in text.match_indices(call) {
                            // `tr(` itself, not `from_str(`.
                            if text[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                                continue;
                            }
                            let key = text[at + call.len()..].split('"').next().unwrap_or("");
                            out.push((p.display().to_string(), key.to_string()));
                        }
                    }
                }
            }
        }
        let mut used = vec![];
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut used);
        assert!(used.len() > 50, "found {} tr calls", used.len());
        for (file, key) in used {
            if key == "no.such.key" || key.contains('{') {
                continue;
            }
            assert!(pick(Lang::En, &key).is_some(), "{file} asks for missing key {key}");
        }
    }

    #[test]
    fn errors_come_in_the_chosen_language_with_their_detail() {
        assert_eq!(tr_in(Lang::Fr, "update.up_to_date", &[]), "Talaan est à jour.");
        assert_eq!(
            tr_in(Lang::Es, "update.install_failed", &[("error", "signature mismatch")]),
            "La actualización no se instaló: signature mismatch"
        );
        assert_eq!(tr_err("error.open_folder", "denied"), "Couldn't open the folder: denied");
        assert_eq!(decimal(2.74), "2.7");
    }

    #[test]
    fn language_codes_parse() {
        assert_eq!(Lang::parse("fr"), Lang::Fr);
        assert_eq!(Lang::parse("es-US"), Lang::Es);
        assert_eq!(Lang::parse("de"), Lang::En);
    }
}
