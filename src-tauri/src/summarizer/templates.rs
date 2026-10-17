//! Meeting types ("templates"): General (the fixed format) plus 1:1, Stand-up, Interview, Sales or client call and
//! Lecture or webinar. A type changes what the summary looks for and adds its own sections; the summary keeps the
//! same recap, decisions, action items and open questions for every type, so every screen and export still works.

use crate::i18n::{tr_in, Lang};

pub struct Template {
    /// Stored in settings.json (`summaryTemplate`), meeting.json (`template`) and summary.json (`template`).
    pub id: &'static str,
    /// What kind of call it is and what matters in it, for the model (English, like the rest of the prompt).
    pub focus: &'static str,
    /// The i18n keys of its section titles, in order.
    pub sections: &'static [&'static str],
    /// How to fill the sections, for the model.
    pub section_rule: &'static str,
}

pub const GENERAL: &str = "general";

pub const TEMPLATES: &[Template] = &[
    Template { id: GENERAL, focus: "", sections: &[], section_rule: "" },
    Template {
        id: "one_on_one",
        focus: "a one-to-one conversation between two people (a manager and a report, a mentor, two peers): what each is working on, feedback given in either direction, concerns, and the support asked for or offered",
        sections: &["section.one_on_one.updates", "section.one_on_one.feedback", "section.one_on_one.support"],
        section_rule: "Updates: what each person is working on. Feedback: feedback given, and by whom. Support needed: help asked for or offered.",
    },
    Template {
        id: "standup",
        focus: "a team stand-up: for each person, what they did since last time, what they will do next, and what blocks them. Keep every item short",
        sections: &["section.standup.yesterday", "section.standup.today", "section.standup.blockers"],
        section_rule: "Start every item with the person's name and a colon (\"Dana: finished the export\"). One item per person per section; leave a person out of a section when they said nothing for it.",
    },
    Template {
        id: "interview",
        focus: "a job interview. Cover the candidate's background, how they answered, and what was agreed about next steps. Judge only from what was said; no opinions beyond the transcript",
        sections: &["section.interview.strengths", "section.interview.concerns", "section.interview.next_steps"],
        section_rule: "Strengths and Concerns: what the candidate said or showed, each backed by the conversation. Next steps: what happens next in the hiring process.",
    },
    Template {
        id: "sales",
        focus: "a sales or client call: the client's needs and problems, objections, budget and timeline as they stated them, what was offered, and next steps",
        sections: &["section.sales.needs", "section.sales.objections", "section.sales.next_steps"],
        section_rule: "Client needs: needs, problems, budget and timeline as the client said them. Objections: concerns or pushback. Next steps: what each side does next.",
    },
    Template {
        id: "lecture",
        focus: "a lecture, class or webinar, mostly one person teaching: the main ideas in the order they came, key terms and what they mean, and examples. Decisions are usually empty; homework and deadlines are action items",
        sections: &["section.lecture.key_points", "section.lecture.terms", "section.lecture.resources"],
        section_rule: "Key points: the main ideas in order. Terms: a term, a colon, and what it means (\"Latency: the delay before ...\"). Resources mentioned: books, links, tools or readings named.",
    },
];

/// The type with this id; General for anything unknown (a summary from a newer build, a typo in settings.json).
pub fn find(id: &str) -> &'static Template {
    TEMPLATES.iter().find(|t| t.id == id).unwrap_or(&TEMPLATES[0])
}

pub fn exists(id: &str) -> bool {
    TEMPLATES.iter().any(|t| t.id == id)
}

/// Section titles have their own words in the languages Talaan's screens speak; other summary languages get the
/// English titles and the model translates them.
fn localized(language: &str) -> Option<Lang> {
    match language.split(['-', '_']).next().unwrap_or("") {
        "en" => Some(Lang::En),
        "fr" => Some(Lang::Fr),
        "es" => Some(Lang::Es),
        _ => None,
    }
}

impl Template {
    pub fn is_general(&self) -> bool {
        self.sections.is_empty()
    }

    /// The section titles to ask for, in the summary's language when Talaan has the words for it.
    pub fn titles(&self, language: &str) -> Vec<String> {
        let lang = localized(language).unwrap_or(Lang::En);
        self.sections.iter().map(|k| tr_in(lang, k, &[])).collect()
    }

    /// Whether the titles above are already in the summary's language (else the model is asked to translate them).
    pub fn titles_localized(language: &str) -> bool {
        localized(language).is_some()
    }

    /// The title to store for a section the model returned: this type's own title in the summary's language when it
    /// names one of them in any of Talaan's languages ("Next steps" in a French summary becomes "Prochaines étapes"),
    /// else the model's title as written.
    pub fn canonical_title(&self, title: &str, language: &str) -> String {
        let t = title.trim().trim_end_matches(':').trim();
        let lower = t.to_lowercase();
        if let Some(lang) = localized(language) {
            for key in self.sections {
                if [Lang::En, Lang::Fr, Lang::Es].iter().any(|l| tr_in(*l, key, &[]).to_lowercase() == lower) {
                    return tr_in(lang, key, &[]);
                }
            }
        }
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_types_with_general_first_and_unknown_ids_falling_back() {
        let ids: Vec<_> = TEMPLATES.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec!["general", "one_on_one", "standup", "interview", "sales", "lecture"]);
        assert!(find("general").is_general());
        assert_eq!(find("nope").id, "general");
        assert!(exists("standup") && !exists("nope"));
        for t in &TEMPLATES[1..] {
            assert_eq!(t.sections.len(), 3, "{}", t.id);
            assert!(!t.focus.is_empty() && !t.section_rule.is_empty());
        }
    }

    #[test]
    fn titles_follow_the_summary_language() {
        let s = find("standup");
        assert_eq!(s.titles("en"), vec!["Yesterday", "Today", "Blockers"]);
        assert_eq!(s.titles("fr"), vec!["Hier", "Aujourd’hui", "Blocages"]);
        assert_eq!(s.titles("es-MX"), vec!["Ayer", "Hoy", "Bloqueos"]);
        // German: English titles, translated by the model.
        assert_eq!(s.titles("de"), vec!["Yesterday", "Today", "Blockers"]);
        assert!(!Template::titles_localized("de") && Template::titles_localized("fr"));
    }

    #[test]
    fn a_returned_title_maps_to_the_types_own_words() {
        let i = find("interview");
        assert_eq!(i.canonical_title("Next steps:", "fr"), "Prochaines étapes");
        assert_eq!(i.canonical_title("FORTALEZAS", "es"), "Fortalezas");
        assert_eq!(i.canonical_title("Culture fit", "en"), "Culture fit");
        // No words of our own for German: kept as the model wrote it.
        assert_eq!(i.canonical_title("Stärken", "de"), "Stärken");
    }
}
