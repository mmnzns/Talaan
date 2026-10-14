//! One meeting's summary, whatever the engine: fit the transcript to the engine's context (splitting long
//! calls), ask, parse, retry once on unusable output, merge parts, and pick a title.

use super::chunk::{estimate_tokens, split};
use super::parse::{parse_draft, to_summary, usable_title, Draft};
use super::prompt::{merge_prompt, summary_prompt, Prompt, SCHEMA};
use crate::storage::model::{Meeting, Summary};
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
    Engine(EngineError),
    /// Two replies in a row couldn't be used.
    BadOutput(String),
}

impl SummarizeError {
    pub fn message(&self) -> String {
        match self {
            SummarizeError::Empty => "There's no transcript to summarize.".into(),
            SummarizeError::Engine(EngineError::Unavailable(m)) | SummarizeError::Engine(EngineError::Failed(m)) => m.clone(),
            SummarizeError::Engine(EngineError::Cancelled) => "Cancelled.".into(),
            SummarizeError::BadOutput(m) => format!("The summary AI gave an answer Talaan couldn't use ({m})."),
        }
    }
}

/// `language`: ISO code to write in. `progress` gets 0..1.
pub fn summarize(meeting: &Meeting, engine: &mut dyn SummaryEngine, language: &str, cancel: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<Outcome, SummarizeError> {
    let lines = &meeting.transcript;
    if lines.iter().all(|l| l.text.trim().is_empty()) {
        return Err(SummarizeError::Empty);
    }
    // What the instructions and header cost, measured on an empty transcript.
    let overhead = {
        let p = summary_prompt(meeting, &[], language, Some((1, 1)));
        estimate_tokens(&p.system) + estimate_tokens(&p.user) + 64
    };
    let budget = (engine.context_tokens() * 9 / 10).saturating_sub(overhead + MAX_OUTPUT_TOKENS as usize).max(500);
    let parts = split(lines, budget);
    let n = parts.len();
    progress(0.0);

    if n == 1 {
        let draft = ask(engine, &summary_prompt(meeting, lines, language, None), cancel)?;
        progress(1.0);
        return Ok(finish(&draft, meeting, engine, 1));
    }

    let mut drafts = vec![];
    for (i, part) in parts.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(SummarizeError::Engine(EngineError::Cancelled));
        }
        drafts.push(ask(engine, &summary_prompt(meeting, part, language, Some((i + 1, n))), cancel)?);
        progress((i + 1) as f32 / (n + 1) as f32);
    }
    let parts_json: Vec<String> = drafts.iter().map(|d| serde_json::to_string(d).expect("drafts serialize")).collect();
    let merged = ask(engine, &merge_prompt(meeting, &parts_json, language), cancel)?;
    progress(1.0);
    Ok(finish(&merged, meeting, engine, n))
}

fn finish(draft: &Draft, meeting: &Meeting, engine: &dyn SummaryEngine, parts: usize) -> Outcome {
    Outcome { summary: to_summary(draft, &meeting.transcript, &engine.label()), title: usable_title(&draft.title), parts }
}

/// One request, with one retry that tells the model what was wrong.
fn ask(engine: &mut dyn SummaryEngine, prompt: &Prompt, cancel: &AtomicBool) -> Result<Draft, SummarizeError> {
    let first = engine.complete(prompt, SCHEMA, MAX_OUTPUT_TOKENS, cancel).map_err(SummarizeError::Engine)?;
    let problem = match parse_draft(&first) {
        Ok(d) => return Ok(d),
        Err(e) => e,
    };
    let retry = Prompt { system: prompt.system.clone(), user: format!("{}\n\nYour previous reply couldn't be used: {problem}. Reply with only the JSON object.", prompt.user) };
    let second = engine.complete(&retry, SCHEMA, MAX_OUTPUT_TOKENS, cancel).map_err(SummarizeError::Engine)?;
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
        meeting(vec![line("l0", 0, "me", "Hi."), line("l1", 60_000, "them:1", "Onboarding first, pricing in Q1.")])
    }

    #[test]
    fn a_short_call_takes_one_request() {
        let mut e = Scripted { replies: vec![Ok(reply("Q4 roadmap sync", "We planned Q4."))], prompts: vec![], context: 16_384 };
        let mut seen = vec![];
        let out = summarize(&short(), &mut e, "en", &AtomicBool::new(false), &mut |p| seen.push(p)).unwrap();
        assert_eq!(e.prompts.len(), 1);
        assert_eq!(out.summary.recap, "We planned Q4.");
        assert_eq!(out.summary.decisions[0].at, Some(60_000));
        assert_eq!(out.summary.engine, "Scripted");
        assert_eq!(out.title.as_deref(), Some("Q4 roadmap sync"));
        assert_eq!(seen, vec![0.0, 1.0]);
    }

    #[test]
    fn unusable_output_is_retried_once_with_the_reason() {
        let mut e = Scripted { replies: vec![Ok("Sure, here's the summary: it went well.".into()), Ok(reply("T", "Fixed."))], prompts: vec![], context: 16_384 };
        let out = summarize(&short(), &mut e, "en", &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(out.summary.recap, "Fixed.");
        assert!(e.prompts[1].user.contains("Your previous reply couldn't be used: the reply had no JSON object."));
    }

    #[test]
    fn two_bad_replies_give_up_with_a_reason() {
        let mut e = Scripted { replies: vec![Ok("nope".into()), Ok("{\"recap\": \"\"}".into())], prompts: vec![], context: 16_384 };
        let err = summarize(&short(), &mut e, "en", &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert_eq!(err, SummarizeError::BadOutput("the reply had no recap".into()));
        assert!(err.message().contains("couldn't use"));
    }

    #[test]
    fn engine_errors_pass_through_without_retry() {
        let mut e = Scripted { replies: vec![Err(EngineError::Unavailable("Ollama isn't running.".into()))], prompts: vec![], context: 16_384 };
        let err = summarize(&short(), &mut e, "en", &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert_eq!(err.message(), "Ollama isn't running.");
        assert_eq!(e.prompts.len(), 1);
    }

    #[test]
    fn a_long_call_is_summarized_in_parts_then_merged() {
        let lines: Vec<_> = (0..200).map(|i| line(&format!("l{i}"), i * 15_000, if i % 2 == 0 { "me" } else { "them:0" }, &"We talked about the roadmap and what ships first. ".repeat(4))).collect();
        let m = meeting(lines);
        let mut e = Scripted { replies: vec![], prompts: vec![], context: 4_096 };
        let mut seen = vec![];
        let out = summarize(&m, &mut e, "en", &AtomicBool::new(false), &mut |p| seen.push(p)).unwrap();
        assert!(out.parts >= 3, "{} parts", out.parts);
        assert_eq!(e.prompts.len(), out.parts + 1);
        assert!(e.prompts[0].user.contains(&format!("This is part 1 of {}", out.parts)));
        assert!(e.prompts.last().unwrap().user.contains("Combine them into one summary"));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(*seen.last().unwrap(), 1.0);
    }

    #[test]
    fn an_empty_transcript_is_not_sent() {
        let mut e = Scripted { replies: vec![], prompts: vec![], context: 16_384 };
        assert_eq!(summarize(&meeting(vec![]), &mut e, "en", &AtomicBool::new(false), &mut |_| {}), Err(SummarizeError::Empty));
        assert!(e.prompts.is_empty());
    }
}
