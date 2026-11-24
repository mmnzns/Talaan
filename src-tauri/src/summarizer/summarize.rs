//! One meeting's summary, whatever the engine: fit the transcript to the engine's context (splitting long
//! calls), ask, parse, retry once on unusable output, merge parts, and pick a title.

use super::chunk::{estimate_tokens, split};
use super::parse::{parse_draft, to_highlights, to_sections, to_summary, usable_title, Draft};
use super::prompt::{is_detailed, merge_prompt, moments, schema, summary_prompt, Options, Prompt, SCHEMA};
use crate::storage::model::{Meeting, Summary, TranscriptLine};
use std::sync::atomic::{AtomicBool, Ordering};

/// Room left for the reply of the plain summary (the base shape, and the retry after a reply that was cut off).
pub const MAX_OUTPUT_TOKENS: u32 = 1_500;

/// Room for the detailed General summary (purpose, takeaways, topics with bullets, decisions, next steps).
pub const DETAILED_OUTPUT_TOKENS: u32 = 4_500;

/// Room for the compact detailed summary a small context asks for (at most 3 topics of 3 bullets).
pub const COMPACT_OUTPUT_TOKENS: u32 = 1_600;

/// Contexts under this (the 4096 of an 8 GB computer) get the compact layout, so the reply is never cut off.
pub const COMPACT_BELOW_CONTEXT: usize = 12_288;

/// Room for the reply: the detailed layout (or the plain base), a meeting type's sections (up to 3 × 12 short items)
/// and one highlight per marked moment, so a long call isn't cut off mid-JSON.
pub fn output_tokens(detailed: bool, compact: bool, sections: bool, highlights: usize) -> u32 {
    let base = match (detailed, compact) {
        (true, true) => COMPACT_OUTPUT_TOKENS,
        (true, false) => DETAILED_OUTPUT_TOKENS,
        _ => MAX_OUTPUT_TOKENS,
    };
    let extra = if sections { 900 } else { 0 } + 60 * highlights.min(30) as u32;
    (base + extra).min(if detailed { 6_000 } else { 4_000 })
}

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
    // A small context gets the compact layout: fewer topics and bullets, so the reply fits with room to spare.
    let compact = engine.context_tokens() < COMPACT_BELOW_CONTEXT;
    let opts = &Options { compact, ..opts.clone() };
    // What the instructions and header cost, measured on an empty transcript.
    let overhead = {
        let p = summary_prompt(meeting, &[], language, Some((1, 1)), opts);
        estimate_tokens(&p.system) + estimate_tokens(&p.user) + 64
    };
    let detailed = is_detailed(opts);
    let sections = !detailed;
    let marked = moments(meeting).len();
    // The reply never takes more than 40% of the context, so a small context (4096 on an 8 GB computer) still leaves
    // room for the instructions and some transcript; a reply that gets cut off is retried as a plain summary.
    let cap = (engine.context_tokens() * 4 / 10).min(u32::MAX as usize) as u32;
    let out = Reply { schema: schema(detailed, compact, sections, marked), tokens: output_tokens(detailed, compact, sections, marked).min(cap), plain_tokens: MAX_OUTPUT_TOKENS.min(cap), plain: AtomicBool::new(false) };
    let budget = (engine.context_tokens() * 9 / 10).saturating_sub(overhead + out.tokens as usize).max(500);
    let parts = split(lines, budget);
    let n = parts.len();
    let schema = &out;
    progress(0.0);

    if n == 1 {
        let draft = ask(engine, &summary_prompt(meeting, lines, language, None, opts), schema, cancel)?;
        progress(1.0);
        return Ok(finish(&draft, meeting, engine, 1, opts, language, &out));
    }

    let mut drafts = vec![];
    for (i, part) in parts.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(SummarizeError::Engine(EngineError::Cancelled));
        }
        drafts.push(ask(engine, &summary_prompt(meeting, part, language, Some((i + 1, n)), opts), schema, cancel)?);
        progress((i + 1) as f32 / (n + 1) as f32);
    }
    let parts_json: Vec<String> = drafts.iter().map(|d| serde_json::to_string(d).expect("drafts serialize")).collect();
    let merged = merge(meeting, parts_json, engine, language, opts, &out, cancel)?;
    progress(1.0);
    Ok(finish(&merged, meeting, engine, n, opts, language, &out))
}

/// Merges the part summaries into one. When they all fit one request that is one call; when they don't (a long
/// call on a small context), they are merged in groups that do fit, and the group summaries merged again, until
/// one is left.
fn merge(meeting: &Meeting, mut level: Vec<String>, engine: &mut dyn SummaryEngine, language: &str, opts: &Options, reply: &Reply, cancel: &AtomicBool) -> Result<Draft, SummarizeError> {
    let empty = merge_prompt(meeting, &[], language, opts);
    let overhead = estimate_tokens(&empty.system) + estimate_tokens(&empty.user) + 64;
    let room = (engine.context_tokens() * 9 / 10).saturating_sub(overhead + reply.tokens as usize).max(500);
    let cost = |json: &String| estimate_tokens(json) + 12;
    loop {
        if level.iter().map(cost).sum::<usize>() <= room || level.len() <= 2 {
            return ask(engine, &merge_prompt(meeting, &level, language, opts), reply, cancel);
        }
        // Consecutive groups of at least two summaries (a lone one waits for the next round) that fit one request.
        let mut next = vec![];
        let mut i = 0;
        while i < level.len() {
            let mut end = i + 1;
            let mut used = cost(&level[i]);
            while end < level.len() && (end < i + 2 || used + cost(&level[end]) <= room) {
                used += cost(&level[end]);
                end += 1;
            }
            if end - i < 2 {
                next.push(level[i].clone());
            } else {
                if cancel.load(Ordering::Relaxed) {
                    return Err(SummarizeError::Engine(EngineError::Cancelled));
                }
                let d = ask(engine, &merge_prompt(meeting, &level[i..end], language, opts), reply, cancel)?;
                next.push(serde_json::to_string(&d).expect("drafts serialize"));
            }
            i = end;
        }
        level = next;
    }
}

/// Too little to summarize: under 1 s of speech in all, or at most two lines with fewer than 12 words between
/// them. A model asked to summarize "Can you hear me?" invents a recap and a title.
pub fn too_short(lines: &[TranscriptLine]) -> bool {
    let spoken: Vec<&TranscriptLine> = lines.iter().filter(|l| !l.text.trim().is_empty()).collect();
    let speech_ms: u64 = spoken.iter().map(|l| l.end.saturating_sub(l.at)).sum();
    let words: usize = spoken.iter().map(|l| l.text.split_whitespace().count()).sum();
    speech_ms < 1_000 || (spoken.len() <= 2 && words < 12)
}

fn finish(draft: &Draft, meeting: &Meeting, engine: &dyn SummaryEngine, parts: usize, opts: &Options, language: &str, reply: &Reply) -> Outcome {
    let mut summary = to_summary(draft, &meeting.transcript, &engine.label());
    let template = opts.template();
    summary.sections = to_sections(draft, &meeting.transcript, template, language);
    summary.highlights = to_highlights(draft, meeting);
    summary.template = Some(template.id.to_string());
    summary.language = Some(language.to_string());
    summary.sections_missing = !template.is_general() && summary.sections.is_empty() && reply.plain.load(Ordering::Relaxed);
    Outcome { summary, title: usable_title(&draft.title), parts }
}

/// The shape and room asked for in every request of one summary.
struct Reply {
    schema: String,
    tokens: u32,
    /// Room for the reply of the plain retry (`MAX_OUTPUT_TOKENS`, held to what the context allows).
    plain_tokens: u32,
    /// Set when a retry had to drop sections and highlights.
    plain: AtomicBool,
}

/// One request, with one retry that tells the model what was wrong. When the first reply asked for the detailed layout,
/// sections or highlights and came back unusable (most often cut off), the retry asks for the base summary only, so the user
/// still gets one: missing highlights are then filled in from the transcript (`to_highlights`).
fn ask(engine: &mut dyn SummaryEngine, prompt: &Prompt, reply: &Reply, cancel: &AtomicBool) -> Result<Draft, SummarizeError> {
    let first = engine.complete(prompt, &reply.schema, reply.tokens, cancel).map_err(SummarizeError::Engine)?;
    let problem = match parse_draft(&first) {
        Ok(d) => return Ok(d),
        Err(e) => e,
    };
    if reply.schema != SCHEMA {
        reply.plain.store(true, Ordering::Relaxed);
    }
    let extended = reply.schema != SCHEMA;
    let (schema, ask_less) = if extended {
        (SCHEMA, " Use the plain shape this time: title, recap (3 to 5 sentences with the purpose and the main points), decisions, action_items and open_questions, and leave out topics, takeaways, sections and highlights.")
    } else {
        (SCHEMA, "")
    };
    let retry = Prompt { system: prompt.system.clone(), user: format!("{}\n\nYour previous reply couldn't be used: {problem}. Reply with only the JSON object.{ask_less}", prompt.user) };
    let second = engine.complete(&retry, schema, reply.plain_tokens, cancel).map_err(SummarizeError::Engine)?;
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
        let opts = Options { template: "interview".into(), ..Default::default() };
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
        // General is the detailed layout now (the full one on a roomy context).
        assert_eq!(e.0[0], schema(true, false, false, 0));
        assert!(out.summary.sections.is_empty() && out.summary.highlights.is_empty());
        assert_eq!(out.summary.template.as_deref(), Some("general"));
    }

    #[test]
    fn a_new_summary_carries_the_language_it_was_asked_for() {
        let mut e = Scripted { replies: vec![Ok(reply("Plan", "Ang buod."))], prompts: vec![], context: 16_384 };
        let out = summarize(&short(), &mut e, "fil", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(out.summary.language.as_deref(), Some("fil"));
        assert!(e.prompts[0].system.contains("Write everything in Filipino."));
    }

    #[test]
    fn a_cut_off_reply_with_sections_is_retried_as_a_plain_summary() {
        /// Keeps the schema and room of each request.
        struct Cut(Vec<(String, u32)>);
        impl SummaryEngine for Cut {
            fn complete(&mut self, _p: &Prompt, schema: &str, max: u32, _c: &AtomicBool) -> Result<String, EngineError> {
                self.0.push((schema.to_string(), max));
                Ok(if self.0.len() == 1 { r#"{"title":"Stand-up","recap":"Round of updates.","sections":[{"title":"Yesterday","items":[{"text":"Dana: fixed"#.into() } else { reply("Stand-up", "Round of updates.") })
            }
            fn context_tokens(&self) -> usize {
                16_384
            }
            fn label(&self) -> String {
                "Cut".into()
            }
        }
        let mut m = short();
        m.marks = (0..5).map(|i| crate::summarizer::prompt::tests::mark(&format!("k{i}"), 61_000 + i * 1_000, None)).collect();
        let mut e = Cut(vec![]);
        let opts = Options { template: "standup".into(), ..Default::default() };
        let out = summarize(&m, &mut e, "en", &opts, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(e.0.len(), 2);
        assert_eq!(e.0[0].1, output_tokens(false, false, true, 5));
        assert!(e.0[0].1 > MAX_OUTPUT_TOKENS);
        assert_eq!(e.0[1], (SCHEMA.to_string(), MAX_OUTPUT_TOKENS));
        assert_eq!(out.summary.recap, "Round of updates.");
        // The marked moments still get highlights, from the transcript, and the card can say the sections are missing.
        assert_eq!(out.summary.highlights.len(), 5);
        assert!(out.summary.sections_missing);
    }

    #[test]
    fn the_reply_room_grows_with_sections_and_marks_and_stops_at_a_cap() {
        assert_eq!(output_tokens(false, false, false, 0), MAX_OUTPUT_TOKENS);
        assert!(output_tokens(false, false, true, 0) > MAX_OUTPUT_TOKENS);
        assert!(output_tokens(false, false, false, 10) > output_tokens(false, false, false, 1));
        assert_eq!(output_tokens(false, false, true, 500), 4_000);
        // The detailed layout has more room than the plain one, the compact one less.
        assert_eq!(output_tokens(true, false, false, 0), DETAILED_OUTPUT_TOKENS);
        assert_eq!(output_tokens(true, true, false, 0), COMPACT_OUTPUT_TOKENS);
        assert!(COMPACT_OUTPUT_TOKENS < DETAILED_OUTPUT_TOKENS && DETAILED_OUTPUT_TOKENS > MAX_OUTPUT_TOKENS);
        assert_eq!(output_tokens(true, false, true, 500), 6_000);
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


    /// A reply in the detailed shape: no recap, the purpose, takeaways and topics.
    pub fn detailed_reply() -> String {
        r#"{"title":"Pilot plan","purpose":"To plan the pilot of the scheduling tool.",
            "takeaways":[{"label":"Pilot scope","text":"Two teams start in March."},{"label":"Budget","text":"The licence fits the budget."}],
            "topics":[{"title":"Pilot plan","bullets":[{"text":"Two teams, about 25 people.","sub_bullets":["Support first","Sales in week 3"]},{"text":"Review after six weeks.","sub_bullets":[]}]}],
            "decisions":[{"text":"Start in March.","time":"01:00"}],
            "action_items":[{"text":"Send the plan.","owner":"Dana Reyes","due":"Friday","time":"02:05"},{"text":"Book the room.","owner":"","due":"","time":""}],
            "open_questions":[]}"#
            .into()
    }

    #[test]
    fn a_detailed_reply_becomes_purpose_takeaways_topics_and_a_plain_recap() {
        let mut e = Scripted { replies: vec![Ok(detailed_reply())], prompts: vec![], context: 16_384 };
        let out = summarize(&short(), &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        let s = &out.summary;
        assert_eq!(s.purpose.as_deref(), Some("To plan the pilot of the scheduling tool."));
        assert_eq!(s.takeaways.len(), 2);
        assert_eq!(s.topics[0].bullets[0].sub_bullets, vec!["Support first", "Sales in week 3"]);
        assert!(s.is_detailed());
        // Everything that reads the recap keeps working: the purpose and the takeaways as sentences.
        assert_eq!(s.recap, "To plan the pilot of the scheduling tool. Pilot scope: Two teams start in March. Budget: The licence fits the budget.");
        assert_eq!(s.action_items[0].owner.as_deref(), Some("Dana Reyes"));
        assert_eq!(out.title.as_deref(), Some("Pilot plan"));
        assert_eq!(s.decisions[0].at, Some(60_000));
    }

    #[test]
    fn a_reply_in_the_older_shape_still_makes_a_summary_that_shows_as_before() {
        let mut e = Scripted { replies: vec![Ok(reply("Plan", "We planned."))], prompts: vec![], context: 16_384 };
        let out = summarize(&short(), &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(out.summary.recap, "We planned.");
        assert!(!out.summary.is_detailed() && out.summary.topics.is_empty() && out.summary.purpose.is_none());
    }

    /// Keeps what each request asked for and answers with a script.
    struct Asks {
        sent: Vec<(String, u32)>,
        replies: Vec<String>,
        context: usize,
    }

    impl SummaryEngine for Asks {
        fn complete(&mut self, _p: &Prompt, schema: &str, max: u32, _c: &AtomicBool) -> Result<String, EngineError> {
            self.sent.push((schema.to_string(), max));
            Ok(if self.replies.is_empty() { detailed_reply() } else { self.replies.remove(0) })
        }
        fn context_tokens(&self) -> usize {
            self.context
        }
        fn label(&self) -> String {
            "Asks".into()
        }
    }

    #[test]
    fn a_small_context_asks_for_the_compact_layout_and_a_reply_that_fits() {
        let mut e = Asks { sent: vec![], replies: vec![], context: 4_096 };
        summarize(&short(), &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(e.sent[0].0, schema(true, true, false, 0));
        assert!(e.sent[0].1 <= 1_638 && e.sent[0].1 == COMPACT_OUTPUT_TOKENS, "{}", e.sent[0].1);
        // A roomy context gets the full one.
        let mut big = Asks { sent: vec![], replies: vec![], context: 16_384 };
        summarize(&short(), &mut big, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(big.sent[0], (schema(true, false, false, 0), DETAILED_OUTPUT_TOKENS));
    }

    #[test]
    fn a_compact_summary_of_a_long_call_fits_every_request_of_a_4096_context() {
        let m = long_call(200);
        let mut e = Strict { context: 4_096, requests: vec![], recap: "Planned.".into() };
        let out = summarize(&m, &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert!(out.parts >= 2);
        assert!(e.requests.iter().all(|(p, max)| p + *max as usize <= 4_096 && *max <= 1_638), "{:?}", e.requests);
        // Even the longest reply the compact schema allows is about what the room covers.
        assert!(estimate_tokens(&schema(true, true, false, 0)) < 900);
    }

    #[test]
    fn a_cut_off_detailed_reply_falls_back_to_the_plain_summary() {
        let cut = r#"{"title":"Pilot","purpose":"To plan.","takeaways":[{"label":"Scope","text":"Two te"#.to_string();
        let mut e = Asks { sent: vec![], replies: vec![cut, reply("Pilot", "We planned the pilot.")], context: 16_384 };
        let out = summarize(&short(), &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(e.sent.len(), 2);
        assert_eq!(e.sent[1], (SCHEMA.to_string(), MAX_OUTPUT_TOKENS));
        assert_eq!(out.summary.recap, "We planned the pilot.");
        assert!(!out.summary.is_detailed());
        // A General meeting has no sections to be missing.
        assert!(!out.summary.sections_missing);
    }

    #[test]
    fn the_parts_of_a_long_call_and_their_merge_use_the_detailed_layout() {
        let m = long_call(200);
        let mut e = Asks { sent: vec![], replies: vec![], context: 4_096 };
        let out = summarize(&m, &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert!(out.parts >= 3);
        assert!(e.sent.iter().all(|(s, _)| *s == schema(true, true, false, 0)));
        assert!(out.summary.is_detailed());
    }

    /// Refuses any request that would not fit its context (prompt + room for the reply), like the real model does.
    struct Strict {
        context: usize,
        requests: Vec<(usize, u32)>,
        recap: String,
    }

    impl SummaryEngine for Strict {
        fn complete(&mut self, p: &Prompt, _s: &str, max_tokens: u32, _c: &AtomicBool) -> Result<String, EngineError> {
            let prompt = estimate_tokens(&p.system) + estimate_tokens(&p.user);
            self.requests.push((prompt, max_tokens));
            if prompt + max_tokens as usize > self.context {
                return Err(EngineError::Failed(format!("{prompt} + {max_tokens} does not fit {}", self.context)));
            }
            Ok(reply("Roadmap", &self.recap))
        }
        fn context_tokens(&self) -> usize {
            self.context
        }
        fn label(&self) -> String {
            "Strict".into()
        }
    }

    fn long_call(lines: u64) -> Meeting {
        let lines: Vec<_> = (0..lines).map(|i| line(&format!("l{i}"), i * 15_000, if i % 2 == 0 { "me" } else { "them:0" }, &"We talked about the roadmap and what ships first. ".repeat(4))).collect();
        meeting(lines)
    }

    #[test]
    fn a_long_call_on_a_4096_context_is_split_into_requests_that_each_fit() {
        let m = long_call(200);
        let mut e = Strict { context: 4_096, requests: vec![], recap: "Planned.".into() };
        let out = summarize(&m, &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert!(out.parts >= 5, "{} parts", out.parts);
        assert_eq!(e.requests.len(), out.parts + 1);
        // The part budget is the context minus the instructions and the reply room, so no part runs over.
        assert!(e.requests.iter().all(|(p, max)| p + *max as usize <= 4_096), "{:?}", e.requests);
    }

    #[test]
    fn meeting_types_and_many_marked_moments_still_fit_a_4096_context() {
        let mut m = long_call(120);
        m.marks = (0..40).map(|i| crate::summarizer::prompt::tests::mark(&format!("k{i}"), 61_000 + i * 20_000, None)).collect();
        let mut e = Strict { context: 4_096, requests: vec![], recap: "Planned.".into() };
        let opts = Options { template: "standup".into(), ..Default::default() };
        summarize(&m, &mut e, "en", &opts, &AtomicBool::new(false), &mut |_| {}).unwrap();
        // The reply room is held to 40% of the context (the sections schema would ask for 4000).
        assert!(e.requests.iter().all(|(p, max)| p + *max as usize <= 4_096 && *max <= 1_638), "{:?}", e.requests);
    }

    #[test]
    fn the_part_summaries_of_a_very_long_call_are_merged_in_rounds_when_they_do_not_fit_one_request() {
        let m = long_call(600);
        // Each part summary is about 300 tokens, so dozens of them can't go into one 4096 request.
        let mut e = Strict { context: 4_096, requests: vec![], recap: "We planned the roadmap and what ships first. ".repeat(25) };
        let out = summarize(&m, &mut e, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert!(out.parts > 10, "{} parts", out.parts);
        assert!(e.requests.len() > out.parts + 1, "{} requests for {} parts", e.requests.len(), out.parts);
        assert!(e.requests.iter().all(|(p, max)| p + *max as usize <= 4_096), "{:?}", e.requests);
        assert_eq!(out.summary.recap, e.recap.trim());
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
        let model_path = std::path::PathBuf::from(std::env::var("TALAAN_LLM_MODEL").unwrap());
        let tmp = tempfile::tempdir().unwrap();
        let client = std::sync::Arc::new(crate::summarizer::local::client(tmp.path(), "test", "summary"));
        let mut engine = LocalEngine::new(client, model_path, "test".into(), true, 16_384);
        let t = std::time::Instant::now();
        let out = summarize(&m, &mut engine, "en", &Options::default(), &AtomicBool::new(false), &mut |p| eprintln!("progress {:.0}% at {:.0}s", p * 100.0, t.elapsed().as_secs_f32())).unwrap();
        eprintln!("== {} min call, {} parts, {:.0}s", m.item.duration_ms / 60_000, out.parts, t.elapsed().as_secs_f32());
        eprintln!("title: {:?}\nrecap: {}", out.title, out.summary.recap);
        for d in &out.summary.decisions { eprintln!("D {:?} {}", d.at.map(|a| a / 1000), d.text); }
        for a in &out.summary.action_items { eprintln!("A {:?} {} | {:?} | {:?}", a.at.map(|a| a / 1000), a.text, a.owner, a.due); }
        for q in &out.summary.open_questions { eprintln!("Q {:?} {}", q.at.map(|a| a / 1000), q.text); }
    }
    /// A short invented Spanish call summarized in Spanish by a real local model, to see whether it writes Spanish (Granite
    /// 4.2 3B used to write English). `TALAAN_SMOKE_FIRM=1` turns the firm-language prompt on. Build the helper first, then
    /// `TALAAN_LLM_MODEL=<gguf> [TALAAN_SMOKE_FIRM=1] cargo test --release spanish_summary_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn spanish_summary_smoke() {
        let say = |id: &str, at: u64, who: &str, text: &str| TranscriptLine { id: id.into(), at, end: at + 6_000, speaker: who.into(), text: text.into(), edited: false };
        let lines = vec![
            say("l0", 0, "me", "Buenos días, Carla. Quería repasar el calendario del lanzamiento de la aplicación."),
            say("l1", 7_000, "them:0", "Claro. El equipo de diseño termina las pantallas el jueves, y las pruebas empiezan el lunes."),
            say("l2", 16_000, "me", "Perfecto. Entonces decidimos lanzar el 12 de diciembre, siempre que las pruebas salgan bien."),
            say("l3", 26_000, "them:0", "De acuerdo. Yo enviaré la lista de errores pendientes el viernes."),
            say("l4", 34_000, "me", "Yo hablaré con soporte para preparar las respuestas a los clientes. Todavía no sabemos quién pagará la licencia del servidor."),
        ];
        let mut m = meeting(lines);
        m.marks.clear();
        m.speakers.clear();
        m.item.duration_ms = 45_000;
        let model_path = std::path::PathBuf::from(std::env::var("TALAAN_LLM_MODEL").unwrap());
        let tmp = tempfile::tempdir().unwrap();
        let client = std::sync::Arc::new(crate::summarizer::local::client(tmp.path(), "test", "summary"));
        let mut engine = LocalEngine::new(client, model_path, "test".into(), true, 16_384);
        let opts = Options { firm_language: std::env::var("TALAAN_SMOKE_FIRM").is_ok_and(|v| v == "1"), ..Default::default() };
        let out = summarize(&m, &mut engine, "es", &opts, &AtomicBool::new(false), &mut |_| {}).unwrap();
        eprintln!("{}", crate::storage::markdown::summary_md(&out.title.clone().unwrap_or_default(), &out.summary, &[], None));
    }


    /// One real detailed summary of the invented 9-minute planning call, to read what the model writes. The names of the
    /// two other speakers are removed (they come up in the conversation), and Your name is empty unless
    /// `TALAAN_SMOKE_ME` is set. Build the helper first, then
    /// `TALAAN_LLM_MODEL=<gguf> TALAAN_SMOKE_OUT=<file.md> cargo test --release detailed_summary_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn detailed_summary_smoke() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../fixtures/summary.planning-call.json")).unwrap();
        let lines: Vec<TranscriptLine> = serde_json::from_value(fixture["transcript"].clone()).unwrap();
        let mut m = meeting(lines);
        m.marks.clear();
        m.speakers.clear();
        if let Ok(me) = std::env::var("TALAAN_SMOKE_ME") {
            m.speakers.insert("me".into(), me);
        }
        m.item.duration_ms = fixture["durationMs"].as_u64().unwrap();
        let context: usize = std::env::var("TALAAN_SMOKE_CONTEXT").ok().and_then(|c| c.parse().ok()).unwrap_or(16_384);
        let model_path = std::path::PathBuf::from(std::env::var("TALAAN_LLM_MODEL").unwrap());
        let tmp = tempfile::tempdir().unwrap();
        let client = std::sync::Arc::new(crate::summarizer::local::client(tmp.path(), "test", "summary"));
        let mut engine = LocalEngine::new(client, model_path, "test".into(), true, context);
        let t = std::time::Instant::now();
        let out = summarize(&m, &mut engine, "en", &Options::default(), &AtomicBool::new(false), &mut |_| {}).unwrap();
        eprintln!("== {} parts, {:.0}s, context {context}", out.parts, t.elapsed().as_secs_f32());
        let title = out.title.clone().unwrap_or_default();
        let md = crate::storage::markdown::summary_md(&title, &out.summary, &[], None);
        eprintln!("{md}");
        if let Ok(path) = std::env::var("TALAAN_SMOKE_OUT") {
            std::fs::write(path, md).unwrap();
        }
    }
}
