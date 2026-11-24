//! Importing an audio file (a Voice Memo, a recording from another app): decode whatever it is, mix to mono,
//! resample to 16 kHz and write the "Them" track, streaming so a long file never sits in memory.

use crate::i18n::{tr, tr_err};
use crate::recorder::resample::{to_i16, Resampler16k};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// File extensions the import dialog offers.
pub const EXTENSIONS: &[&str] = &["m4a", "mp4", "aac", "mp3", "wav", "aif", "aiff", "caf", "flac", "ogg", "oga", "mkv"];

/// A file that stopped early or went bad partway: what was kept, and how much the file should have held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CutOff {
    /// Audio kept, in ms.
    pub read_ms: u64,
    /// What the file's header promised, in ms, when it says.
    pub expected_ms: Option<u64>,
}

/// What an import produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Imported {
    pub duration_ms: u64,
    pub cut_off: Option<CutOff>,
}

/// A file that is removed when this goes out of scope (failure, cancel, panic), unless `keep` was called.
pub struct TempFile(Option<PathBuf>);

impl TempFile {
    pub fn new(path: &Path) -> Self {
        TempFile(Some(path.to_path_buf()))
    }
    /// The file was moved to its place: leave it alone.
    pub fn keep(&mut self) {
        self.0 = None;
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_file(p);
        }
    }
}

const TEMP_PREFIX: &str = "import-";

/// Temp file name for one import in `root`.
pub fn temp_name(root: &Path, seq: u64) -> PathBuf {
    root.join(format!("{TEMP_PREFIX}{}-{seq}.wav", std::process::id()))
}

/// Exactly the names `temp_name` makes (`import-<pid>-<seq>.wav`) and its in-progress copy (`….wav.tmp`).
fn is_temp_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(TEMP_PREFIX) else { return false };
    let Some(stem) = rest.strip_suffix(".wav.tmp").or_else(|| rest.strip_suffix(".wav")) else { return false };
    let mut parts = stem.split('-');
    let digits = |p: Option<&str>| p.is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
    digits(parts.next()) && digits(parts.next()) && parts.next().is_none()
}

/// Removes import temp files left by a crash or a force quit. Run at launch, when no import can be running.
pub fn clean_stale(root: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else { return 0 };
    let mut removed = 0;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if is_temp_name(&name) && e.path().is_file() && std::fs::remove_file(e.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Runs `f`, turning a panic into an error message so a bug in a decoder never leaves the screen waiting.
pub fn guard_panic<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => {
            let detail = payload.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| payload.downcast_ref::<String>().cloned()).unwrap_or_default();
            log::error!("import panicked: {}", crate::logging::text(&detail));
            Err(tr("import.crashed"))
        }
    }
}

/// The file held fewer frames than its header promised: by more than 2% and by more than half a second, so codec
/// priming and padding never flag a normal short file. A streamed WAV (data size 0xFFFFFFFF) has no real length.
fn is_short(expected: u64, got: u64, rate: u64) -> bool {
    let shortfall = expected.saturating_sub(got);
    shortfall * 100 > expected * 2 && shortfall * 1000 > rate * 500
}

/// A WAV written as a stream carries 0xFFFFFFFF for its data size, which reads as a length of about 4 GB. When the
/// file is far smaller than that, the header gave no real length, so there is nothing to compare against.
fn is_streamed_length(frames: u64, bytes_per_frame: u64, file_len: u64) -> bool {
    let claimed = frames.saturating_mul(bytes_per_frame);
    claimed >= 0xFFFF_0000 && file_len < claimed / 2
}

/// Decodes `src` into a 16 kHz mono 16-bit WAV at `out`. If the file ends early or goes bad partway, what was
/// decoded is kept and `cut_off` says so. Cancelling returns `Err("cancelled")` and leaves no file.
pub fn import_to_wav(src: &Path, out: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<Imported, String> {
    let unreadable = |e: &dyn std::fmt::Display| tr_err("import.unreadable", e);
    let file = File::open(src).map_err(|e| tr_err("import.open_failed", e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = src.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).map_err(|e| unreadable(&e))?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or_else(|| tr("import.no_audio"))?;
    let track_id = track.id;
    let params = &track.codec_params;
    let file_len = std::fs::metadata(src).map(|m| m.len()).unwrap_or(u64::MAX);
    let bytes_per_frame = params.channels.map_or(1, |c| c.count().max(1) as u64) * u64::from(params.bits_per_sample.unwrap_or(16) / 8).max(1);
    let total_frames = params.n_frames.filter(|t| *t > 0 && !is_streamed_length(*t, bytes_per_frame, file_len));
    let source_rate = track.codec_params.sample_rate;
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| unreadable(&e))?;

    let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let tmp = out.with_extension("wav.tmp");
    let mut tmp_guard = TempFile::new(&tmp);
    let mut writer = hound::WavWriter::create(&tmp, spec).map_err(|e| tr_err("import.write_failed", e))?;
    let mut resampler: Option<Resampler16k> = None;
    let mut buf: Option<SampleBuffer<f32>> = None;
    let (mut frames_in, mut written) = (0u64, 0u64);
    let mut broke = false;
    let mut decoded_rate = 0u32;
    let mut write = |samples: Vec<f32>, w: &mut hound::WavWriter<_>| -> Result<(), String> {
        written += samples.len() as u64;
        for s in to_i16(&samples) {
            w.write_sample(s).map_err(|e| tr_err("import.write_failed", e))?;
        }
        Ok(())
    };

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            // Went bad partway: keep what we have, and say so.
            Err(_) if frames_in > 0 => {
                broke = true;
                break;
            }
            Err(e) => return Err(unreadable(&e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged packet: skip it, keep the rest of the file.
            Err(SymError::DecodeError(_)) => continue,
            Err(_) if frames_in > 0 => {
                broke = true;
                break;
            }
            Err(e) => return Err(unreadable(&e)),
        };
        let s = *decoded.spec();
        decoded_rate = s.rate;
        let channels = s.channels.count().max(1);
        let sb = buf.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, s));
        if sb.capacity() < decoded.capacity() * channels {
            *sb = SampleBuffer::new(decoded.capacity() as u64, s);
        }
        sb.copy_interleaved_ref(decoded);
        frames_in += (sb.samples().len() / channels) as u64;
        let r = resampler.get_or_insert_with(|| Resampler16k::new(s.rate, channels));
        let out_samples = r.push(sb.samples());
        write(out_samples, &mut writer)?;
        if let Some(total) = total_frames {
            progress((frames_in as f64 / total as f64).min(1.0) as f32);
        }
    }
    if let Some(mut r) = resampler {
        write(r.finish(), &mut writer)?;
    }
    writer.finalize().map_err(|e| tr_err("import.write_failed", e))?;
    if written == 0 {
        return Err(tr("import.no_audio"));
    }
    let rate = source_rate.filter(|r| *r > 0).unwrap_or(decoded_rate).max(1) as u64;
    let short = total_frames.is_some_and(|t| is_short(t, frames_in, rate));
    std::fs::rename(&tmp, out).map_err(|e| tr_err("import.write_failed", e))?;
    tmp_guard.keep();
    progress(1.0);
    let duration_ms = written * 1000 / 16_000;
    let cut_off = (broke || short).then(|| CutOff {
        read_ms: duration_ms,
        expected_ms: total_frames.zip(source_rate).filter(|(_, r)| *r > 0).map(|(t, r)| t * 1000 / r as u64),
    });
    Ok(Imported { duration_ms, cut_off })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone_wav(path: &Path, rate: u32, channels: u16, secs: f32) {
        let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..(rate as f32 * secs) as usize {
            let v = ((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 12_000.0) as i16;
            for _ in 0..channels {
                w.write_sample(v).unwrap();
            }
        }
        w.finalize().unwrap();
    }

    #[test]
    fn a_44k_stereo_file_becomes_16k_mono_of_the_same_length() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("memo.wav");
        tone_wav(&src, 44_100, 2, 3.0);
        let out = d.path().join("them.wav");
        let mut last = 0.0;
        let ms = import_to_wav(&src, &out, &AtomicBool::new(false), &mut |p| last = p).unwrap().duration_ms;
        assert!((2_950..=3_050).contains(&ms), "{ms}");
        let r = hound::WavReader::open(&out).unwrap();
        assert_eq!((r.spec().sample_rate, r.spec().channels), (16_000, 1));
        assert_eq!(last, 1.0);
        let peak = r.into_samples::<i16>().map(|s| s.unwrap().unsigned_abs()).max().unwrap();
        assert!(peak > 8_000, "the tone survived: {peak}");
    }

    #[test]
    fn something_that_isnt_audio_says_so() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("notes.m4a");
        std::fs::write(&src, b"this is not audio").unwrap();
        let e = import_to_wav(&src, &d.path().join("them.wav"), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert!(e.starts_with("Talaan can't read this file"), "{e}");
        assert!(!d.path().join("them.wav").exists());
    }

    /// A real M4A (AAC) made by macOS: `afconvert -f m4af -d aac` (skipped where afconvert doesn't exist).
    #[test]
    fn an_m4a_from_voice_memos_imports() {
        let d = tempfile::tempdir().unwrap();
        let wav = d.path().join("src.wav");
        tone_wav(&wav, 48_000, 1, 2.0);
        let m4a = d.path().join("memo.m4a");
        let ok = std::process::Command::new("afconvert").args(["-f", "m4af", "-d", "aac"]).arg(&wav).arg(&m4a).status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            return;
        }
        let ms = import_to_wav(&m4a, &d.path().join("them.wav"), &AtomicBool::new(false), &mut |_| {}).unwrap().duration_ms;
        assert!((1_900..=2_200).contains(&ms), "{ms}");
    }

    #[test]
    fn a_cut_off_wav_keeps_what_was_read_and_says_so() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("full.wav");
        tone_wav(&src, 16_000, 1, 4.0);
        let bytes = std::fs::read(&src).unwrap();
        let cut = d.path().join("cut.wav");
        std::fs::write(&cut, &bytes[..44 + 16_000 * 2]).unwrap(); // the header promises 4 s, 1 s is there
        let out = d.path().join("them.wav");
        let r = import_to_wav(&cut, &out, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let c = r.cut_off.expect("flagged");
        assert!((900..=1_100).contains(&c.read_ms), "{c:?}");
        assert_eq!(c.read_ms, r.duration_ms);
        assert!(c.expected_ms.is_some_and(|e| (3_900..=4_100).contains(&e)), "{c:?}");
        assert!(out.exists());
        assert!(!d.path().join("them.wav.tmp").exists());
    }

    #[test]
    fn a_complete_file_is_not_flagged() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("ok.wav");
        tone_wav(&src, 16_000, 1, 2.0);
        let r = import_to_wav(&src, &d.path().join("them.wav"), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(r.cut_off, None);
    }

    /// A 16 kHz mono WAV whose header says `claimed_data` bytes of data, with `secs` of tone actually there.
    fn wav_with_claimed_len(path: &Path, secs: f32, claimed_data: u32) {
        tone_wav(path, 16_000, 1, secs);
        let mut b = std::fs::read(path).unwrap();
        b[40..44].copy_from_slice(&claimed_data.to_le_bytes());
        b[4..8].copy_from_slice(&claimed_data.saturating_add(36).to_le_bytes());
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn a_file_short_by_50_ms_is_not_flagged() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("s.wav");
        wav_with_claimed_len(&src, 0.95, 16_000 * 2); // promises 1 s, 0.95 s there
        let r = import_to_wav(&src, &d.path().join("them.wav"), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(r.cut_off, None);
        assert!(!is_short(48_000, 47_000, 48_000), "21 ms short of 1 s");
        assert!(!is_short(10 * 16_000, 10 * 16_000 - 4_000, 16_000), "2.5% but only 250 ms");
    }

    #[test]
    fn a_file_short_by_several_seconds_is_flagged() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("s.wav");
        wav_with_claimed_len(&src, 2.0, 10 * 16_000 * 2); // promises 10 s, 2 s there
        let r = import_to_wav(&src, &d.path().join("them.wav"), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert!(r.cut_off.is_some());
        assert!(is_short(100 * 48_000, 90 * 48_000, 48_000));
    }

    #[test]
    fn a_streamed_wav_with_no_real_length_is_not_flagged() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("s.wav");
        wav_with_claimed_len(&src, 2.0, u32::MAX);
        let r = import_to_wav(&src, &d.path().join("them.wav"), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(r.cut_off, None, "{r:?}");
    }

    #[test]
    fn only_talaans_own_temp_names_match() {
        for ok in ["import-1-0.wav", "import-123-45.wav.tmp"] {
            assert!(is_temp_name(ok), "{ok}");
        }
        for no in ["import-.wav", "import-1.wav", "import-1-2-3.wav", "import-a-1.wav", "import-1-2.wav.bak", "import-1-2.mp3", "my-import-1-2.wav", "import-1-2.wav.tmp.old"] {
            assert!(!is_temp_name(no), "{no}");
        }
    }

    #[test]
    fn cancelling_leaves_no_files_behind() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("memo.wav");
        tone_wav(&src, 16_000, 1, 2.0);
        let work = d.path().join("work");
        std::fs::create_dir(&work).unwrap();
        let e = import_to_wav(&src, &work.join("x.wav"), &AtomicBool::new(true), &mut |_| {}).unwrap_err();
        assert_eq!(e, "cancelled");
        assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
    }

    #[test]
    fn a_cancel_partway_leaves_no_files_behind() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("memo.wav");
        tone_wav(&src, 16_000, 1, 10.0);
        let work = d.path().join("work");
        std::fs::create_dir(&work).unwrap();
        let cancel = AtomicBool::new(false);
        let e = import_to_wav(&src, &work.join("x.wav"), &cancel, &mut |p| {
            if p > 0.2 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err();
        assert_eq!(e, "cancelled");
        assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
    }

    #[test]
    fn a_failure_leaves_no_temp_file() {
        let d = tempfile::tempdir().unwrap();
        let work = d.path().join("work");
        std::fs::create_dir(&work).unwrap();
        let src = d.path().join("notes.m4a");
        std::fs::write(&src, b"this is not audio").unwrap();
        let _ = import_to_wav(&src, &work.join("them.wav"), &AtomicBool::new(false), &mut |_| {});
        assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
    }

    #[test]
    fn a_panic_becomes_an_error_and_the_temp_file_goes() {
        let d = tempfile::tempdir().unwrap();
        let tmp = temp_name(d.path(), 0);
        std::fs::write(&tmp, b"x").unwrap();
        let r: Result<(), String> = guard_panic(|| {
            let _t = TempFile::new(&tmp);
            panic!("decoder bug");
        });
        assert_eq!(r.unwrap_err(), tr("import.crashed"));
        assert!(!tmp.exists());
    }

    #[test]
    fn stale_temp_files_are_cleaned_at_launch_and_nothing_else() {
        let d = tempfile::tempdir().unwrap();
        for n in ["import-9-0.wav", "import-9-1.wav.tmp", "import-notes.wav", "settings.json", "library.json"] {
            std::fs::write(d.path().join(n), b"x").unwrap();
        }
        std::fs::create_dir(d.path().join("meetings")).unwrap();
        assert_eq!(clean_stale(d.path()), 2);
        assert!(d.path().join("import-notes.wav").exists());
        assert!(d.path().join("settings.json").exists() && d.path().join("library.json").exists() && d.path().join("meetings").exists());
        assert!(!d.path().join("import-9-0.wav").exists());
    }
}
