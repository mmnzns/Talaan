//! 16 kHz mono 16-bit WAV files that stay valid while recording, plus header repair after a crash.

use super::clock::RATE;
use super::timeline::SampleSink;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// The header is rewritten this often, so a crash loses at most this much audio.
const FLUSH_EVERY: usize = RATE as usize * 2;

pub struct WavSink {
    writer: Option<hound::WavWriter<BufWriter<File>>>,
    since_flush: usize,
}

fn spec() -> hound::WavSpec {
    hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }
}

fn to_io(e: hound::Error) -> io::Error {
    match e {
        hound::Error::IoError(e) => e,
        other => io::Error::other(other.to_string()),
    }
}

impl WavSink {
    pub fn create(path: &Path) -> io::Result<Self> {
        Ok(WavSink { writer: Some(hound::WavWriter::create(path, spec()).map_err(to_io)?), since_flush: 0 })
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.since_flush = 0;
        match &mut self.writer {
            Some(w) => w.flush().map_err(to_io),
            None => Ok(()),
        }
    }

    pub fn finalize(mut self) -> io::Result<()> {
        match self.writer.take() {
            Some(w) => w.finalize().map_err(to_io),
            None => Ok(()),
        }
    }
}

impl SampleSink for WavSink {
    fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        let w = self.writer.as_mut().ok_or_else(|| io::Error::other("closed"))?;
        let mut i16w = w.get_i16_writer(samples.len() as u32);
        for s in samples {
            i16w.write_sample(*s);
        }
        i16w.flush().map_err(to_io)?;
        self.since_flush += samples.len();
        if self.since_flush >= FLUSH_EVERY {
            self.flush()?;
        }
        Ok(())
    }
}

/// Fixes the RIFF and data sizes from the real file length (after a crash) and returns the number of samples.
pub fn repair_wav(path: &Path) -> io::Result<u64> {
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let len = f.metadata()?.len();
    let mut head = [0u8; 12];
    f.read_exact(&mut head)?;
    if &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not a WAV file"));
    }
    let mut pos = 12u64;
    loop {
        if pos + 8 > len {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no data chunk"));
        }
        f.seek(SeekFrom::Start(pos))?;
        let mut chunk = [0u8; 8];
        f.read_exact(&mut chunk)?;
        let size = u32::from_le_bytes(chunk[4..8].try_into().unwrap()) as u64;
        if &chunk[0..4] == b"data" {
            let data_start = pos + 8;
            let data_len = (len - data_start) & !1;
            f.set_len(data_start + data_len)?;
            f.seek(SeekFrom::Start(pos + 4))?;
            f.write_all(&(data_len as u32).to_le_bytes())?;
            f.seek(SeekFrom::Start(4))?;
            f.write_all(&((data_start + data_len - 8) as u32).to_le_bytes())?;
            f.sync_all()?;
            return Ok(data_len / 2);
        }
        pos += 8 + size + (size & 1);
    }
}

/// Duration of a 16 kHz mono 16-bit file from its header, in ms.
pub fn wav_duration_ms(path: &Path) -> Option<u64> {
    let r = hound::WavReader::open(path).ok()?;
    let spec = r.spec();
    Some(r.duration() as u64 * 1000 / spec.sample_rate as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_readable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("me.wav");
        let mut s = WavSink::create(&path).unwrap();
        s.write(&vec![1000i16; 48_000]).unwrap();
        s.finalize().unwrap();
        assert_eq!(wav_duration_ms(&path), Some(3_000));
    }

    #[test]
    fn the_file_is_valid_mid_recording() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("me.wav");
        let mut s = WavSink::create(&path).unwrap();
        s.write(&vec![5i16; 40_000]).unwrap(); // passes the 2 s flush point
        let ms = wav_duration_ms(&path).unwrap();
        assert!(ms >= 2_000, "{ms}");
        drop(s);
    }

    #[test]
    fn repair_recovers_audio_after_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("them.wav");
        let mut s = WavSink::create(&path).unwrap();
        s.write(&vec![7i16; 32_000]).unwrap();
        s.flush().unwrap();
        // the crash: more audio reaches the disk but the header still says 32000 samples
        std::mem::forget(s);
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(&vec![0u8; 16_000 * 2]).unwrap();
        f.write_all(&[1u8]).unwrap(); // a torn half sample
        drop(f);
        assert_eq!(repair_wav(&path).unwrap(), 48_000);
        assert_eq!(wav_duration_ms(&path), Some(3_000));
        let r = hound::WavReader::open(&path).unwrap();
        assert_eq!(r.into_samples::<i16>().count(), 48_000);
    }

    #[test]
    fn repair_rejects_non_wav_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.wav");
        std::fs::write(&path, b"hello world, not a wav").unwrap();
        assert!(repair_wav(&path).is_err());
    }
}
