//! One meeting's summary, whatever the engine: fit the transcript to the engine's context (splitting long
//! calls), ask, parse, retry once on unusable output, merge parts, and pick a title.

use super::chunk::{estimate_tokens, split};
use super::parse::{parse_draft, to_highlights, to_sections, to_summary, usable_title, Draft};
use super::prompt::{merge_prompt, moments, schema, summary_prompt, Options, Prompt};
use crate::storage::model::{Meeting, Summary, TranscriptLine};
use std::sync::atomic::{AtomicBool, Ordering};

/// Room left for the reply in every call.
pub const MAX_OUTPUT_TOKENS: u32 = 1_500;

#[derive(Debug, Clone, PartialEq)]
pub enum EngineError {
    /// The engine can't run at all right now (no model, no key, server not running). Nothing to retry.
    Unavailable(String),
    /// The call failed (network, crash, refusal). May work next time.
    Failed(String),
    Cancelled,
}

pub trait SummaryEngine: Send {
    /// One request. The reply should be JSON matching `schema`; engines that can enforce it do.
    fn complete(&mut self, prompt: &Prompt, schema: &str, max_tokens: u32, cancel: &AtomicBool) -> Result<String, EngineError>;
    /// Total context in tokens (prompt + reply).
    fn context_tokens(&self) -> usize;
    /// Shown under the summary ("Local AI · Qwen3.5 4B", "Claude").
    fn label(&self) -> String;
    /// After a failed `complete`: what the engine knows about why, for debugging (kept beside the message).
    fn failure_detail(&mut self) -> Option<String> {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub summary: Summary,
    /// A title from the summary, when it gave a usable one.
    pub title: Option<String>,
    pub parts: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SummarizeError {
    Empty,
    /// Under a second of speech, or a line or two with almost no words: nothing a summary could say.
    TooShort,
    Engine(EngineError),
    /// Two replies in a row couldn't be used.
    BadOutput(String),
}

impl SummarizeError {
    pub fn message(&self) -> String {
        match self {
            SummarizeError::Empty => crate::i18n::tr("error.summary_empty"),
            SummarizeError::TooShort => crate::i18n::tr("error.summary_too_short"),
            SummarizeError::Engine(EngineError::Unavailable(m)) | SummarizeError::Engine(EngineError::Failed(m)) => m.clone(),
            SummarizeError::Engine(EngineError::Cancelled) => crate::i18n::tr("error.cancelled"),
            SummarizeError::BadOutput(m) => crate::i18n::tr_err("error.summary_bad_output", m),
        }
    }
}

/// `language`: ISO code to write in. `opts`: the meeting type and custom instructions. `progress` gets 0..1.
pub fn summarize(meeting: &Meeting, engine: &mut dyn SummaryEngine, language: &str, opts: &Options, cancel: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<Outcome, SummarizeError> {
    let lines = &meeting.transcript;
    if lines.iter().all(|l| l.text.trim().is_empty()) {
        return Err(SummarizeError::Empty);
    }
    if too_short(lines) {
        return Err(SummarizeError::TooShort);
    }
    // What the instructions and header cost, measured on an empty transcript.
    let overhead = {
        let p = summary_prompt(meeting, &[], language, Some((1, 1)), opts);
        estimate_tokens(&p.system) + estimate_tokens(&p.user) + 64
    };
    let budget = (engine.context_tokens() * 9 / 10).saturating_sub(overhead + MAX_OUTPUT_TOKENS as usize).max(500);
    let parts = split(lines, budget);
    let n = parts.len();
    let schema = schema(!opts.template().is_general(), !moments(meeting).is_empty());
    progress(0.0);

    if n == 1 {
        let draft = ask(engine, &summary_prompt(meeting, lines, language, None, opts), &schema, cancel)?;
        progress(1.0);
        return Ok(finish(&draft, meeting, engine, 1, opts, language));
    }

    let mut drafts = vec![];
    for (i, part) in parts.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(SummarizeError::Engine(EngineError::Cancelled));
        }
        drafts.push(ask(engine, &summary_prompt(meeting, part, language, Some((i + 1, n)), opts), &schema, cancel)?);
        progress((i + 1) as f32 / (n + 1) as f32);
    }
    let parts_json: Vec<String> = drafts.iter().map(|d| serde_json::to_string(d).expect("drafts serialize")).collect();
    let merged = ask(engine, &merge_prompt(meeting, &parts_json, language, opts), &schema, cancel)?;
    progress(1.0);
    Ok(finish(&merged, meeting, engine, n, opts, language))
}

/// Too little to summarize: under 1 s of speech in all, or at most two lines with fewer than 12 words between
/// them. A model asked to summarize "Can you hear me?" invents a recap and a title.
pub fn too_short(lines: &[TranscriptLine]) -> bool {
    let spoken: Vec<&TranscriptLine> = lines.iter().filter(|l| !l.text.trim().is_empty()).collect();
    let speech_ms: u64 = spoken.iter().map(|l| l.end.saturating_sub(l.at)).sum();
    let words: usize = spoken.iter().map(|l| l.text.split_whitespace().count()).sum();
    speech_ms < 1_000 || (spoken.len() <= 2 && words < 12)
}

fn finish(draft: &Draft, meeting: &Meeting, engine: &dyn SummaryEngine, parts: usize, opts: &Options, language: &str) -> Outcome {
    let mut summary = to_summary(draft, &meeting.transcript, &engine.label());
    let template = opts.template();
    summary.sections = to_sections(draft, &meeting.transcript, template, language);
    summary.highlights = to_highlights(draft, meeting);
    summary.template = Some(template.id.to_string());
    Outcome { summary, title: usable_title(&draft.title), parts }
}

/// One request, with one retry that tells the model what was wrong.
fn ask(engine: &mut dyn SummaryEngine, prompt: &Prompt, schema: &str, cancel: &AtomicBool) -> Result<Draft, SummarizeError> {
    let first = engine.complete(prompt, schema, MAX_OUTPUT_TOKENS, cancel).map_err(SummarizeError::Engine)?;
    let problem = match parse_draft(&first) {
        Ok(d) => return Ok(d),
        Err(e) => e,
    };
    let retry = Prompt { system: prompt.system.clone(), user: format!("{}\n\nYour previous reply couldn't be used: {problem}. Reply with only the JSON object.", prompt.user) };
    let second = engine.complete(&retry, schema, MAX_OUTPUT_TOKENS, cancel).map_err(SummarizeError::Engine)?;
    parse_draft(&second).map_err(SummarizeError::BadOutput)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::summarizer::prompt::tests::{line, meeting};

    /// Replies from a script; records the prompts it got.
    pub struct Scripted {
        pub replies: Vec<Result<String, EngineError>>,
        pub prompts: Vec<Prompt>,
        pub context: usize,
    }

    impl SummaryEngine for Scripted {
        fn complete(&mut self, p: &Prompt, _s: &str, _m: u32, _c: &AtomicBool) -> Result<String, EngineError> {
            self.prompts.push(p.clone());
            if self.replies.is_empty() {
                return Ok(reply("Default", "Part summary."));
            }
            self.replies.remove(0)
        }
        fn context_tokens(&self) -> usize {
            self.context
        }
        fn label(&self) -> String {
            "Scripted".into()
        }
    }

    pub fn reply(title: &str, recap: &str) -> String {
        format!(r#"{{"title":"{title}","recap":"{recap}","decisions":[{{"text":"Ship onboarding first.","time":"01:00"}}],"action_items":[],"open_questions":[]}}"#)
    }

    fn short() -> Meeting {
        meeting(vec![line("l0", 0, "me", "Hi."), line("l1", 60_000, "them:1", "Onboarding first, pricing in Q1."), line("l2", 125_000, "me", "I'll send the wireframes Friday.")])
    }

    #[test]
    fn a_short_call_takes_one_request() {
        let mut e = Scripted { replies: vec![Ok(reply("Q4 roadmap sync", "We planned Q4."))], prompts: vec![], context: 16_384 };
        let mut seen = vec![];
        let out = summarize(&short(), &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |p| seen.push(p)).unwrap();
        assert_eq!(e.prompts.len(), 1);
        assert_eq!(out.summary.recap, "We planned Q4.");
        assert_eq!(out.summary.decisions[0].at, Some(60_000));
        assert_eq!(out.summary.engine, "Scripted");
        assert_eq!(out.title.as_deref(), Some("Q4 roadmap sync"));
        assert_eq!(seen, vec![0.0, 1.0]);
    }

    #[test]
    fn a_meeting_type_and_marked_moments_shape_the_schema_and_the_summary() {
        /// Answers once and keeps the schema it was given.
        struct Keeps(Vec<String>);
        impl SummaryEngine for Keeps {
            fn complete(&mut self, _p: &Prompt, schema: &str, _m: u32, _c: &AtomicBool) -> Result<String, EngineError> {
                self.0.push(schema.to_string());
                Ok(r#"{"title":"Interview","recap":"We met Sam.","decisions":[],"action_items":[],"open_questions":[],
                    "sections":[{"title":"Strengths","items":[{"text":"Led a migration.","time":"01:00"}]}],
                    "highlights":[{"time":"01:00","text":"Sam described the migration."}]}"#.into())
            }
            fn context_tokens(&self) -> usize {
                16_384
            }
            fn label(&self) -> String {
                "Keeps".into()
            }
        }
        let mut m = short();
        m.marks = vec![crate::summarizer::prompt::tests::mark("k0", 61_000, None)];
        let mut e = Keeps(vec![]);
        let opts = Options { template: "interview".into(), instructions: String::new() };
        let out = summarize(&m, &mut e, "en", &opts, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert!(e.0[0].contains("\"sections\"") && e.0[0].contains("\"highlights\""));
        assert_eq!(out.summary.template.as_deref(), Some("interview"));
        assert_eq!(out.summary.sections[0].title, "Strengths");
        assert_eq!(out.summary.highlights[0].text, "Sam described the migration.");
        assert_eq!(out.summary.highlights[0].at, Some(60_000));
        // General, no marks: the original schema.
        let mut plain = short();
        plain.marks.clear();
        let mut e = Keeps(vec![]);
        let out = summarize(&plain, &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(e.0[0], crate::summarizer::prompt::SCHEMA);
        assert!(out.summary.sections.is_empty() && out.summary.highlights.is_empty());
        assert_eq!(out.summary.template.as_deref(), Some("general"));
    }

    #[test]
    fn unusable_output_is_retried_once_with_the_reason() {
        let mut e = Scripted { replies: vec![Ok("Sure, here's the summary: it went well.".into()), Ok(reply("T", "Fixed."))], prompts: vec![], context: 16_384 };
        let out = summarize(&short(), &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(out.summary.recap, "Fixed.");
        assert!(e.prompts[1].user.contains("Your previous reply couldn't be used: the reply had no JSON object."));
    }

    #[test]
    fn two_bad_replies_give_up_with_a_reason() {
        let mut e = Scripted { replies: vec![Ok("nope".into()), Ok("{\"recap\": \"\"}".into())], prompts: vec![], context: 16_384 };
        let err = summarize(&short(), &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert_eq!(err, SummarizeError::BadOutput("the reply had no recap".into()));
        assert!(err.message().contains("couldn't use"));
    }

    #[test]
    fn engine_errors_pass_through_without_retry() {
        let mut e = Scripted { replies: vec![Err(EngineError::Unavailable("Ollama isn't running.".into()))], prompts: vec![], context: 16_384 };
        let err = summarize(&short(), &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert_eq!(err.message(), "Ollama isn't running.");
        assert_eq!(e.prompts.len(), 1);
    }

    #[test]
    fn a_long_call_is_summarized_in_parts_then_merged() {
        let lines: Vec<_> = (0..200).map(|i| line(&format!("l{i}"), i * 15_000, if i % 2 == 0 { "me" } else { "them:0" }, &"We talked about the roadmap and what ships first. ".repeat(4))).collect();
        let m = meeting(lines);
        let mut e = Scripted { replies: vec![], prompts: vec![], context: 4_096 };
        let mut seen = vec![];
        let out = summarize(&m, &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |p| seen.push(p)).unwrap();
        assert!(out.parts >= 3, "{} parts", out.parts);
        assert_eq!(e.prompts.len(), out.parts + 1);
        assert!(e.prompts[0].user.contains(&format!("This is part 1 of {}", out.parts)));
        assert!(e.prompts.last().unwrap().user.contains("Combine them into one summary"));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(*seen.last().unwrap(), 1.0);
    }

    #[test]
    fn a_recording_too_short_to_summarize_is_not_sent() {
        let mut e = Scripted { replies: vec![], prompts: vec![], context: 16_384 };
        let no = AtomicBool::new(false);
        // A cough: under a second of speech.
        let mut cough = line("l0", 0, "me", "Hm.");
        cough.end = 600;
        assert_eq!(summarize(&meeting(vec![cough]), &mut e, "en", &Options::default(), &no, &mut |_| {}), Err(SummarizeError::TooShort));
        // Two short lines.
        let two = meeting(vec![line("l0", 0, "me", "Can you hear me?"), line("l1", 5_000, "them:0", "Yes, one second.")]);
        assert_eq!(summarize(&two, &mut e, "en", &Options::default(), &no, &mut |_| {}), Err(SummarizeError::TooShort));
        assert!(e.prompts.is_empty());
        assert!(SummarizeError::TooShort.message().contains("too short"));
    }

    #[test]
    fn two_long_lines_are_still_summarized() {
        // An imported voice memo can be one long line per speaker.
        let words = "We agreed to ship onboarding first and move pricing to the first quarter. ";
        let m = meeting(vec![line("l0", 0, "them:0", &words.repeat(3)), line("l1", 30_000, "them:0", &words.repeat(2))]);
        let mut e = Scripted { replies: vec![Ok(reply("Plan", "We planned."))], prompts: vec![], context: 16_384 };
        assert!(summarize(&m, &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).is_ok());
        // Three lines is a conversation, however short.
        let three = meeting(vec![line("l0", 0, "me", "Hi."), line("l1", 5_000, "them:0", "Hi."), line("l2", 9_000, "me", "Ship it.")]);
        let mut e = Scripted { replies: vec![Ok(reply("Plan", "Short."))], prompts: vec![], context: 16_384 };
        assert!(summarize(&three, &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).is_ok());
    }

    #[test]
    fn an_empty_transcript_is_not_sent() {
        let mut e = Scripted { replies: vec![], prompts: vec![], context: 16_384 };
        assert_eq!(summarize(&meeting(vec![]), &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}), Err(SummarizeError::Empty));
        assert!(e.prompts.is_empty());
    }
}

/// A 2-hour call (the planning fixture 13 times) through the real local model: parts, merge, time.
/// Build the helper first (`scripts/build-llm-helper.sh`), then
/// `TALAAN_LLM_MODEL=<gguf> cargo test --release long_summary_smoke -- --ignored --nocapture`
#[cfg(test)]
mod long_smoke {
    use super::*;
    use crate::storage::model::TranscriptLine;
    use crate::summarizer::engines::LocalEngine;
    use crate::summarizer::local::{helper_file, LocalAi};
    use crate::summarizer::prompt::tests::meeting;

    #[test]
    #[ignore]
    fn long_summary_smoke() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/summary.planning-call.json")).unwrap();
        let one: Vec<TranscriptLine> = serde_json::from_value(fixture["transcript"].clone()).unwrap();
        let span = one.last().unwrap().end + 5_000;
        let lines: Vec<TranscriptLine> = (0..13u64)
            .flat_map(|k| one.iter().map(move |l| TranscriptLine { id: format!("r{k}{}", l.id), at: l.at + k * span, end: l.end + k * span, ..l.clone() }))
            .collect();
        let mut m = meeting(lines);
        m.speakers = serde_json::from_value(fixture["speakers"].clone()).unwrap();
        m.item.duration_ms = 13 * span;
        let exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join(helper_file());
        let model_path = std::path::PathBuf::from(std::env::var("TALAAN_LLM_MODEL").unwrap());
        let mut engine = LocalEngine { ai: LocalAi::new(&exe), model_path, model_name: "test".into() };
        let t = std::time::Instant::now();
        let out = summarize(&m, &mut engine, "en", &Options::default(), &AtomicBool::new(false), &mut |p| eprintln!("progress {:.0}% at {:.0}s", p * 100.0, t.elapsed().as_secs_f32())).unwrap();
        eprintln!("== {} min call, {} parts, {:.0}s", m.item.duration_ms / 60_000, out.parts, t.elapsed().as_secs_f32());
        eprintln!("title: {:?}\nrecap: {}", out.title, out.summary.recap);
        for d in &out.summary.decisions { eprintln!("D {:?} {}", d.at.map(|a| a / 1000), d.text); }
        for a in &out.summary.action_items { eprintln!("A {:?} {} | {:?} | {:?}", a.at.map(|a| a / 1000), a.text, a.owner, a.due); }
        for q in &out.summary.open_questions { eprintln!("Q {:?} {}", q.at.map(|a| a / 1000), q.text); }
    }
}
