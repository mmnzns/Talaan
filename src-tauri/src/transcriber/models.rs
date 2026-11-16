//! Which engine catalog models Talaan shows, in Talaan's order. The catalog itself is the engine's.
use voice_engine_models::{catalog, Kind, ModelInfo};

/// Whether Talaan offers a catalog model. Multitalker (who said what over each other) is a separate, later feature:
/// the engine keeps it out of `catalog::speech()` already, and this says so explicitly by kind so no list, download
/// or setting can reach it by accident.
pub fn offered(m: &ModelInfo) -> bool {
    m.kind != Kind::Multitalker
}

/// Speech models in the order the Models screen lists them: Talaan's three tiers first (best, fast, older Macs),
/// then the rest of the engine's speech catalog in its own order.
pub const FIRST: [&str; 3] = ["whisper-turbo", "parakeet-v3", "whisper-small"];

pub fn speech() -> Vec<&'static ModelInfo> {
    let mut list: Vec<_> = FIRST.iter().filter_map(|id| catalog::find(id)).collect();
    list.extend(catalog::speech().filter(|m| offered(m) && !FIRST.contains(&m.id)));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multitalker_is_in_the_catalog_but_never_offered() {
        let multitalker: Vec<_> = catalog::all().iter().filter(|m| m.kind == Kind::Multitalker).collect();
        assert!(!multitalker.is_empty(), "the catalog has no Multitalker entry any more: drop this test and the filter");
        assert!(multitalker.iter().all(|m| !offered(m)));
        assert!(speech().iter().all(|m| m.kind != Kind::Multitalker));
        // Nor as a local AI model, or a model the commands will download or delete by id.
        assert!(catalog::local_ai().all(|m| offered(m)));
        assert!(crate::commands::transcriber::find_any("multitalker-tdt").is_none());
    }

    #[test]
    fn the_first_tiers_are_in_the_catalog_and_listed_first_without_repeats() {
        let list = speech();
        assert_eq!(list.len(), catalog::speech().count());
        for (i, id) in FIRST.iter().enumerate() {
            assert_eq!(list[i].id, *id);
        }
        let mut ids: Vec<_> = list.iter().map(|m| m.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), list.len());
    }

    /// The browser preview lists the same speech models in the same order as the app.
    #[test]
    fn the_preview_lists_the_same_speech_models_in_the_same_order() {
        let ts = include_str!("../../../src/data/defaultSettings.ts");
        let block = &ts[ts.find("speechModels").expect("speechModels in defaultSettings.ts")..];
        let block = &block[..block.find("localModels").unwrap_or(block.len())];
        let mut ids = vec![];
        let mut rest = block;
        while let Some(at) = rest.find("id: \"") {
            rest = &rest[at + 5..];
            ids.push(rest[..rest.find('"').unwrap()].to_string());
        }
        // The preview may also list the two add-on models (speaker separation, search) after the speech ones.
        ids.retain(|id| id != "diarization" && id != "search");
        let want: Vec<_> = speech().iter().map(|m| m.id.to_string()).collect();
        assert_eq!(ids, want);
    }
}
