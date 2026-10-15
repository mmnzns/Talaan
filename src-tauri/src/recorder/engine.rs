//! Runs one recording: two sources, two writer threads, the session, and the meeting folder.

use super::clock::RATE;
use super::feed::{feed, FeedConsumer};
use super::pipeline::TrackPipeline;
use super::session::Session;
use super::source::{AudioSource, SharedFeed};
use super::wav::WavSink;
use crate::storage::model::{Mark, MeetingStatus};
use crate::storage::paths::{ME_WAV, THEM_WAV};
use crate::storage::Storage;
use chrono::{DateTime, FixedOffset};
use serde::Serialize;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const SAVE_EVERY_NS: u64 = 5_000_000_000;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: &'static str, // "idle" | "recording" | "paused"
    pub meeting_id: Option<String>,
    pub elapsed_ms: u64,
    pub marks: usize,
    pub notes: usize,
    pub mic_name: String,
    pub system_audio: bool,
}

impl Status {
    fn idle() -> Self {
        Status { state: "idle", meeting_id: None, elapsed_ms: 0, marks: 0, notes: 0, mic_name: String::new(), system_audio: false }
    }
}

struct Track {
    level: Arc<AtomicU32>,
    handle: JoinHandle<io::Result<u64>>,
}

struct Active {
    session: Arc<Mutex<Session>>,
    mic: Box<dyn AudioSource>,
    system: Box<dyn AudioSource>,
    mic_name: String,
    system_ok: bool,
    stop: Arc<AtomicBool>,
    end_pos: Arc<AtomicU64>,
    tracks: Vec<Track>,
    last_save_ns: u64,
}

#[derive(Default)]
pub struct Engine {
    active: Option<Active>,
}

fn spawn_writer(mut consumer: FeedConsumer, path: &Path, session: Arc<Mutex<Session>>, stop: Arc<AtomicBool>, end_pos: Arc<AtomicU64>, level: Arc<AtomicU32>) -> io::Result<JoinHandle<io::Result<u64>>> {
    let mut pipeline = TrackPipeline::new(WavSink::create(path)?);
    Ok(std::thread::Builder::new().name("talaan-writer".into()).spawn(move || {
        let mut buf = Vec::new();
        let mut last_level = Instant::now();
        loop {
            let mut got = false;
            while let Some(h) = consumer.pop(&mut buf) {
                got = true;
                let pos = session.lock().unwrap_or_else(|e| e.into_inner()).clock.position_samples(h.host_ns);
                pipeline.push(pos, h.rate, h.channels as usize, &buf)?;
            }
            if last_level.elapsed() >= Duration::from_millis(50) {
                level.store(pipeline.level().to_bits(), Ordering::Relaxed);
                last_level = Instant::now();
            }
            if stop.load(Ordering::Acquire) && consumer.is_empty() {
                pipeline.finish(end_pos.load(Ordering::Acquire))?;
                let written = pipeline.written();
                pipeline.into_sink().finalize()?;
                return Ok(written);
            }
            if !got {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    })?)
}

impl Engine {
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn start(
        &mut self,
        storage: &Storage,
        app: &str,
        app_label: &str,
        mut mic: Box<dyn AudioSource>,
        mut system: Box<dyn AudioSource>,
        now_ns: u64,
        started_at: DateTime<FixedOffset>,
    ) -> Result<Status, String> {
        if self.active.is_some() {
            return Err("Already recording".into());
        }
        let file = storage.create_meeting(app, app_label, started_at).map_err(|e| e.to_string())?;
        let dir = storage.dir(&file.id).map_err(|e| e.to_string())?;
        let session = Arc::new(Mutex::new(Session::new(file.id.clone(), now_ns)));
        let stop = Arc::new(AtomicBool::new(false));
        let end_pos = Arc::new(AtomicU64::new(0));

        let mut tracks = vec![];
        let mut feeds: Vec<SharedFeed> = vec![];
        for name in [ME_WAV, THEM_WAV] {
            let (producer, consumer) = feed();
            let level = Arc::new(AtomicU32::new(0));
            let handle = spawn_writer(consumer, &dir.join(name), session.clone(), stop.clone(), end_pos.clone(), level.clone()).map_err(|e| e.to_string())?;
            tracks.push(Track { level, handle });
            feeds.push(Arc::new(Mutex::new(producer)));
        }

        let mic_result = mic.start(feeds[0].clone());
        let system_result = system.start(feeds[1].clone());
        if mic_result.is_err() && system_result.is_err() {
            mic.stop();
            system.stop();
            stop.store(true, Ordering::Release);
            for t in tracks {
                let _ = t.handle.join();
            }
            let _ = storage.delete_meeting(&file.id);
            return Err(mic_result.err().unwrap());
        }

        self.active = Some(Active {
            session,
            mic,
            system,
            mic_name: mic_result.unwrap_or_else(|_| "No microphone".into()),
            system_ok: system_result.is_ok(),
            stop,
            end_pos,
            tracks,
            last_save_ns: now_ns,
        });
        Ok(self.status(now_ns))
    }

    fn session(&self) -> Option<std::sync::MutexGuard<'_, Session>> {
        self.active.as_ref().map(|a| a.session.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn status(&self, now_ns: u64) -> Status {
        let Some(a) = &self.active else { return Status::idle() };
        let s = a.session.lock().unwrap_or_else(|e| e.into_inner());
        Status {
            state: if s.is_paused() { "paused" } else { "recording" },
            meeting_id: Some(s.meeting_id.clone()),
            elapsed_ms: s.elapsed_ms(now_ns),
            marks: s.marks.len(),
            notes: s.notes(),
            mic_name: a.mic_name.clone(),
            system_audio: a.system_ok,
        }
    }

    pub fn levels(&self) -> (f32, f32) {
        match &self.active {
            Some(a) => (f32::from_bits(a.tracks[0].level.load(Ordering::Relaxed)), f32::from_bits(a.tracks[1].level.load(Ordering::Relaxed))),
            None => (0.0, 0.0),
        }
    }

    /// True once system audio stopped coming through for good during this recording.
    pub fn system_failed(&self) -> bool {
        self.active.as_ref().is_some_and(|a| a.system.failed())
    }

    pub fn pause(&mut self, now_ns: u64) -> bool {
        self.session().is_some_and(|mut s| s.pause(now_ns))
    }

    pub fn resume(&mut self, now_ns: u64) -> bool {
        self.session().is_some_and(|mut s| s.resume(now_ns))
    }

    pub fn mark(&mut self, now_ns: u64, note: Option<String>) -> Option<Mark> {
        self.session().map(|mut s| s.mark(now_ns, note))
    }

    /// Saves duration, marks and pauses every few seconds so a crash keeps them.
    pub fn tick(&mut self, storage: &Storage, now_ns: u64) {
        let Some(a) = &mut self.active else { return };
        if now_ns.saturating_sub(a.last_save_ns) < SAVE_EVERY_NS {
            return;
        }
        a.last_save_ns = now_ns;
        let s = a.session.lock().unwrap_or_else(|e| e.into_inner());
        let _ = storage.update_meeting_file(&s.meeting_id, |f| s.apply(f, now_ns));
    }

    pub fn restart_mic(&mut self) -> Result<String, String> {
        let a = self.active.as_mut().ok_or("Not recording")?;
        let name = a.mic.restart()?;
        a.mic_name = name.clone();
        Ok(name)
    }

    pub fn restart_system(&mut self) -> Result<String, String> {
        let a = self.active.as_mut().ok_or("Not recording")?;
        let name = a.system.restart()?;
        a.system_ok = true;
        Ok(name)
    }

    /// Stops sources, lets the writers drain and close the files. Returns the session.
    fn shutdown(&mut self, now_ns: u64) -> Option<(Arc<Mutex<Session>>, u64)> {
        let mut a = self.active.take()?;
        a.mic.stop();
        a.system.stop();
        let elapsed = a.session.lock().unwrap_or_else(|e| e.into_inner()).elapsed_ms(now_ns);
        a.end_pos.store(elapsed * RATE as u64 / 1000, Ordering::Release);
        a.stop.store(true, Ordering::Release);
        for t in a.tracks {
            let _ = t.handle.join();
        }
        Some((a.session, elapsed))
    }

    /// Finishes the recording and marks the meeting ready. Returns its id.
    pub fn stop(&mut self, storage: &Storage, now_ns: u64) -> Result<String, String> {
        let (session, _) = self.shutdown(now_ns).ok_or("Not recording")?;
        let s = session.lock().unwrap_or_else(|e| e.into_inner());
        storage
            .update_meeting_file(&s.meeting_id, |f| {
                s.apply(f, now_ns);
                // The transcriber picks it up from here.
                f.status = MeetingStatus::Transcribing;
                f.progress = Some(0.0);
            })
            .map_err(|e| e.to_string())?;
        Ok(s.meeting_id.clone())
    }

    /// Stops and deletes everything recorded.
    pub fn discard(&mut self, storage: &Storage, now_ns: u64) -> Result<(), String> {
        let (session, _) = self.shutdown(now_ns).ok_or("Not recording")?;
        let id = session.lock().unwrap_or_else(|e| e.into_inner()).meeting_id.clone();
        storage.delete_meeting(&id).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::recorder::wav::wav_duration_ms;
    use crate::storage::model::PauseGap;

    pub type Handle = Arc<Mutex<Option<SharedFeed>>>;

    pub struct FakeSource {
        handle: Handle,
        fail: bool,
    }

    pub fn fake(fail: bool) -> (Box<dyn AudioSource>, Handle) {
        let handle: Handle = Arc::new(Mutex::new(None));
        (Box::new(FakeSource { handle: handle.clone(), fail }), handle)
    }

    impl AudioSource for FakeSource {
        fn start(&mut self, feed: SharedFeed) -> Result<String, String> {
            if self.fail {
                return Err("denied".into());
            }
            *self.handle.lock().unwrap() = Some(feed);
            Ok("Fake mic".into())
        }
        fn restart(&mut self) -> Result<String, String> {
            Ok("Fake mic 2".into())
        }
        fn stop(&mut self) {
            *self.handle.lock().unwrap() = None;
        }
    }

    /// A source that starts, then gives up on its own.
    struct GaveUp;

    impl AudioSource for GaveUp {
        fn start(&mut self, _feed: SharedFeed) -> Result<String, String> {
            Ok("Gone".into())
        }
        fn restart(&mut self) -> Result<String, String> {
            Ok("Gone".into())
        }
        fn stop(&mut self) {}
        fn failed(&self) -> bool {
            true
        }
    }

    #[test]
    fn a_system_source_that_gave_up_is_reported() {
        let (_d, storage) = storage();
        let mut e = Engine::default();
        assert!(!e.system_failed(), "not recording");
        let (mic, _me) = fake(false);
        e.start(&storage, "other", "Call", mic, Box::new(GaveUp), 0, at()).unwrap();
        assert!(e.system_failed());
        e.stop(&storage, 100 * MS).unwrap();
        assert!(!e.system_failed());

        let (mic, _me) = fake(false);
        let (sys, _them) = fake(false);
        e.start(&storage, "other", "Call", mic, sys, 0, at()).unwrap();
        assert!(!e.system_failed(), "sources don't fail by default");
    }

    const MS: u64 = 1_000_000;

    /// Pushes 10 ms buffers of 16 kHz audio from `from_ms` to `to_ms` (host time).
    pub fn push(h: &Handle, from_ms: u64, to_ms: u64, value: f32) {
        let guard = h.lock().unwrap();
        let Some(feed) = guard.as_ref() else { return };
        let mut p = feed.lock().unwrap();
        let mut t = from_ms;
        while t < to_ms {
            assert!(p.push(t * MS, 16_000, 1, &[value; 160]));
            t += 10;
        }
    }

    fn at() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-14T14:30:00-07:00").unwrap()
    }

    fn storage() -> (tempfile::TempDir, Storage) {
        let d = tempfile::tempdir().unwrap();
        let s = Storage::new(d.path().to_path_buf()).unwrap();
        (d, s)
    }

    #[test]
    fn a_full_recording_with_a_pause_and_a_mark() {
        let (_d, storage) = storage();
        let (mic, me) = fake(false);
        let (sys, them) = fake(false);
        let mut e = Engine::default();
        let st = e.start(&storage, "zoom", "Zoom", mic, sys, 0, at()).unwrap();
        assert_eq!(st.state, "recording");
        let id = st.meeting_id.unwrap();
        assert_eq!(storage.read_meeting_file(&id).unwrap().status, MeetingStatus::Recording);

        push(&me, 0, 1_000, 0.2);
        push(&them, 0, 1_000, 0.4);
        assert!(e.pause(1_000 * MS));
        push(&me, 1_500, 1_600, 0.9); // captured while paused: dropped
        assert_eq!(e.status(2_000 * MS).state, "paused");
        assert!(e.resume(3_000 * MS));
        push(&me, 3_000, 4_000, 0.2);
        push(&them, 3_000, 4_000, 0.4);
        let m = e.mark(3_500 * MS, Some("pricing".into())).unwrap();
        assert_eq!(m.at, 1_500);

        assert_eq!(e.stop(&storage, 4_000 * MS).unwrap(), id);
        assert!(!e.is_active());

        let f = storage.read_meeting_file(&id).unwrap();
        assert_eq!(f.status, MeetingStatus::Transcribing);
        assert_eq!(f.duration_ms, 2_000);
        assert_eq!(f.pauses, vec![PauseGap { at: 1_000, duration_ms: 2_000 }]);
        assert_eq!(f.marks.len(), 1);
        let dir = storage.dir(&id).unwrap();
        assert_eq!(wav_duration_ms(&dir.join(ME_WAV)), Some(2_000));
        assert_eq!(wav_duration_ms(&dir.join(THEM_WAV)), Some(2_000));
        assert!(storage.get_meeting(&id).unwrap().has_audio);
    }

    #[test]
    fn without_system_audio_the_them_track_is_silence_of_the_same_length() {
        let (_d, storage) = storage();
        let (mic, me) = fake(false);
        let (sys, _them) = fake(true);
        let mut e = Engine::default();
        let st = e.start(&storage, "other", "Call", mic, sys, 0, at()).unwrap();
        assert!(!st.system_audio);
        push(&me, 0, 1_000, 0.2);
        let id = e.stop(&storage, 1_000 * MS).unwrap();
        let dir = storage.dir(&id).unwrap();
        assert_eq!(wav_duration_ms(&dir.join(THEM_WAV)), Some(1_000));
        let r = hound::WavReader::open(dir.join(THEM_WAV)).unwrap();
        assert!(r.into_samples::<i16>().all(|s| s.unwrap() == 0));
    }

    #[test]
    fn if_nothing_can_record_the_meeting_is_removed() {
        let (_d, storage) = storage();
        let (mic, _) = fake(true);
        let (sys, _) = fake(true);
        let mut e = Engine::default();
        assert!(e.start(&storage, "zoom", "Zoom", mic, sys, 0, at()).is_err());
        assert!(storage.list_meetings(&Default::default()).unwrap().is_empty());
        assert!(!e.is_active());
    }

    #[test]
    fn discard_deletes_the_meeting() {
        let (_d, storage) = storage();
        let (mic, me) = fake(false);
        let (sys, _) = fake(false);
        let mut e = Engine::default();
        e.start(&storage, "zoom", "Zoom", mic, sys, 0, at()).unwrap();
        push(&me, 0, 500, 0.2);
        e.discard(&storage, 500 * MS).unwrap();
        assert!(storage.list_meetings(&Default::default()).unwrap().is_empty());
    }

    #[test]
    fn tick_saves_progress_every_five_seconds() {
        let (_d, storage) = storage();
        let (mic, _) = fake(false);
        let (sys, _) = fake(false);
        let mut e = Engine::default();
        let id = e.start(&storage, "zoom", "Zoom", mic, sys, 0, at()).unwrap().meeting_id.unwrap();
        e.mark(1_000 * MS, None);
        e.tick(&storage, 3_000 * MS);
        assert_eq!(storage.read_meeting_file(&id).unwrap().duration_ms, 0);
        e.tick(&storage, 6_000 * MS);
        let f = storage.read_meeting_file(&id).unwrap();
        assert_eq!(f.duration_ms, 6_000);
        assert_eq!(f.marks.len(), 1);
        assert_eq!(f.status, MeetingStatus::Recording);
        e.discard(&storage, 7_000 * MS).unwrap();
    }

    #[test]
    fn only_one_recording_at_a_time() {
        let (_d, storage) = storage();
        let mut e = Engine::default();
        e.start(&storage, "zoom", "Zoom", fake(false).0, fake(false).0, 0, at()).unwrap();
        assert!(e.start(&storage, "zoom", "Zoom", fake(false).0, fake(false).0, 0, at()).is_err());
        e.discard(&storage, 0).unwrap();
    }
}
