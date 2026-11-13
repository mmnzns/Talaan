//! Runs one recording: two sources, two writer threads, the session, and the meeting folder.

use super::clock::RATE;
use super::feed::{feed, FeedConsumer};
use super::pipeline::TrackPipeline;
use super::session::Session;
use super::source::{AudioSource, SharedFeed};
use super::timeline::SampleSink;
use super::wav::WavSink;
use super::wav::repair_wav;
use crate::storage::model::{Mark, MeetingStatus, ProcessingError, RecordingIssue};
use crate::storage::paths::{ME_WAV, THEM_WAV};
use crate::storage::Storage;
use chrono::{DateTime, FixedOffset};
use serde::Serialize;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const SAVE_EVERY_NS: u64 = 5_000_000_000;
const TRACKS: usize = 2;
/// When both tracks failed, less than this saved (per track) isn't worth transcribing.
const MIN_USABLE_MS: u64 = 1_000;

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

/// What a writer thread shows the engine while it runs.
#[derive(Default)]
struct WriterState {
    level: AtomicU32,
    /// 16 kHz samples written so far, so a failure can say how far the file got.
    written: AtomicU64,
    /// Set, before the thread ends, when the writer gives up: the error's kind and the system's text.
    error: Mutex<Option<(io::ErrorKind, String)>>,
}

struct Track {
    name: &'static str,
    path: PathBuf,
    state: Arc<WriterState>,
    handle: JoinHandle<io::Result<u64>>,
}

impl Track {
    /// What the writer reported when it ended (it only ends early by failing).
    fn failure(&self) -> WriteFailure {
        let (kind, reason) = self.state.error.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or((io::ErrorKind::Other, "the writer stopped".into()));
        WriteFailure { track: self.name, kind, reason, saved_ms: self.state.written.load(Ordering::Relaxed) * 1000 / RATE as u64 }
    }
}

/// A track whose writer stopped before the recording ended (the disk filled up, or a write failed).
#[derive(Clone, Debug, PartialEq)]
pub struct WriteFailure {
    /// "me" or "them".
    pub track: &'static str,
    pub kind: io::ErrorKind,
    /// The system's text for the error (English, no paths).
    pub reason: String,
    /// How much of the track is in the file, in ms.
    pub saved_ms: u64,
}

/// Where a writer puts its samples, and how it closes the file.
pub trait TrackSink: SampleSink + Send + 'static {
    fn close(self) -> io::Result<()>;
}

impl TrackSink for WavSink {
    fn close(self) -> io::Result<()> {
        self.finalize()
    }
}

/// What `shutdown` leaves for `stop`.
struct Stopped {
    session: Arc<Mutex<Session>>,
    failures: Vec<WriteFailure>,
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
    /// How many periodic saves failed this recording (only the first is logged).
    save_failures: u32,
}

#[derive(Default)]
pub struct Engine {
    active: Option<Active>,
}

fn spawn_writer<S: TrackSink>(consumer: FeedConsumer, sink: S, session: Arc<Mutex<Session>>, stop: Arc<AtomicBool>, end_pos: Arc<AtomicU64>, state: Arc<WriterState>) -> io::Result<JoinHandle<io::Result<u64>>> {
    std::thread::Builder::new().name("talaan-writer".into()).spawn(move || {
        let result = write_track(consumer, sink, &session, &stop, &end_pos, &state);
        if let Err(e) = &result {
            *state.error.lock().unwrap_or_else(|e| e.into_inner()) = Some((e.kind(), e.to_string()));
        }
        result
    })
}

/// The writer's loop: drains the feed into the file until told to stop, then pads to `end_pos` and closes it. The
/// first error ends it (and drops the sink, which closes the file as far as it can).
fn write_track<S: TrackSink>(mut consumer: FeedConsumer, sink: S, session: &Mutex<Session>, stop: &AtomicBool, end_pos: &AtomicU64, state: &WriterState) -> io::Result<u64> {
    let mut pipeline = TrackPipeline::new(sink);
    let mut buf = Vec::new();
    let mut last_level = Instant::now();
    loop {
        let mut got = false;
        while let Some(h) = consumer.pop(&mut buf) {
            got = true;
            let pos = session.lock().unwrap_or_else(|e| e.into_inner()).clock.position_samples(h.host_ns);
            pipeline.push(pos, h.rate, h.channels as usize, &buf)?;
            state.written.store(pipeline.written(), Ordering::Relaxed);
        }
        if last_level.elapsed() >= Duration::from_millis(50) {
            state.level.store(pipeline.level().to_bits(), Ordering::Relaxed);
            last_level = Instant::now();
        }
        if stop.load(Ordering::Acquire) && consumer.is_empty() {
            pipeline.finish(end_pos.load(Ordering::Acquire))?;
            let written = pipeline.written();
            pipeline.into_sink().close()?;
            return Ok(written);
        }
        if !got {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Engine {
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// The meeting being recorded (or paused), if any.
    pub fn meeting_id(&self) -> Option<String> {
        self.session().map(|s| s.meeting_id.clone())
    }

    pub fn start(
        &mut self,
        storage: &Storage,
        app: &str,
        app_label: &str,
        mic: Box<dyn AudioSource>,
        system: Box<dyn AudioSource>,
        now_ns: u64,
        started_at: DateTime<FixedOffset>,
    ) -> Result<Status, String> {
        self.start_with_sinks(storage, (app, app_label), mic, system, (now_ns, started_at), WavSink::create)
    }

    /// `start`, with the file each track is written to (tests make one that fails).
    fn start_with_sinks<S: TrackSink>(
        &mut self,
        storage: &Storage,
        (app, app_label): (&str, &str),
        mut mic: Box<dyn AudioSource>,
        mut system: Box<dyn AudioSource>,
        (now_ns, started_at): (u64, DateTime<FixedOffset>),
        make_sink: impl Fn(&Path) -> io::Result<S>,
    ) -> Result<Status, String> {
        if self.active.is_some() {
            return Err(crate::i18n::tr("recorder.already_recording"));
        }
        let file = storage.create_meeting(app, app_label, started_at).map_err(|e| e.to_string())?;
        let dir = storage.dir(&file.id).map_err(|e| e.to_string())?;
        let session = Arc::new(Mutex::new(Session::new(file.id.clone(), now_ns)));
        let stop = Arc::new(AtomicBool::new(false));
        let end_pos = Arc::new(AtomicU64::new(0));

        let mut tracks = vec![];
        let mut feeds: Vec<SharedFeed> = vec![];
        for (name, file_name) in [("me", ME_WAV), ("them", THEM_WAV)] {
            let (producer, consumer) = feed();
            let state = Arc::new(WriterState::default());
            let path = dir.join(file_name);
            let sink = make_sink(&path).map_err(|e| e.to_string())?;
            let handle = spawn_writer(consumer, sink, session.clone(), stop.clone(), end_pos.clone(), state.clone()).map_err(|e| e.to_string())?;
            tracks.push(Track { name, path, state, handle });
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
            mic_name: mic_result.unwrap_or_else(|_| crate::i18n::tr("recorder.no_mic")),
            system_ok: system_result.is_ok(),
            stop,
            end_pos,
            tracks,
            last_save_ns: now_ns,
            save_failures: 0,
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
            Some(a) => (f32::from_bits(a.tracks[0].state.level.load(Ordering::Relaxed)), f32::from_bits(a.tracks[1].state.level.load(Ordering::Relaxed))),
            None => (0.0, 0.0),
        }
    }

    /// True once system audio stopped coming through for good during this recording.
    pub fn system_failed(&self) -> bool {
        self.active.as_ref().is_some_and(|a| a.system.failed())
    }

    /// A track whose writer ended while the recording is still going: it failed, and what comes after it is lost.
    pub fn failed_track(&self) -> Option<WriteFailure> {
        let a = self.active.as_ref()?;
        if a.stop.load(Ordering::Acquire) {
            return None;
        }
        a.tracks.iter().find(|t| t.handle.is_finished()).map(|t| t.failure())
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
        let saved = storage.update_meeting_file(&s.meeting_id, |f| s.apply(f, now_ns));
        if let Err(e) = &saved {
            a.save_failures += 1;
            if a.save_failures == 1 {
                log::warn!("saving the recording's progress failed (meeting {}, later tries stay quiet): {}", s.meeting_id, crate::logging::text(&e.to_string()));
            }
        }
    }

    #[cfg(test)]
    fn save_failures(&self) -> u32 {
        self.active.as_ref().map_or(0, |a| a.save_failures)
    }

    pub fn restart_mic(&mut self) -> Result<String, String> {
        let a = self.active.as_mut().ok_or_else(|| crate::i18n::tr("recorder.not_recording"))?;
        let name = a.mic.restart()?;
        a.mic_name = name.clone();
        Ok(name)
    }

    pub fn restart_system(&mut self) -> Result<String, String> {
        let a = self.active.as_mut().ok_or_else(|| crate::i18n::tr("recorder.not_recording"))?;
        let name = a.system.restart()?;
        a.system_ok = true;
        Ok(name)
    }

    /// Stops sources, lets the writers drain and close the files. Returns the session and the tracks whose writer
    /// failed. With `repair`, a failed track's file is made readable up to where it stopped.
    fn shutdown(&mut self, now_ns: u64, repair: bool) -> Option<Stopped> {
        let mut a = self.active.take()?;
        a.mic.stop();
        a.system.stop();
        let elapsed = a.session.lock().unwrap_or_else(|e| e.into_inner()).elapsed_ms(now_ns);
        a.end_pos.store(elapsed * RATE as u64 / 1000, Ordering::Release);
        a.stop.store(true, Ordering::Release);
        let mut failures = vec![];
        for t in a.tracks {
            let (kind, reason) = match t.handle.join() {
                Ok(Ok(_)) => continue,
                Ok(Err(e)) => (e.kind(), e.to_string()),
                Err(_) => (io::ErrorKind::Other, "the writer stopped unexpectedly".to_string()),
            };
            let written_ms = t.state.written.load(Ordering::Relaxed) * 1000 / RATE as u64;
            // The file's own length is the truth: the writer may have got further than it counted, or not.
            let saved_ms = if repair { repair_wav(&t.path).map_or(0, |samples| samples * 1000 / RATE as u64) } else { written_ms };
            let track = t.name;
            log::warn!("the {track} track stopped saving at {saved_ms} ms: {kind}");
            failures.push(WriteFailure { track: t.name, kind, reason, saved_ms });
        }
        Some(Stopped { session: a.session, failures })
    }

    /// Finishes the recording and marks the meeting ready. Returns its id. A track whose writer failed (the disk
    /// filled up) is kept as far as it got and the meeting says so; if both failed and nothing usable was saved the
    /// meeting is Failed instead of transcribing silence. The sources are already shut down when the final save
    /// fails, so the meeting is left in needs_recovery (see `save_finished`) and the error is returned.
    pub fn stop(&mut self, storage: &Storage, now_ns: u64) -> Result<String, String> {
        let Stopped { session, failures } = self.shutdown(now_ns, true).ok_or_else(|| crate::i18n::tr("recorder.not_recording"))?;
        let s = session.lock().unwrap_or_else(|e| e.into_inner());
        let saved = failures.iter().map(|f| f.saved_ms);
        let lost_all = failures.len() == TRACKS;
        let usable_ms = saved.clone().max().unwrap_or(0);
        let nothing_usable = lost_all && usable_ms < MIN_USABLE_MS;
        let issue = saved.min().map(|at_ms| RecordingIssue { kind: "write_failed".into(), at_ms });
        if nothing_usable {
            // Two empty files aren't audio: without them the meeting page doesn't offer a retry on nothing.
            if let Ok(dir) = storage.dir(&s.meeting_id) {
                for name in [ME_WAV, THEM_WAV] {
                    let _ = std::fs::remove_file(dir.join(name));
                }
            }
        }
        let first = if nothing_usable { MeetingStatus::Failed } else { MeetingStatus::Transcribing };
        save_finished(first, |status| {
            storage
                .update_meeting_file(&s.meeting_id, |f| {
                    s.apply(f, now_ns);
                    if lost_all {
                        // Each track ended early: that is how much of the call there is.
                        f.duration_ms = usable_ms;
                    }
                    f.recording_issue = issue.clone();
                    f.progress = (status == MeetingStatus::Transcribing).then_some(0.0);
                    if status == MeetingStatus::Failed {
                        f.error = Some(ProcessingError {
                            code: "write_failed".into(),
                            message: "Nothing could be saved because the disk was full or couldn't be written to.".into(),
                            detail: None,
                        });
                    }
                    // Transcribing: the transcriber picks it up from here.
                    f.status = status;
                })
                .map_err(|e| e.to_string())
        })?;
        Ok(s.meeting_id.clone())
    }

    /// Stops and deletes everything recorded. If the folder can't be removed, what is left is marked
    /// needs_recovery rather than "recording" (the sources are already off), and the error is returned.
    pub fn discard(&mut self, storage: &Storage, now_ns: u64) -> Result<(), String> {
        let Stopped { session, .. } = self.shutdown(now_ns, false).ok_or_else(|| crate::i18n::tr("recorder.not_recording"))?;
        let id = session.lock().unwrap_or_else(|e| e.into_inner()).meeting_id.clone();
        storage.delete_meeting(&id).map_err(|e| {
            let _ = storage.set_status(&id, MeetingStatus::NeedsRecovery);
            e.to_string()
        })
    }
}

/// Writes the finished meeting.json through `write`, which sets the status it is given (`status`: Transcribing, or
/// Failed when nothing could be saved). A failed write is tried once more; if that fails too the meeting goes to
/// needs_recovery (the audio is closed and safe, and launch already offers Recover) instead of staying "recording"
/// with nothing running. The first error is returned.
fn save_finished(status: MeetingStatus, mut write: impl FnMut(MeetingStatus) -> Result<(), String>) -> Result<(), String> {
    write(status).or_else(|_| write(status)).inspect_err(|_| {
        let _ = write(MeetingStatus::NeedsRecovery);
    })
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

    /// A WAV sink that starts failing like a full disk once `limit` samples were written (None: never).
    pub struct Brittle {
        inner: WavSink,
        limit: Option<usize>,
        written: usize,
    }

    impl Brittle {
        fn create(path: &Path, limit: Option<usize>) -> io::Result<Self> {
            Ok(Brittle { inner: WavSink::create(path)?, limit, written: 0 })
        }
    }

    impl SampleSink for Brittle {
        fn write(&mut self, samples: &[i16]) -> io::Result<()> {
            if self.limit.is_some_and(|l| self.written + samples.len() > l) {
                return Err(io::Error::from(io::ErrorKind::StorageFull));
            }
            self.written += samples.len();
            self.inner.write(samples)
        }
    }

    impl TrackSink for Brittle {
        fn close(self) -> io::Result<()> {
            self.inner.close()
        }
    }

    /// Starts a recording whose me / them tracks fail after the given number of samples.
    fn start_brittle(e: &mut Engine, storage: &Storage, me_limit: Option<usize>, them_limit: Option<usize>) -> (String, Handle, Handle) {
        let (mic, me) = fake(false);
        let (sys, them) = fake(false);
        let st = e
            .start_with_sinks(storage, ("zoom", "Zoom"), mic, sys, (0, at()), |p| Brittle::create(p, if p.ends_with(ME_WAV) { me_limit } else { them_limit }))
            .unwrap();
        (st.meeting_id.unwrap(), me, them)
    }

    /// Waits up to 2 s for the engine to notice a writer that gave up.
    fn wait_for_failure(e: &Engine) -> Option<WriteFailure> {
        for _ in 0..400 {
            if let Some(f) = e.failed_track() {
                return Some(f);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    #[test]
    fn a_writer_whose_disk_fills_up_is_seen_while_still_recording() {
        let (_d, storage) = storage();
        let mut e = Engine::default();
        let (_id, me, them) = start_brittle(&mut e, &storage, None, Some(16_000)); // them: 1 s fits
        assert_eq!(e.failed_track(), None, "nothing wrong yet");
        push(&me, 0, 3_000, 0.2);
        push(&them, 0, 3_000, 0.4);
        let f = wait_for_failure(&e).expect("the them writer failed");
        assert_eq!(f.track, "them");
        assert_eq!(f.kind, io::ErrorKind::StorageFull);
        assert!(f.saved_ms >= 900 && f.saved_ms <= 1_000, "{}", f.saved_ms);
        assert!(e.is_active(), "the recording itself goes on");
        e.discard(&storage, 3_000 * MS).unwrap();
    }

    #[test]
    fn stop_after_one_track_failed_keeps_what_was_saved_and_says_so() {
        let (_d, storage) = storage();
        let mut e = Engine::default();
        let (id, me, them) = start_brittle(&mut e, &storage, None, Some(16_000));
        push(&me, 0, 3_000, 0.2);
        push(&them, 0, 3_000, 0.4);
        wait_for_failure(&e).expect("the them writer failed");
        assert_eq!(e.stop(&storage, 3_000 * MS).unwrap(), id);

        let f = storage.read_meeting_file(&id).unwrap();
        assert_eq!(f.status, MeetingStatus::Transcribing, "the saved part is still transcribed");
        assert_eq!(f.progress, Some(0.0));
        assert_eq!(f.duration_ms, 3_000, "the mic track covers the whole call");
        let issue = f.recording_issue.expect("the meeting says what happened");
        assert_eq!(issue.kind, "write_failed");
        assert!(issue.at_ms >= 900 && issue.at_ms <= 1_000, "{}", issue.at_ms);

        let dir = storage.dir(&id).unwrap();
        let them_ms = wav_duration_ms(&dir.join(THEM_WAV)).expect("the failed track is a readable file");
        assert!(them_ms >= 900 && them_ms <= 1_000, "{them_ms}");
        assert_eq!(wav_duration_ms(&dir.join(ME_WAV)), Some(3_000), "the good track is whole");
        let r = hound::WavReader::open(dir.join(ME_WAV)).unwrap();
        assert!(r.into_samples::<i16>().all(|s| s.unwrap() != 0), "and still has its audio");
    }

    #[test]
    fn a_failure_found_only_at_stop_is_handled_the_same_way() {
        // The writer is still going when Stop comes; it fails padding to the end (the them track heard nothing).
        let (_d, storage) = storage();
        let mut e = Engine::default();
        let (id, me, _them) = start_brittle(&mut e, &storage, None, Some(16_000));
        push(&me, 0, 3_000, 0.2);
        e.stop(&storage, 3_000 * MS).unwrap();
        let f = storage.read_meeting_file(&id).unwrap();
        assert_eq!(f.status, MeetingStatus::Transcribing);
        assert_eq!(f.recording_issue.map(|i| i.kind), Some("write_failed".into()));
        let ms = wav_duration_ms(&storage.dir(&id).unwrap().join(THEM_WAV)).unwrap();
        assert!(ms >= 700 && ms <= 1_000, "{ms}"); // the silence is written in 4096-sample steps up to the limit
    }

    #[test]
    fn when_both_tracks_fail_with_some_audio_saved_the_duration_is_what_was_saved() {
        let (_d, storage) = storage();
        let mut e = Engine::default();
        let (id, me, them) = start_brittle(&mut e, &storage, Some(32_000), Some(16_000));
        push(&me, 0, 4_000, 0.2);
        push(&them, 0, 4_000, 0.4);
        std::thread::sleep(Duration::from_millis(200));
        e.stop(&storage, 4_000 * MS).unwrap();
        let f = storage.read_meeting_file(&id).unwrap();
        assert_eq!(f.status, MeetingStatus::Transcribing);
        assert!(f.duration_ms >= 1_900 && f.duration_ms <= 2_000, "{}", f.duration_ms);
        assert!(f.recording_issue.unwrap().at_ms <= 1_000, "the earlier failure is where it starts");
    }

    #[test]
    fn when_both_tracks_fail_and_nothing_was_saved_the_meeting_fails() {
        let (_d, storage) = storage();
        let mut e = Engine::default();
        let (id, me, them) = start_brittle(&mut e, &storage, Some(0), Some(0));
        push(&me, 0, 2_000, 0.2);
        push(&them, 0, 2_000, 0.4);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(e.stop(&storage, 2_000 * MS).unwrap(), id);
        assert!(!e.is_active());
        let f = storage.read_meeting_file(&id).unwrap();
        assert_eq!(f.status, MeetingStatus::Failed);
        assert_eq!(f.progress, None);
        assert_eq!(f.error.as_ref().map(|e| e.code.as_str()), Some("write_failed"));
        assert!(f.error.unwrap().message.contains("disk"));
        assert_eq!(f.duration_ms, 0);
        assert!(!storage.get_meeting(&id).unwrap().has_audio, "two empty files aren't audio");
    }

    #[test]
    fn a_healthy_recording_never_reports_a_failed_track() {
        let (_d, storage) = storage();
        let mut e = Engine::default();
        let (_id, me, them) = start_brittle(&mut e, &storage, None, None);
        push(&me, 0, 2_000, 0.2);
        push(&them, 0, 2_000, 0.4);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(e.failed_track(), None);
        e.discard(&storage, 2_000 * MS).unwrap();
    }

    #[test]
    fn a_failed_final_save_is_tried_again_then_left_recoverable() {
        let mut tried = Vec::new();
        let r = save_finished(MeetingStatus::Transcribing, |status| {
            tried.push(status);
            if tried.len() == 1 { Err("disk busy".into()) } else { Ok(()) }
        });
        assert_eq!(r, Ok(()), "the retry worked");
        assert_eq!(tried, [MeetingStatus::Transcribing, MeetingStatus::Transcribing]);

        let mut tried = Vec::new();
        let r = save_finished(MeetingStatus::Transcribing, |status| {
            tried.push(status);
            if status == MeetingStatus::NeedsRecovery { Ok(()) } else { Err("disk full".into()) }
        });
        assert_eq!(r, Err("disk full".into()), "the caller still hears about it");
        assert_eq!(tried, [MeetingStatus::Transcribing, MeetingStatus::Transcribing, MeetingStatus::NeedsRecovery]);
    }

    #[test]
    fn the_engine_names_the_meeting_it_is_recording() {
        let (_d, storage) = storage();
        let mut e = Engine::default();
        assert_eq!(e.meeting_id(), None);
        let (mic, _me) = fake(false);
        let (sys, _them) = fake(false);
        e.start(&storage, "other", "Call", mic, sys, 0, at()).unwrap();
        let id = e.meeting_id().unwrap();
        assert_eq!(Some(id), e.status(0).meeting_id);
        e.stop(&storage, 100 * MS).unwrap();
        assert_eq!(e.meeting_id(), None);
    }

    #[test]
    fn a_stop_that_cannot_save_still_ends_the_recording() {
        let (_d, storage) = storage();
        let mut e = Engine::default();
        let (mic, _me) = fake(false);
        let (sys, _them) = fake(false);
        e.start(&storage, "other", "Call", mic, sys, 0, at()).unwrap();
        let id = e.status(0).meeting_id.unwrap();
        // meeting.json can't be read back: the final save fails.
        let json = storage.dir(&id).unwrap().join("meeting.json");
        std::fs::remove_file(&json).unwrap();
        std::fs::create_dir(&json).unwrap();
        assert!(e.stop(&storage, 100 * MS).is_err());
        assert!(!e.is_active());
        assert_eq!(e.status(0).state, "idle");
    }


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
    fn a_failed_periodic_save_is_counted_and_the_next_one_still_tries() {
        let (_d, storage) = storage();
        let (mic, _) = fake(false);
        let (sys, _) = fake(false);
        let mut e = Engine::default();
        let id = e.start(&storage, "zoom", "Zoom", mic, sys, 0, at()).unwrap().meeting_id.unwrap();
        let json = storage.dir(&id).unwrap().join("meeting.json");
        let good = std::fs::read(&json).unwrap();
        std::fs::remove_file(&json).unwrap();
        std::fs::create_dir(&json).unwrap();
        e.tick(&storage, 6_000 * MS);
        e.tick(&storage, 12_000 * MS);
        assert_eq!(e.save_failures(), 2);
        std::fs::remove_dir(&json).unwrap();
        std::fs::write(&json, good).unwrap();
        e.tick(&storage, 18_000 * MS);
        assert_eq!(e.save_failures(), 2, "a save that works isn't a failure");
        assert_eq!(storage.read_meeting_file(&id).unwrap().duration_ms, 18_000);
        e.discard(&storage, 19_000 * MS).unwrap();
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
