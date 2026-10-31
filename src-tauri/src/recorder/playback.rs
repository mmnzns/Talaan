//! One file to listen to: both tracks mixed into `playback.wav` (16 kHz mono), made the first time someone
//! presses Play. Streamed in chunks, so a 2-hour call takes constant memory.

use crate::storage::paths::{ME_WAV, PLAYBACK_WAV, THEM_WAV};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const CHUNK: usize = 16_000;

fn modified(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// One lock per meeting folder, so two Plays at once mix it once: the second waits, then finds the mix fresh.
fn folder_lock(dir: &Path) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    let mut locks = LOCKS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    locks.retain(|_, l| Arc::strong_count(l) > 1); // folders nobody is mixing
    locks.entry(dir.to_path_buf()).or_default().clone()
}

/// The playback file for a meeting folder, made (or remade, if a track is newer) when needed.
/// None when there is no audio.
pub fn ensure(dir: &Path) -> std::io::Result<Option<PathBuf>> {
    let tracks: Vec<PathBuf> = [ME_WAV, THEM_WAV].iter().map(|n| dir.join(n)).filter(|p| p.exists()).collect();
    if tracks.is_empty() {
        return Ok(None);
    }
    let lock = folder_lock(dir);
    let _mixing = lock.lock().unwrap_or_else(|e| e.into_inner());
    let out = dir.join(PLAYBACK_WAV);
    let fresh = modified(&out).is_some_and(|t| tracks.iter().all(|tr| modified(tr).is_some_and(|m| m <= t)));
    if !fresh {
        mix(&tracks, &out)?;
    }
    Ok(Some(out))
}

/// Sums the tracks sample by sample (the shorter ones end in silence) with soft clipping, so two people
/// talking over each other never crackle.
/// The mix is written to a temp file of its own (named per process and call, so mixes running at once never
/// share one) and renamed into place when complete; a failed mix removes its temp file.
pub fn mix(tracks: &[PathBuf], out: &Path) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let tmp = out.with_extension(format!("wav.{}-{n}.tmp", std::process::id()));
    let made = write_mix(tracks, &tmp).and_then(|()| std::fs::rename(&tmp, out));
    if made.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    made
}

fn write_mix(tracks: &[PathBuf], tmp: &Path) -> std::io::Result<()> {
    let to_io = |e: hound::Error| std::io::Error::other(e.to_string());
    let mut readers = tracks.iter().map(|t| hound::WavReader::open(t).map_err(to_io)).collect::<Result<Vec<_>, _>>()?;
    let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(tmp, spec).map_err(to_io)?;
    let mut iters: Vec<_> = readers.iter_mut().map(|r| r.samples::<i16>()).collect();
    let mut buf = vec![0f32; CHUNK];
    loop {
        buf.iter_mut().for_each(|x| *x = 0.0);
        let mut longest = 0;
        for it in iters.iter_mut() {
            let mut n = 0;
            for slot in buf.iter_mut() {
                match it.next() {
                    Some(Ok(s)) => {
                        *slot += s as f32 / 32768.0;
                        n += 1;
                    }
                    _ => break,
                }
            }
            longest = longest.max(n);
        }
        if longest == 0 {
            break;
        }
        for x in &buf[..longest] {
            // tanh-style soft clip above ±0.8 keeps loud overlaps from wrapping.
            let y = if x.abs() <= 0.8 { *x } else { x.signum() * (0.8 + 0.2 * ((x.abs() - 0.8) / 0.2).tanh()) };
            w.write_sample((y * 32767.0) as i16).map_err(to_io)?;
        }
    }
    w.finalize().map_err(to_io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(path: &Path, samples: &[i16]) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 16_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for s in samples {
            w.write_sample(*s).unwrap();
        }
        w.finalize().unwrap();
    }

    fn read(path: &Path) -> Vec<i16> {
        hound::WavReader::open(path).unwrap().samples::<i16>().map(|s| s.unwrap()).collect()
    }

    #[test]
    fn tracks_are_summed_and_the_longer_one_sets_the_length() {
        let d = tempfile::tempdir().unwrap();
        wav(&d.path().join(ME_WAV), &[1000; 20_000]);
        wav(&d.path().join(THEM_WAV), &[2000; 35_000]);
        let out = ensure(d.path()).unwrap().unwrap();
        let s = read(&out);
        assert_eq!(s.len(), 35_000);
        assert!((s[0] - 3000).abs() <= 2);
        assert!((s[30_000] - 2000).abs() <= 2);
    }

    #[test]
    fn loud_overlap_is_soft_clipped_not_wrapped() {
        let d = tempfile::tempdir().unwrap();
        wav(&d.path().join(ME_WAV), &[30_000; 100]);
        wav(&d.path().join(THEM_WAV), &[30_000; 100]);
        let s = read(&ensure(d.path()).unwrap().unwrap());
        // Wrapped samples would come out negative.
        assert!(s.iter().all(|x| *x > 26_000), "{:?}", &s[..3]);
    }

    #[test]
    fn one_track_is_enough_and_none_means_no_playback() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(ensure(d.path()).unwrap(), None);
        wav(&d.path().join(THEM_WAV), &[5; 1600]);
        assert_eq!(read(&ensure(d.path()).unwrap().unwrap()).len(), 1600);
    }

    /// Play pressed twice (or from two windows): both get the mix, and no temp file is left behind.
    #[test]
    fn plays_at_the_same_time_never_break_each_other() {
        let d = tempfile::tempdir().unwrap();
        wav(&d.path().join(ME_WAV), &[1000; 40_000]);
        wav(&d.path().join(THEM_WAV), &[2000; 40_000]);
        let runs: Vec<_> = (0..6)
            .map(|_| {
                let dir = d.path().to_path_buf();
                std::thread::spawn(move || ensure(&dir))
            })
            .collect();
        for r in runs {
            let out = r.join().unwrap().unwrap().unwrap();
            assert_eq!(read(&out).len(), 40_000);
        }
        let left: Vec<_> = std::fs::read_dir(d.path()).unwrap().map(|e| e.unwrap().file_name()).filter(|n| n.to_string_lossy().ends_with(".tmp")).collect();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn mixes_running_at_once_use_their_own_temp_files() {
        let d = tempfile::tempdir().unwrap();
        wav(&d.path().join(ME_WAV), &[1000; 40_000]);
        let tracks = vec![d.path().join(ME_WAV)];
        let runs: Vec<_> = (0..4)
            .map(|_| {
                let (tracks, out) = (tracks.clone(), d.path().join(PLAYBACK_WAV));
                std::thread::spawn(move || mix(&tracks, &out).map(|()| out))
            })
            .collect();
        for r in runs {
            // Renaming over the finished file can race on Windows; what matters is that no mix corrupts another.
            if let Ok(out) = r.join().unwrap() {
                assert_eq!(read(&out).len(), 40_000);
            }
        }
        assert_eq!(read(&d.path().join(PLAYBACK_WAV)).len(), 40_000);
    }

    #[test]
    fn the_mix_is_made_again_when_a_track_changes() {
        let d = tempfile::tempdir().unwrap();
        wav(&d.path().join(THEM_WAV), &[5; 1600]);
        ensure(d.path()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        wav(&d.path().join(THEM_WAV), &[5; 3200]);
        assert_eq!(read(&ensure(d.path()).unwrap().unwrap()).len(), 3200);
    }
}
