//! Named speakers: splitting the "Them" track into Speaker 1, 2, 3 with NVIDIA's Nemotron 3 Diarization
//! (a streaming Sortformer, up to 8 speakers, OpenMDW-1.1), through parakeet-rs.
//!
//! A streaming model needs to hear a voice before it can tell it apart, so a speaker's first turns tend to be
//! absorbed by whoever it already knows. Feeding it the start of the call first (and throwing those
//! predictions away) fills its speaker memory; on a three-voice test that took the result from 8/12 to 12/12
//! turns right. Each transcript sentence then gets the speaker most active over its span.

use super::merge::{Timed, Track};
use std::path::Path;

pub const RATE: usize = 16_000;
/// Warm-up: the first 10 minutes (or the whole call when shorter).
const WARMUP: usize = 10 * 60 * RATE;
/// Audio per inference call, so memory stays flat on long calls. A multiple of the 160-sample hop.
const CHUNK: usize = 5 * 60 * RATE;
/// A sentence needs at least this mean activity on its best speaker to be assigned.
const MIN_ACTIVITY: f32 = 0.2;

/// Per-10 ms-frame speaker activity, frames × speakers, values 0..1.
pub type Activity = Vec<Vec<f32>>;

pub trait SpeakerSplitter: Send {
    fn activity(&mut self, them: &[f32]) -> Result<Activity, String>;
}

pub struct Diarizer {
    model: parakeet_rs::sortformer::Sortformer,
}

impl Diarizer {
    pub fn load(path: &Path) -> Result<Self, String> {
        let model = parakeet_rs::sortformer::Sortformer::new(path).map_err(|e| format!("couldn't load speaker separation: {e}"))?;
        Ok(Diarizer { model })
    }
}

impl SpeakerSplitter for Diarizer {
    fn activity(&mut self, them: &[f32]) -> Result<Activity, String> {
        let err = |e: parakeet_rs::Error| format!("speaker separation failed: {e}");
        self.model.reset_state();
        if them.is_empty() {
            return Ok(vec![]);
        }
        let warm = &them[..them.len().min(WARMUP)];
        self.model.diarize_chunk_raw(warm).map_err(err)?;
        let mut rows: Activity = Vec::with_capacity(them.len() / 160 + 1);
        for chunk in them.chunks(CHUNK) {
            let p = self.model.diarize_chunk_raw(chunk).map_err(err)?;
            for r in 0..p.num_valid_frames {
                rows.push(p.predictions.row(r).to_vec());
            }
        }
        Ok(rows)
    }
}

/// The most active speaker over `[start_ms, end_ms)`, or None when nobody clearly is.
fn dominant(activity: &Activity, start_ms: u64, end_ms: u64) -> Option<usize> {
    let (a, b) = ((start_ms / 10) as usize, ((end_ms / 10) as usize).min(activity.len()));
    if b <= a {
        return None;
    }
    let speakers = activity[a].len();
    let mut sums = vec![0f32; speakers];
    for row in &activity[a..b] {
        for (k, v) in row.iter().enumerate() {
            sums[k] += v;
        }
    }
    let (best, sum) = sums.iter().enumerate().max_by(|x, y| x.1.total_cmp(y.1))?;
    (*sum / (b - a) as f32 >= MIN_ACTIVITY).then_some(best)
}

/// Gives each Them sentence a speaker number. Numbers are 1, 2, 3 in order of first appearance; a sentence
/// nobody is clearly speaking takes the previous speaker. With fewer than two speakers everything stays
/// plain "Them" (0), since "Speaker 1" alone says nothing.
pub fn assign_speakers(pieces: &mut [Timed], activity: &Activity) -> usize {
    let mut order: Vec<usize> = vec![];
    let mut raw: Vec<Option<usize>> = vec![];
    let mut idx: Vec<usize> = (0..pieces.len()).filter(|&i| pieces[i].track == Track::Them).collect();
    idx.sort_by_key(|&i| pieces[i].start_ms);
    for &i in &idx {
        let d = dominant(activity, pieces[i].start_ms, pieces[i].end_ms);
        if let Some(s) = d {
            if !order.contains(&s) {
                order.push(s);
            }
        }
        raw.push(d);
    }
    if order.len() < 2 {
        return order.len();
    }
    let mut last = 1;
    for (&i, d) in idx.iter().zip(raw) {
        let n = d.map(|s| order.iter().position(|o| *o == s).unwrap() + 1).unwrap_or(last);
        pieces[i].speaker = n as u8;
        last = n;
    }
    order.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn them(s: f32, e: f32) -> Timed {
        Timed { track: Track::Them, speaker: 0, start_ms: (s * 1000.0) as u64, end_ms: (e * 1000.0) as u64, text: "x".into() }
    }

    /// Activity with `who[i]` speaking second i (raw model ids), 8 speaker columns.
    fn activity(who: &[Option<usize>]) -> Activity {
        who.iter().flat_map(|w| (0..100).map(move |_| (0..8).map(|k| if Some(k) == *w { 0.9 } else { 0.01 }).collect())).collect()
    }

    #[test]
    fn speakers_are_numbered_by_first_appearance() {
        // Raw model ids 3, 0, 5 → Speaker 1, 2, 3.
        let act = activity(&[Some(3), Some(3), Some(0), Some(0), Some(5), Some(3)]);
        let mut p = vec![them(0.0, 2.0), them(2.0, 4.0), them(4.0, 5.0), them(5.0, 6.0)];
        assert_eq!(assign_speakers(&mut p, &act), 3);
        assert_eq!(p.iter().map(|x| x.speaker).collect::<Vec<_>>(), vec![1, 2, 3, 1]);
    }

    #[test]
    fn one_voice_stays_them() {
        let act = activity(&[Some(2), Some(2), Some(2)]);
        let mut p = vec![them(0.0, 1.0), them(1.0, 3.0)];
        assert_eq!(assign_speakers(&mut p, &act), 1);
        assert!(p.iter().all(|x| x.speaker == 0));
    }

    #[test]
    fn unclear_sentences_follow_the_previous_speaker() {
        let act = activity(&[Some(1), None, Some(4), Some(4)]);
        let mut p = vec![them(0.0, 1.0), them(1.0, 2.0), them(2.0, 4.0)];
        assign_speakers(&mut p, &act);
        assert_eq!(p.iter().map(|x| x.speaker).collect::<Vec<_>>(), vec![1, 1, 2]);
    }

    #[test]
    fn me_lines_are_left_alone() {
        let act = activity(&[Some(1), Some(2)]);
        let mut p = vec![them(0.0, 1.0), Timed { track: Track::Me, ..them(0.5, 1.5) }, them(1.0, 2.0)];
        assign_speakers(&mut p, &act);
        assert_eq!(p[1].speaker, 0);
        assert_eq!((p[0].speaker, p[2].speaker), (1, 2));
    }

    /// Real model on the three-voice fixture (Karen / Daniel / Rishi, 12 turns).
    /// `cargo test --release diarize_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn diarize_smoke() {
        let model = dirs::data_dir().unwrap().join("Talaan/models/diarization/nemotron3_diar_v3.onnx");
        // Three synthetic voices (Karen, Daniel, Rishi), 12 turns; truth.json has who spoke when.
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/three-speakers/");
        let audio = crate::transcriber::vad::read_track(Path::new(&format!("{dir}them.wav"))).unwrap();
        let truth: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(format!("{dir}truth.json")).unwrap()).unwrap();
        let mut pieces: Vec<Timed> = truth.iter().map(|t| Timed { track: Track::Them, speaker: 0, start_ms: t["start_ms"].as_u64().unwrap(), end_ms: t["end_ms"].as_u64().unwrap(), text: t["speaker"].as_str().unwrap().into() }).collect();
        let t = std::time::Instant::now();
        let act = Diarizer::load(&model).unwrap().activity(&audio).unwrap();
        let n = assign_speakers(&mut pieces, &act);
        eprintln!("{n} speakers in {:.1}s: {}", t.elapsed().as_secs_f32(), pieces.iter().map(|p| format!("{}={}", &p.text[..1], p.speaker)).collect::<Vec<_>>().join(" "));
        assert_eq!(n, 3);
    }
}
