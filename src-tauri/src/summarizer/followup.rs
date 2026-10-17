//! "Draft follow-up email": one short request over the summary (not the transcript, so it's quick even on the
//! processor), giving a plain-text email to the people on the call: thanks, decisions, action items with owners and
//! dates. Works with every summary engine.

use super::parse::{extract_json, strip_reasoning};
use super::prompt::{language_name, speaker_name, Prompt};
use super::summarize::{EngineError, SummaryEngine};
use crate::eta::Profile;
use crate::i18n::{tr_in, Lang};
use crate::storage::model::{Meeting, Summary};
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;

pub const SCHEMA: &str = r#"{"type":"object","properties":{"subject":{"type":"string"},"body":{"type":"string"}},"required":["subject","body"],"additionalProperties":false}"#;

/// Room for the reply: an email of about 200 words.
pub const MAX_OUTPUT_TOKENS: u32 = 700;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FollowUp {
    pub subject: String,
    pub body: String,
}

/// The list headings in the summary's language, in the words the summary card uses (Décisions, Tâches, Tareas
/// pendientes); other languages get English ones to translate.
fn headings(language: &str) -> (String, String, bool) {
    let lang = match language.split(['-', '_']).next().unwrap_or("") {
        "fr" => Some(Lang::Fr),
        "es" => Some(Lang::Es),
        "en" => Some(Lang::En),
        _ => None,
    };
    let l = lang.unwrap_or(Lang::En);
    (tr_in(l, "followup.decisions", &[]), tr_in(l, "followup.action_items", &[]), lang.is_some())
}

fn render_summary(s: &Summary) -> String {
    let mut out = format!("Recap: {}\n", s.recap.trim());
    if !s.decisions.is_empty() {
        out.push_str("Decisions:\n");
        for d in &s.decisions {
            out.push_str(&format!("- {}\n", d.text.trim()));
        }
    }
    if !s.action_items.is_empty() {
        out.push_str("Action items:\n");
        for a in &s.action_items {
            let owner = a.owner.as_deref().map(|o| o.trim()).filter(|o| !o.is_empty()).map(|o| format!(" | owner: {o}")).unwrap_or_default();
            let due = a.due.as_deref().map(|d| d.trim()).filter(|d| !d.is_empty()).map(|d| format!(" | due: {d}")).unwrap_or_default();
            out.push_str(&format!("- {}{owner}{due}\n", a.text.trim()));
        }
    }
    if !s.open_questions.is_empty() {
        out.push_str("Open questions:\n");
        for q in &s.open_questions {
            out.push_str(&format!("- {}\n", q.text.trim()));
        }
    }
    for sec in &s.sections {
        out.push_str(&format!("{}:\n", sec.title.trim()));
        for i in &sec.items {
            out.push_str(&format!("- {}\n", i.text.trim()));
        }
    }
    out
}

pub fn prompt(meeting: &Meeting, summary: &Summary, language: &str) -> Prompt {
    let me = speaker_name(meeting, "me");
    let (decisions, actions, localized) = headings(language);
    let translate = if localized { "" } else { " (translated)" };
    let system = format!(
        "You write a short follow-up email after a meeting, from the user (\"{me}\") to the other people who were on it. Reply with JSON only: subject and body.

- subject: under 10 words, naming the meeting.
- body: plain text, no Markdown, no placeholders like [Name]. A one-line thank-you; the decisions under the heading \"{decisions}:\"{translate}; the action items under \"{actions}:\"{translate}, each with its owner and due date when the summary gives them; then a one-line closing{sign}.
- Leave out a heading whose list would be empty. Lists use \"- \" at the start of each line.
- Use only what is in the summary. Never invent decisions, owners, dates, numbers or names.
- Under 200 words.
- Write everything in {lang}.",
        sign = if me != "Me" { format!(" signed \"{me}\"") } else { String::new() },
        lang = language_name(language),
    );
    let mut user = format!("Meeting: {}, {}.\n", meeting.item.title.trim(), meeting.item.started_at.get(..10).unwrap_or(""));
    if let Some(ev) = meeting.calendar_event.as_ref().filter(|e| !e.attendees.is_empty()) {
        user.push_str(&format!("Invited: {}.\n", ev.attendees.join(", ")));
    }
    user.push_str("\nSummary:\n");
    user.push_str(&render_summary(summary));
    Prompt { system, user }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FollowUpError {
    Engine(EngineError),
    BadOutput,
}

fn parse(reply: &str) -> Option<FollowUp> {
    let text = strip_reasoning(reply);
    let f: FollowUp = serde_json::from_str(extract_json(&text)?).ok()?;
    let body = f.body.trim().replace("\r\n", "\n");
    (!body.is_empty()).then(|| FollowUp { subject: f.subject.split_whitespace().collect::<Vec<_>>().join(" "), body })
}

/// One request, one retry when the reply can't be used.
pub fn draft(meeting: &Meeting, summary: &Summary, engine: &mut dyn SummaryEngine, language: &str, cancel: &AtomicBool) -> Result<FollowUp, FollowUpError> {
    let p = prompt(meeting, summary, language);
    let first = engine.complete(&p, SCHEMA, MAX_OUTPUT_TOKENS, cancel).map_err(FollowUpError::Engine)?;
    if let Some(f) = parse(&first) {
        return Ok(f);
    }
    let retry = Prompt { system: p.system.clone(), user: format!("{}\n\nYour previous reply couldn't be used. Reply with only the JSON object.", p.user) };
    let second = engine.complete(&retry, SCHEMA, MAX_OUTPUT_TOKENS, cancel).map_err(FollowUpError::Engine)?;
    parse(&second).ok_or(FollowUpError::BadOutput)
}

/// How long a draft should take, in seconds: the engine's start-up plus its pace on a summary-sized input (the
/// summary's words read at speaking pace) and the email it writes.
pub fn estimate(profile: &Profile, measured: Option<f64>, summary: &Summary) -> f64 {
    let words = render_words(summary);
    profile.total(measured, words as f64 / 2.5 + 120.0)
}

fn render_words(s: &Summary) -> usize {
    let count = |t: &str| t.split_whitespace().count();
    count(&s.recap)
        + s.decisions.iter().map(|d| count(&d.text)).sum::<usize>()
        + s.action_items.iter().map(|a| count(&a.text) + 4).sum::<usize>()
        + s.open_questions.iter().map(|q| count(&q.text)).sum::<usize>()
        + s.sections.iter().flat_map(|x| x.items.iter()).map(|i| count(&i.text)).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summarizer::prompt::tests::meeting;
    use crate::summarizer::summarize::tests::Scripted;

    fn summary() -> Summary {
        let mut s = crate::storage::store::tests::summary("We chose to ship onboarding first.");
        s.action_items[0].due = Some("Friday".into());
        s
    }

    #[test]
    fn the_prompt_uses_the_summary_not_the_transcript() {
        let mut m = meeting(vec![crate::summarizer::prompt::tests::line("l0", 0, "me", "SECRET TRANSCRIPT LINE")]);
        m.item.title = "Q4 roadmap".into();
        m.calendar_event = Some(crate::storage::model::CalendarEvent { title: "Q4".into(), attendees: vec!["Dana Reyes".into()], emails: vec!["dana@example.com".into()] });
        let p = prompt(&m, &summary(), "en");
        assert!(p.user.contains("Meeting: Q4 roadmap, 2026-10-14."));
        assert!(p.user.contains("Invited: Dana Reyes."));
        assert!(p.user.contains("- Send wireframes | owner: Me | due: Friday"));
        assert!(!p.user.contains("SECRET"));
        assert!(p.system.contains("\"Decisions:\"") && p.system.contains("Never invent"));
    }

    #[test]
    fn headings_match_the_words_on_screen() {
        let m = meeting(vec![]);
        let fr = prompt(&m, &summary(), "fr").system;
        assert!(fr.contains("\"Décisions:\"") && fr.contains("\"Tâches:\"") && fr.contains("Write everything in French."));
        let es = prompt(&m, &summary(), "es").system;
        assert!(es.contains("\"Tareas pendientes:\""));
        assert!(prompt(&m, &summary(), "de").system.contains("\"Decisions:\" (translated)"));
        let mut named = meeting(vec![]);
        named.speakers.insert("me".into(), "Miguel".into());
        assert!(prompt(&named, &summary(), "en").system.contains("signed \"Miguel\""));
    }

    #[test]
    fn a_bad_reply_is_retried_once() {
        let good = r#"{"subject":"Thanks  for today","body":"Hi all,\n\nThanks for the call.\n\nAction items:\n- Send wireframes (Me, Friday)"}"#;
        let mut e = Scripted { replies: vec![Ok("Sure! Here is an email.".into()), Ok(good.into())], prompts: vec![], context: 16_384 };
        let f = draft(&meeting(vec![]), &summary(), &mut e, "en", &AtomicBool::new(false)).unwrap();
        assert_eq!(f.subject, "Thanks for today");
        assert!(f.body.starts_with("Hi all,"));
        assert_eq!(e.prompts.len(), 2);
        let mut bad = Scripted { replies: vec![Ok("no".into()), Ok("{\"subject\":\"x\",\"body\":\" \"}".into())], prompts: vec![], context: 16_384 };
        assert_eq!(draft(&meeting(vec![]), &summary(), &mut bad, "en", &AtomicBool::new(false)), Err(FollowUpError::BadOutput));
    }

    #[test]
    fn a_draft_is_much_quicker_than_a_summary() {
        let cpu = crate::eta::summary_profile("local", "qwen3.5-4b", false);
        let secs = estimate(&cpu, None, &summary());
        assert!((30.0..120.0).contains(&secs), "{secs}");
        let cloud = crate::eta::summary_profile("byok", "claude", false);
        assert!(estimate(&cloud, None, &summary()) < 15.0);
    }
}
