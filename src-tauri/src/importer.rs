//! Importing an audio file (a Voice Memo, a recording from another app): decode whatever it is, mix to mono,
//! resample to 16 kHz and write the "Them" track, streaming so a long file never sits in memory.

use crate::i18n::{tr, tr_err};
use crate::recorder::resample::{to_i16, Resampler16k};
use std::fs::File;
use std::path::Path;
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

/// Decodes `src` into a 16 kHz mono 16-bit WAV at `out`. Returns the duration in ms.
pub fn import_to_wav(src: &Path, out: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<u64, String> {
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
    let total_frames = track.codec_params.n_frames;
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| unreadable(&e))?;

    let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let tmp = out.with_extension("wav.tmp");
    let mut writer = hound::WavWriter::create(&tmp, spec).map_err(|e| tr_err("import.write_failed", e))?;
    let mut resampler: Option<Resampler16k> = None;
    let mut buf: Option<SampleBuffer<f32>> = None;
    let (mut frames_in, mut written) = (0u64, 0u64);
    let mut write = |samples: Vec<f32>, w: &mut hound::WavWriter<_>| -> Result<(), String> {
        written += samples.len() as u64;
        for s in to_i16(&samples) {
            w.write_sample(s).map_err(|e| tr_err("import.write_failed", e))?;
        }
        Ok(())
    };

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&tmp);
            return Err("cancelled".into());
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(unreadable(&e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged packet: skip it, keep the rest of the file.
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(unreadable(&e)),
        };
        let s = *decoded.spec();
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
        if let Some(total) = total_frames.filter(|t| *t > 0) {
            progress((frames_in as f64 / total as f64).min(1.0) as f32);
        }
    }
    if let Some(mut r) = resampler {
        write(r.finish(), &mut writer)?;
    }
    writer.finalize().map_err(|e| tr_err("import.write_failed", e))?;
    if written == 0 {
        let _ = std::fs::remove_file(&tmp);
        return Err(tr("import.no_audio"));
    }
    std::fs::rename(&tmp, out).map_err(|e| tr_err("import.write_failed", e))?;
    progress(1.0);
    Ok(written * 1000 / 16_000)
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
        let ms = import_to_wav(&src, &out, &AtomicBool::new(false), &mut |p| last = p).unwrap();
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
        let ms = import_to_wav(&m4a, &d.path().join("them.wav"), &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert!((1_900..=2_200).contains(&ms), "{ms}");
    }
}
