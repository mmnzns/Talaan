//! Ask: `meetings_ask` answers a question from the meetings in scope (see `ask/`).

use super::{AppState, CmdResult};
use crate::ask::answer::{self, AskAnswer, MeetingRef};
use crate::ask::passages::{passages, Passage};
use crate::ask::{index, query_text, search};
use crate::storage::model::{Meeting, MeetingStatus};
use crate::storage::store::Filter;
use crate::i18n::{tr, tr_err, tr_with};
use crate::storage::StorageError;
use crate::summarizer::local::{helper_path, LocalAi};
use crate::summarizer::summarize::EngineError;
use crate::summarizer::worker::Config;
use crate::models::{Models, SEARCH};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// Most excerpts sent with a question.
const MAX_EXCERPTS: usize = 14;
/// Tokens kept for the system prompt and the reply.
const RESERVED_TOKENS: usize = 1_500;
const REPLY_TOKENS: u32 = 900;

pub struct AskState {
    data_dir: PathBuf,
    models: Arc<Models>,
    /// The search model's helper, shared by questions and the indexer (it exits by itself when idle).
    embedder: Arc<Mutex<LocalAi>>,
    kick: Mutex<mpsc::Sender<()>>,
}

/// Wait this long after the last change before indexing, so a burst of updates is one pass.
const INDEX_DEBOUNCE: Duration = Duration::from_secs(5);

pub fn setup(app: &AppHandle, data_dir: PathBuf) {
    let embedder = Arc::new(Mutex::new(LocalAi::new(&helper_path())));
    let (kick, rx) = mpsc::channel::<()>();
    app.manage(AskState { data_dir: data_dir.clone(), models: app.state::<Arc<Models>>().inner().clone(), embedder: embedder.clone(), kick: Mutex::new(kick) });
    let app = app.clone();
    std::thread::Builder::new()
        .name("talaan-ask-index".into())
        .spawn(move || {
            crate::platform::background_priority();
            // Index once at launch, then after changes.
            let _ = app.state::<AskState>().kick.lock().map(|k| k.send(()));
            while rx.recv().is_ok() {
                while rx.recv_timeout(INDEX_DEBOUNCE).is_ok() {}
                index_all(&app, &embedder);
            }
        })
        .expect("spawn the Ask indexer");
}

/// Asks the indexer for a pass (after a transcript, summary or edit changed, or the search model arrived).
pub fn reindex_soon(app: &AppHandle) {
    if let Some(s) = app.try_state::<AskState>() {
        let _ = s.kick.lock().map(|k| k.send(()));
    }
}

/// Brings every finished meeting's index up to date. Takes the embedder per meeting so questions can
/// interleave instead of waiting for the whole pass.
fn index_all(app: &AppHandle, embedder: &Mutex<LocalAi>) {
    let Some(model) = search_model(&app.state::<AskState>().models) else { return };
    let ids: Vec<String> = {
        let s = app.state::<AppState>();
        let s = s.lock();
        s.list_meetings(&Filter::default()).map(|l| l.into_iter().filter(|m| m.status == MeetingStatus::Ready).map(|m| m.id).collect()).unwrap_or_default()
    };
    let cancel = AtomicBool::new(false);
    for id in ids {
        let loaded = {
            let s = app.state::<AppState>();
            let s = s.lock();
            s.get_meeting(&id).ok().filter(searchable).and_then(|m| s.dir(&id).ok().map(|d| (m, d)))
        };
        let Some((m, dir)) = loaded else { continue };
        let ps = passages(&m);
        let mut ai = embedder.lock().unwrap_or_else(|e| e.into_inner());
        if index::ensure(&dir, SEARCH, &ps, &mut |texts| ai.embed(&model, texts, &cancel)).is_err() {
            return; // the helper failed: try again on the next change rather than hammering it
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Scope {
    Meeting { id: String },
    All,
    Folder { id: String },
    Tag { tag: String },
}

fn refused(message: impl Into<String>) -> StorageError {
    StorageError::Refused(message.into())
}

/// "Oct 14" from the meeting's ISO start.
fn short_date(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso).map(|d| d.format("%b %-d").to_string()).unwrap_or_default()
}

fn searchable(m: &Meeting) -> bool {
    !m.transcript.is_empty() && !matches!(m.item.status, MeetingStatus::Recording | MeetingStatus::Transcribing | MeetingStatus::NeedsRecovery)
}

#[tauri::command]
pub async fn meetings_ask(app: AppHandle, question: String, scope: Scope) -> CmdResult<AskAnswer> {
    tauri::async_runtime::spawn_blocking(move || ask(&app, &question, &scope)).await.map_err(|e| refused(tr_err("ask.stopped", e)))?
}

fn ask(app: &AppHandle, question: &str, scope: &Scope) -> CmdResult<AskAnswer> {
    let question = question.trim();
    if question.is_empty() {
        return Err(refused(tr("ask.empty")));
    }
    let state = app.state::<AskState>();
    let (meetings, dirs, settings) = {
        let storage = app.state::<AppState>();
        let storage = storage.lock();
        let ids: Vec<String> = match scope {
            Scope::Meeting { id } => vec![id.clone()],
            Scope::All => storage.list_meetings(&Filter::default())?.into_iter().map(|m| m.id).collect(),
            Scope::Folder { id } => storage.list_meetings(&Filter { folder_id: Some(id.clone()), ..Default::default() })?.into_iter().map(|m| m.id).collect(),
            Scope::Tag { tag } => storage.list_meetings(&Filter { tag: Some(tag.clone()), ..Default::default() })?.into_iter().map(|m| m.id).collect(),
        };
        let settings = storage.settings_get()?;
        // The guided test recording stays out of answers while its keep-or-delete offer is open (unless it's the
        // meeting being asked about).
        let test = (!matches!(scope, Scope::Meeting { .. })).then(|| pending_test(settings.as_ref())).flatten();
        let meetings: Vec<Meeting> = ids.iter().filter(|id| Some(id.as_str()) != test).filter_map(|id| storage.get_meeting(id).ok()).filter(searchable).collect();
        let dirs: Vec<PathBuf> = meetings.iter().map(|m| storage.dir(&m.item.id)).collect::<Result<_, _>>()?;
        (meetings, dirs, settings)
    };
    let single = matches!(scope, Scope::Meeting { .. });
    answer_from(&state.data_dir, &state.models, &state.embedder, &meetings, &dirs, settings.as_ref(), single, question)
}

/// The test recording whose keep-or-delete offer is still open (`testMeetingId` in settings.json).
pub(crate) fn pending_test(settings: Option<&serde_json::Value>) -> Option<&str> {
    settings?.get("testMeetingId")?.as_str().filter(|s| !s.is_empty())
}

/// The question answered from these meetings (their folders hold the search indexes).
fn answer_from(data_dir: &Path, models: &Models, embedder: &Mutex<LocalAi>, meetings: &[Meeting], dirs: &[PathBuf], settings: Option<&serde_json::Value>, single: bool, question: &str) -> CmdResult<AskAnswer> {
    let total = meetings.len();
    if total == 0 {
        return Ok(AskAnswer::nothing(0));
    }

    let refs: Vec<MeetingRef> = meetings.iter().map(|m| MeetingRef { id: m.item.id.clone(), title: m.item.title.clone(), date: short_date(&m.item.started_at) }).collect();
    let per_meeting: Vec<Vec<Passage>> = meetings.iter().map(passages).collect();
    let all: Vec<(&Passage, &MeetingRef)> = per_meeting.iter().zip(&refs).flat_map(|(ps, r)| ps.iter().map(move |p| (p, r))).collect();

    // Meaning: only when the search model is downloaded. A failure here falls back to keywords.
    let cancel = AtomicBool::new(false);
    let vectors = embed_all(models, embedder, dirs, &per_meeting, question, &cancel).ok().flatten();
    let (query_vec, doc_vecs) = match &vectors {
        Some((q, docs)) => (Some(q.as_slice()), docs.iter().flatten().map(|v| Some(v.as_slice())).collect::<Vec<_>>()),
        None => (None, vec![None; all.len()]),
    };

    let config = Config::from_settings(settings);
    let mut engine = crate::commands::summarizer::make_engine(&config, data_dir, models).map_err(|e| match e {
        EngineError::Unavailable(m) => refused(tr_with("ask.needs_ai", &[("message", &m)])),
        EngineError::Failed(m) => refused(m),
        EngineError::Cancelled => refused(tr("error.cancelled")),
    })?;
    let budget_chars = engine.context_tokens().saturating_sub(RESERVED_TOKENS + REPLY_TOKENS as usize) * 3;

    // One meeting that fits whole: send all of it, in order. Otherwise the best-ranked passages.
    let total_chars: usize = all.iter().map(|(p, _)| p.text.len() + 40).sum();
    let picked: Vec<usize> = if single && total_chars <= budget_chars {
        (0..all.len()).collect()
    } else {
        let docs: Vec<Vec<String>> = all.iter().map(|(p, m)| search::terms(&format!("{} {}", m.title, p.text))).collect();
        let kw = search::bm25(&search::terms(question), &docs);
        let mut used = 0;
        search::rank(&kw, query_vec, &doc_vecs, MAX_EXCERPTS)
            .into_iter()
            .take_while(|&i| {
                used += all[i].0.text.len() + 40;
                used <= budget_chars
            })
            .collect()
    };
    if picked.is_empty() {
        return Ok(AskAnswer::nothing(total));
    }
    let excerpts: Vec<(&Passage, &MeetingRef)> = picked.iter().map(|&i| all[i]).collect();

    let prompt = answer::prompt(question, &excerpts);
    let mut last_err = String::new();
    // One retry on a reply that doesn't parse, as summaries do.
    for _ in 0..2 {
        match engine.complete(&prompt, answer::SCHEMA, REPLY_TOKENS, &cancel) {
            Ok(reply) => match answer::parse(&reply, &excerpts, total) {
                Ok(a) => return Ok(a),
                Err(e) => last_err = e,
            },
            Err(EngineError::Unavailable(m)) | Err(EngineError::Failed(m)) => return Err(refused(m)),
            Err(EngineError::Cancelled) => return Err(refused(tr("error.cancelled"))),
        }
    }
    Err(refused(tr_err("ask.unreadable", last_err)))
}

/// The question's vector and every meeting's passage vectors (building missing indexes now), or None when
/// the search model isn't downloaded.
#[allow(clippy::type_complexity)]
fn embed_all(models: &Models, embedder: &Mutex<LocalAi>, dirs: &[PathBuf], per_meeting: &[Vec<Passage>], question: &str, cancel: &AtomicBool) -> Result<Option<(Vec<f32>, Vec<Vec<Vec<f32>>>)>, String> {
    let Some(model) = search_model(models) else { return Ok(None) };
    let mut ai = embedder.lock().unwrap_or_else(|e| e.into_inner());
    let mut embed = |texts: Vec<String>| ai.embed(&model, texts, cancel);
    let mut docs = Vec::with_capacity(dirs.len());
    for (dir, ps) in dirs.iter().zip(per_meeting) {
        docs.push(index::ensure(dir, SEARCH, ps, &mut embed)?);
    }
    let q = embed(vec![query_text(question)])?.pop().ok_or("no vector for the question")?;
    Ok(Some((q, docs)))
}

pub fn search_model(models: &Models) -> Option<PathBuf> {
    let file = models.info(SEARCH)?.files.first()?.name;
    models.ready_dir(SEARCH).map(|dir| dir.join(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pending_test_recording_is_known_from_settings() {
        assert_eq!(pending_test(Some(&serde_json::json!({ "testMeetingId": "m1" }))), Some("m1"));
        assert_eq!(pending_test(Some(&serde_json::json!({ "testMeetingId": "" }))), None);
        assert_eq!(pending_test(Some(&serde_json::json!({}))), None);
        assert_eq!(pending_test(None), None);
    }

    #[test]
    fn scopes_come_from_the_frontend_shape() {
        let s: Scope = serde_json::from_str(r#"{"kind":"meeting","id":"m1"}"#).unwrap();
        assert!(matches!(s, Scope::Meeting { id } if id == "m1"));
        assert!(matches!(serde_json::from_str::<Scope>(r#"{"kind":"all"}"#).unwrap(), Scope::All));
        assert!(matches!(serde_json::from_str::<Scope>(r#"{"kind":"tag","tag":"roadmap"}"#).unwrap(), Scope::Tag { tag } if tag == "roadmap"));
    }

    /// Six made-up meetings, real local AI and search model, no network.
    /// `TALAAN_SMOKE_DATA=<scratch folder whose shared-models holds the models> cargo build -p talaan-llm && cargo test ask_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn ask_smoke() {
        let data = PathBuf::from(std::env::var("TALAAN_SMOKE_DATA").expect("a scratch data folder"));
        let models = Models::open(Some(&data)).unwrap();
        let calls: [(&str, &str, &[(u64, &str, &str)]); 6] = [
            ("Q4 roadmap sync", "2026-10-01T10:00:00-07:00", &[(0, "me", "Let's settle the order for next quarter."), (21_000, "them:1", "Onboarding ships first. Pricing moves to Q1, after the revamp."), (40_000, "me", "Agreed, I'll tell design.")]),
            ("Vendor demo, Acme analytics", "2026-10-03T13:00:00-07:00", &[(0, "them:0", "Our dashboard loads in under two seconds."), (65_000, "them:0", "We can have the SSO integration ready by November 15th, that's a promise."), (90_000, "me", "Send the security questionnaire too.")]),
            ("Weekly 1:1 with Priya", "2026-10-06T09:00:00-07:00", &[(0, "them:0", "I'm worried about hiring for the data role."), (300_000, "me", "Let's open the req this week and use the agency.")]),
            ("Launch planning", "2026-10-08T15:00:00-07:00", &[(0, "them:1", "The landing page is done."), (24_000, "them:2", "I'll email legal today and ask for a turnaround by Monday."), (36_000, "them:1", "The fallback launch date is the 20th.")]),
            ("Budget review", "2026-10-10T11:00:00-07:00", &[(0, "them:0", "Marketing spend is twelve percent over plan."), (50_000, "me", "We'll cut the conference sponsorship to get back on track.")]),
            ("Design crit", "2026-10-13T14:00:00-07:00", &[(0, "them:0", "The new onboarding screens feel cramped on small laptops."), (30_000, "me", "Let's try a two-column layout.")]),
        ];
        let tmp = tempfile::tempdir().unwrap();
        let mut meetings = vec![];
        let mut dirs = vec![];
        for (i, (title, at, lines)) in calls.iter().enumerate() {
            let mut m = crate::ask::passages::tests::meeting(&format!("m{i}"), lines);
            m.item.title = title.to_string();
            m.item.started_at = at.to_string();
            meetings.push(m);
            let d = tmp.path().join(format!("m{i}"));
            std::fs::create_dir_all(&d).unwrap();
            dirs.push(d);
        }
        let exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join(crate::summarizer::local::helper_file());
        let embedder = Mutex::new(LocalAi::new(&exe));
        let settings = serde_json::json!({ "summaryEngine": "local", "localModel": "recommended" });
        for (q, want_meeting, want_at) in [
            ("When did the vendor promise SSO?", "m1", 65_000),
            ("Who is talking to the lawyers about the launch?", "m3", 24_000),
            ("What did we decide about pricing?", "m0", 21_000),
            ("How are we fixing the overspend?", "m4", 50_000),
        ] {
            let t = std::time::Instant::now();
            let a = answer_from(&data, &models, &embedder, &meetings, &dirs, Some(&settings), false, q).unwrap();
            eprintln!("\n{q} ({:.1}s)\n  {}", t.elapsed().as_secs_f32(), a.text);
            for p in &a.points {
                eprintln!("  - {} {:?}", p.text, p.citation.as_ref().map(|c| (&c.meeting_title, &c.date, c.at)));
            }
            let cites: Vec<_> = a.points.iter().filter_map(|p| p.citation.as_ref()).map(|c| (c.meeting_id.as_str(), c.at)).collect();
            assert!(cites.contains(&(want_meeting, want_at)), "{q}: {cites:?}");
            assert_eq!(a.total_meetings, 6);
        }
    }

    #[test]
    fn dates_are_short() {
        assert_eq!(short_date("2026-10-04T14:30:00-07:00"), "Oct 4");
        assert_eq!(short_date("nope"), "");
    }
}
