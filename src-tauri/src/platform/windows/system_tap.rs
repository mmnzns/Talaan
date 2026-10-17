//! System audio ("them") through WASAPI process loopback: everything the PC plays except Talaan's own process
//! tree, mixed by Windows (Windows 10 2004+). Not tied to an output device, so an output switch only needs the
//! new device's name.
//!
//! One capture thread per start: it joins the MTA, asks MMCSS for "Pro Audio" scheduling, waits on the WASAPI
//! event (200 ms timeout so a stop is noticed), drains every packet and pushes it with `try_lock`, so it never
//! blocks, like the Mac IOProc. If the stream dies (device invalidated, audio service restarted, or no packets
//! for 2 s), the thread reconnects. Only errors count toward `MAX_RECONNECTS`; past that it gives up and says so
//! through `failed()`. Silence alone is retried forever.

use super::clock::{hns_to_ns, host_now_ns};
use super::com;
use crate::recorder::source::{AudioSource, SharedFeed};
use ::windows::core::w;
use ::windows::Win32::System::Threading::{AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;

/// Process loopback has no mix format of its own; Windows converts to what we ask for.
const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
/// 20 ms of buffer, in 100 ns units.
const BUFFER_HNS: i64 = 200_000;
/// How long one wait for the WASAPI event lasts, so a stop is noticed.
const WAIT_MS: u32 = 200;
/// A healthy process-loopback stream delivers a packet (silent ones too) every period, so this long without one
/// means the stream is dead.
const DEAD_AFTER_MS: u32 = 2_000;
/// Reconnect attempts in a row before giving up.
const MAX_RECONNECTS: u32 = 10;
const RECONNECT_DELAY: Duration = Duration::from_millis(500);
/// How long a stop waits for the capture thread before detaching it.
const CLOSE_WAIT: Duration = Duration::from_secs(2);
/// 100 ms of 48 kHz stereo: buffers this size are allocated up front, so the hot path doesn't allocate.
const PREALLOC: usize = RATE as usize / 10 * CHANNELS as usize;

struct Live {
    stop: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    /// Signalled when the capture thread is about to return.
    done: mpsc::Receiver<()>,
    thread: JoinHandle<()>,
}

#[derive(Default)]
pub struct SystemTap {
    feed: Option<SharedFeed>,
    live: Option<Live>,
}

/// Whose audio the loopback hears. Talaan always uses `EveryoneButUs`; `OnlyUs` exists for the exclusion test's
/// control run.
#[derive(Clone, Copy)]
enum Target {
    EveryoneButUs,
    #[cfg(test)]
    OnlyUs,
}

/// Samples in a packet of `read_frames` frames.
fn packet_len(read_frames: u32) -> usize {
    read_frames as usize * CHANNELS as usize
}

/// The samples to push for one packet: as captured, or zeros of the same length when Windows flagged it silent.
/// `zeros` only grows when a packet is longer than any before it.
fn packet_samples<'a>(data: &'a [f32], silent: bool, zeros: &'a mut Vec<f32>) -> &'a [f32] {
    if !silent {
        return data;
    }
    if zeros.len() < data.len() {
        zeros.resize(data.len(), 0.0);
    }
    &zeros[..data.len()]
}

/// Packet stamps in host ns. The QPC position when Windows vouches for it; otherwise right after the previous
/// packet; host time only when there is no previous packet.
#[derive(Default)]
struct Stamps {
    next: Option<u64>,
}

impl Stamps {
    fn stamp(&mut self, qpc_hns: u64, flagged_bad: bool, frames: u32, now_ns: u64) -> u64 {
        let at = if !flagged_bad && qpc_hns != 0 { hns_to_ns(qpc_hns) } else { self.next.unwrap_or(now_ns) };
        self.next = Some(at + frames as u64 * 1_000_000_000 / RATE as u64);
        at
    }
}

/// Why a stream was dropped.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Loss {
    /// WASAPI returned an error (device invalidated, audio service restarted), or a reconnect failed.
    Error,
    /// No packets for `DEAD_AFTER_MS`.
    Silence,
}

/// When to give up reconnecting. Only errors count, so a stream that merely went quiet is retried forever.
#[derive(Default)]
struct Reconnects {
    errors: u32,
}

impl Reconnects {
    /// A stream was lost, or a reconnect failed (`Loss::Error`). False means give up.
    fn lost(&mut self, cause: Loss) -> bool {
        if cause == Loss::Error {
            self.errors += 1;
        }
        self.errors <= MAX_RECONNECTS
    }

    /// A reconnect after a loss of `cause` succeeded. One after silence proves the service is fine.
    fn connected(&mut self, cause: Loss) {
        if cause == Loss::Silence {
            self.errors = 0;
        }
    }

    /// Packets are arriving again.
    fn flowing(&mut self) {
        self.errors = 0;
    }
}

/// Whether this many event waits in a row without a packet mean the stream is dead.
fn stream_dead(consecutive_timeouts: u32) -> bool {
    consecutive_timeouts * WAIT_MS >= DEAD_AFTER_MS
}

/// The default output's name, for the UI ("Speakers (Realtek(R) Audio)"). Called on the capture thread, which
/// is already in the MTA.
fn output_name() -> String {
    wasapi::DeviceEnumerator::new()
        .and_then(|e| e.get_default_device(&wasapi::Direction::Render))
        .and_then(|d| d.get_friendlyname())
        .unwrap_or_else(|_| "System audio".into())
}

/// The f32 buffer as the bytes WASAPI copies into. Every bit pattern is a valid f32, so this is sound.
fn as_bytes_mut(samples: &mut [f32]) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(samples.as_mut_ptr().cast::<u8>(), std::mem::size_of_val(samples)) }
}

/// "Pro Audio" scheduling for the capture thread, reverted when dropped.
struct Mmcss(::windows::Win32::Foundation::HANDLE);

impl Mmcss {
    fn pro_audio() -> Option<Mmcss> {
        let mut index = 0u32;
        unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index) }.ok().map(Mmcss)
    }
}

impl Drop for Mmcss {
    fn drop(&mut self) {
        unsafe {
            let _ = AvRevertMmThreadCharacteristics(self.0);
        }
    }
}

/// A started loopback stream.
struct Stream {
    client: wasapi::AudioClient,
    event: wasapi::Handle,
    capture: wasapi::AudioCaptureClient,
}

impl Stream {
    fn connect(target: Target) -> Result<Stream, String> {
        // `include_tree = false` is PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE: everyone but Talaan.
        let include_tree = !matches!(target, Target::EveryoneButUs);
        let mut client = wasapi::AudioClient::new_application_loopback_client(std::process::id(), include_tree)
            .map_err(|e| format!("Couldn't start system audio capture (needs Windows 10 2004 or later): {e}"))?;
        fn step(what: &'static str) -> impl Fn(wasapi::WasapiError) -> String {
            move |e| format!("Couldn't {what} for system audio: {e}")
        }
        let format = wasapi::WaveFormat::new(32, 32, &wasapi::SampleType::Float, RATE as usize, CHANNELS as usize, None);
        let mode = wasapi::StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: BUFFER_HNS };
        client.initialize_client(&format, &wasapi::Direction::Capture, &mode).map_err(step("set up the 48 kHz stream"))?;
        let event = client.set_get_eventhandle().map_err(step("get the capture event"))?;
        let capture = client.get_audiocaptureclient().map_err(step("get the capture client"))?;
        client.start_stream().map_err(step("start the stream"))?;
        Ok(Stream { client, event, capture })
    }

    fn close(self) {
        let _ = self.client.stop_stream();
    }
}

/// The capture thread's working state: buffers allocated once, and the stamp history.
struct Pump {
    feed: SharedFeed,
    buf: Vec<f32>,
    zeros: Vec<f32>,
    stamps: Stamps,
}

impl Pump {
    /// Pushes every waiting packet. Returns whether any arrived; an `Err` means the stream is broken.
    fn drain(&mut self, stream: &Stream) -> Result<bool, wasapi::WasapiError> {
        let mut any = false;
        loop {
            let frames = match stream.capture.get_next_packet_size()? {
                Some(n) if n > 0 => n,
                _ => return Ok(any),
            };
            let n = packet_len(frames);
            if self.buf.len() < n {
                self.buf.resize(n, 0.0); // only for a packet longer than 100 ms
            }
            let (read, info) = match stream.capture.read_from_device(as_bytes_mut(&mut self.buf[..n])) {
                Ok(v) => v,
                // The packet grew between the size query and the read; it's released, the stream is fine.
                Err(wasapi::WasapiError::DataLengthTooShort { .. }) => continue,
                Err(e) => return Err(e),
            };
            if read == 0 {
                return Ok(any);
            }
            any = true;
            let host_ns = self.stamps.stamp(info.timestamp, info.flags.timestamp_error, read, host_now_ns());
            let data = packet_samples(&self.buf[..packet_len(read)], info.flags.silent, &mut self.zeros);
            if let Ok(mut f) = self.feed.try_lock() {
                f.push(host_ns, RATE, CHANNELS, data);
            }
        }
    }
}

/// Sleeps `d` in small steps; false if a stop was asked for meanwhile.
fn pause(d: Duration, stop: &AtomicBool) -> bool {
    let step = Duration::from_millis(50);
    let mut left = d;
    while !left.is_zero() {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let s = left.min(step);
        std::thread::sleep(s);
        left -= s;
    }
    !stop.load(Ordering::Relaxed)
}

/// Tells `close` the capture thread is finishing, on every return path.
struct Done(mpsc::Sender<()>);

impl Drop for Done {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

fn capture(
    target: Target,
    feed: SharedFeed,
    stop: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<String, String>>,
    done: mpsc::Sender<()>,
) {
    let _done = Done(done);
    com::init();
    let mut stream = match Stream::connect(target) {
        Ok(s) => s,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(output_name()));
    let _mmcss = Mmcss::pro_audio();
    let mut pump = Pump { feed, buf: vec![0.0; PREALLOC], zeros: vec![0.0; PREALLOC], stamps: Stamps::default() };
    let mut timeouts = 0u32;
    let mut reconnects = Reconnects::default();
    while !stop.load(Ordering::Relaxed) {
        let lost = match stream.event.wait_for_event(WAIT_MS) {
            Err(_) => {
                timeouts += 1;
                stream_dead(timeouts).then(|| (Loss::Silence, format!("no packets for {} ms", timeouts * WAIT_MS)))
            }
            Ok(()) => match pump.drain(&stream) {
                Ok(any) => {
                    timeouts = 0;
                    if any {
                        reconnects.flowing();
                    }
                    None
                }
                Err(e) => Some((Loss::Error, e.to_string())),
            },
        };
        let Some((mut cause, why)) = lost else { continue };
        log::warn!("call audio (loopback) lost: {}", crate::logging::text(&why));
        stream.close();
        // Reconnect: the new stream's stamps start fresh.
        pump.stamps = Stamps::default();
        timeouts = 0;
        stream = loop {
            if !reconnects.lost(cause) {
                log::error!("call audio (loopback): giving up after {MAX_RECONNECTS} failed reconnects");
                failed.store(true, Ordering::Relaxed);
                return;
            }
            if !pause(RECONNECT_DELAY, &stop) {
                return;
            }
            match Stream::connect(target) {
                Ok(s) => {
                    reconnects.connected(cause);
                    break s;
                }
                Err(e) => {
                    log::warn!("call audio (loopback): reconnect failed: {}", crate::logging::text(&e));
                    cause = Loss::Error;
                }
            }
        };
    }
    stream.close();
}

fn open(target: Target, feed: &SharedFeed) -> Result<(Live, String), String> {
    let stop = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let (done_tx, done) = mpsc::channel();
    let (f, s, x) = (feed.clone(), stop.clone(), failed.clone());
    let thread = std::thread::Builder::new()
        .name("talaan-loopback".into())
        .spawn(move || capture(target, f, s, x, tx, done_tx))
        .map_err(|e| e.to_string())?;
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(name)) => Ok((Live { stop, failed, done, thread }, name)),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Ordering::Relaxed);
            Err("System audio capture didn't start in time".into())
        }
    }
}

/// Stops the capture thread, waiting at most `CLOSE_WAIT`. This runs under the engine mutex, and a reconnect can
/// be stuck in loopback activation (it waits with no timeout), so a thread that doesn't finish in time is detached.
fn close(live: Live) {
    live.stop.store(true, Ordering::Relaxed);
    match live.done.recv_timeout(CLOSE_WAIT) {
        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = live.thread.join();
        }
        // The thread may outlive its SystemTap: it holds only Arc clones and exits once `connect` returns, since
        // `stop` is set.
        Err(mpsc::RecvTimeoutError::Timeout) => log::warn!("call audio (loopback): capture thread still busy, detached"),
    }
}

impl AudioSource for SystemTap {
    fn start(&mut self, feed: SharedFeed) -> Result<String, String> {
        self.feed = Some(feed.clone());
        let (live, name) = open(Target::EveryoneButUs, &feed)?;
        self.live = Some(live);
        Ok(name)
    }

    fn restart(&mut self) -> Result<String, String> {
        if let Some(live) = self.live.take() {
            close(live);
        }
        let feed = self.feed.clone().ok_or("System audio not started")?;
        let (live, name) = open(Target::EveryoneButUs, &feed)?;
        self.live = Some(live);
        Ok(name)
    }

    fn stop(&mut self) {
        if let Some(live) = self.live.take() {
            close(live);
        }
    }

    fn failed(&self) -> bool {
        self.live.as_ref().is_some_and(|l| l.failed.load(Ordering::Relaxed))
    }
}

impl Drop for SystemTap {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::host_now_ns;
    use crate::recorder::engine::Engine;
    use crate::recorder::mic::MicSource;
    use crate::storage::Storage;
    use std::path::Path;

    #[test]
    fn silent_packets_become_zeros_of_the_same_length() {
        let mut zeros = vec![];
        assert_eq!(packet_samples(&[0.5, -0.5, 0.25, 0.0], true, &mut zeros), &[0.0; 4]);
        assert_eq!(packet_samples(&[0.5, -0.5], false, &mut zeros), &[0.5, -0.5]);
    }

    #[test]
    fn packets_hold_two_samples_per_frame() {
        assert_eq!(packet_len(480), 960);
        assert_eq!(packet_len(0), 0);
    }

    #[test]
    fn good_stamps_are_used_and_bad_ones_continue_from_the_last() {
        let mut s = Stamps::default();
        assert_eq!(s.stamp(0, false, 480, 7_000), 7_000, "no previous stamp: host time");
        assert_eq!(s.stamp(1_000, false, 480, 99), 100_000, "QPC position in 100 ns units");
        assert_eq!(s.stamp(5, true, 480, 99), 10_100_000, "flagged bad: 480 frames (10 ms) after the last");
        assert_eq!(s.stamp(0, false, 480, 99), 20_100_000, "zero: another 10 ms on");
        assert_eq!(s.stamp(3_000, false, 480, 99), 300_000, "a good stamp wins again");
    }

    #[test]
    fn errors_give_up_after_the_limit() {
        let mut r = Reconnects::default();
        for _ in 0..MAX_RECONNECTS {
            assert!(r.lost(Loss::Error));
            r.connected(Loss::Error); // a reconnect that then breaks again before any packet
        }
        assert!(!r.lost(Loss::Error));
    }

    #[test]
    fn silence_never_gives_up() {
        let mut r = Reconnects::default();
        for _ in 0..1_000 {
            assert!(r.lost(Loss::Silence));
            r.connected(Loss::Silence);
        }
    }

    #[test]
    fn a_reconnect_after_silence_clears_earlier_errors() {
        let mut r = Reconnects::default();
        for _ in 0..MAX_RECONNECTS {
            assert!(r.lost(Loss::Error));
        }
        assert!(r.lost(Loss::Silence));
        r.connected(Loss::Silence);
        assert!(r.lost(Loss::Error));
    }

    #[test]
    fn failed_connects_after_silence_still_count() {
        let mut r = Reconnects::default();
        assert!(r.lost(Loss::Silence));
        for _ in 0..MAX_RECONNECTS {
            assert!(r.lost(Loss::Error)); // each failed connect
        }
        assert!(!r.lost(Loss::Error));
    }

    #[test]
    fn packets_flowing_clear_the_count() {
        let mut r = Reconnects::default();
        for _ in 0..MAX_RECONNECTS {
            assert!(r.lost(Loss::Error));
        }
        r.flowing();
        assert!(r.lost(Loss::Error));
    }

    #[test]
    fn two_seconds_without_packets_is_a_dead_stream() {
        assert!(!stream_dead(0));
        assert!(!stream_dead(9));
        assert!(stream_dead(10));
    }

    /// Seconds, RMS, and the RMS amplitude of the `hz` component (Goertzel) of a 16-bit WAV.
    fn wav_stats(path: &Path, hz: f64) -> (f64, f64, f64) {
        let mut r = hound::WavReader::open(path).unwrap();
        let rate = r.spec().sample_rate as f64;
        let x: Vec<f64> = r.samples::<i16>().map(|s| s.unwrap() as f64 / 32768.0).collect();
        (x.len() as f64 / rate, rms(&x), tone(&x, rate, hz))
    }

    fn rms(x: &[f64]) -> f64 {
        (x.iter().map(|v| v * v).sum::<f64>() / x.len().max(1) as f64).sqrt()
    }

    /// RMS amplitude of the `hz` component (a pure tone of amplitude A reads A/sqrt(2)).
    fn tone(x: &[f64], rate: f64, hz: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * hz / rate;
        let (mut re, mut im) = (0.0, 0.0);
        for (n, v) in x.iter().enumerate() {
            re += v * (w * n as f64).cos();
            im -= v * (w * n as f64).sin();
        }
        2.0 * (re * re + im * im).sqrt() / x.len().max(1) as f64 / std::f64::consts::SQRT_2
    }

    /// Records 5 s of system audio plus mic. Play something (a YouTube clip) while it runs.
    /// `cargo test system_smoke -- --ignored --nocapture`
    /// Runs from `src-tauri/`, needs a working mic, and is judged by its printed output (it asserts nothing).
    #[test]
    #[ignore]
    fn system_smoke() {
        let dir = std::env::temp_dir().join("talaan-system-smoke");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir).unwrap();
        let mut e = Engine::default();
        let st = e
            .start(&storage, "other", "Call", Box::new(MicSource::default()), Box::new(SystemTap::default()), host_now_ns(), chrono::Local::now().fixed_offset())
            .unwrap();
        println!("mic: {} · system audio: {} ({})", st.mic_name, st.system_audio, output_name());
        let mut peak = (0.0f32, 0.0f32);
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let (a, b) = e.levels();
            peak = (peak.0.max(a), peak.1.max(b));
        }
        let id = e.stop(&storage, host_now_ns()).unwrap();
        let d = storage.dir(&id).unwrap();
        println!("wrote {} · peak me {:.2} · peak them {:.2}", d.display(), peak.0, peak.1);
        let (secs, level, _) = wav_stats(&d.join("them.wav"), 440.0);
        println!("them.wav: {secs:.2} s · RMS {level:.4}");
    }

    /// A 440 Hz tone at amplitude 0.3, from this process, until dropped. On the output whose name contains `TALAAN_TONE_DEVICE` if set (to bypass a relay app such as FxSound,
    /// which re-plays everything under its own process), else the default output.
    fn play_tone() -> cpal::Stream {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let device = match std::env::var("TALAAN_TONE_DEVICE") {
            Ok(name) => host.output_devices().unwrap().find(|d| d.to_string().contains(&name)).expect("no output matches TALAAN_TONE_DEVICE"),
            Err(_) => host.default_output_device().expect("an output device"),
        };
        println!("tone on {device}");
        let supported = device.default_output_config().unwrap();
        assert_eq!(supported.sample_format(), cpal::SampleFormat::F32, "shared-mode WASAPI mixes in f32");
        let config = supported.config();
        let (rate, channels) = (config.sample_rate as f32, config.channels as usize);
        let mut phase = 0.0f32;
        let stream = device
            .build_output_stream::<f32, _, _>(
                config,
                move |out: &mut [f32], _| {
                    for frame in out.chunks_mut(channels) {
                        let v = 0.3 * (phase * std::f32::consts::TAU).sin();
                        phase = (phase + 440.0 / rate).fract();
                        frame.fill(v);
                    }
                },
                |e| eprintln!("tone: {e}"),
                None,
            )
            .unwrap();
        stream.play().unwrap();
        stream
    }

    /// Drains a feed: (seconds, RMS, 440 Hz amplitude, how far the first stamp is from "now" in ms).
    fn feed_stats(c: &mut crate::recorder::feed::FeedConsumer, opened_ns: u64) -> (f64, f64, f64, i64) {
        let (mut mono, mut buf, mut first) = (vec![], vec![], None);
        while let Some(h) = c.pop(&mut buf) {
            first.get_or_insert(h.host_ns);
            mono.extend(buf.chunks(h.channels as usize).map(|f| f.iter().map(|v| *v as f64).sum::<f64>() / f.len() as f64));
        }
        let skew = first.map(|t| (t as i64 - opened_ns as i64) / 1_000_000).unwrap_or(i64::MAX);
        (mono.len() as f64 / RATE as f64, rms(&mono), tone(&mono, RATE as f64, 440.0), skew)
    }

    /// A loopback of a process that plays nothing (this test process) still delivers zero-filled packets every
    /// period, so 30 s of silence is 30 s of samples and never trips the 2 s dead-stream check (no
    /// "loopback: no packets" on stderr). Runs from `src-tauri/`.
    /// `cargo test system_silence_keeps_packets_flowing -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn system_silence_keeps_packets_flowing() {
        let (p, mut c) = crate::recorder::feed::feed();
        let feed: SharedFeed = Arc::new(std::sync::Mutex::new(p));
        let opened = host_now_ns();
        let (live, _) = open(Target::OnlyUs, &feed).unwrap();
        let mut secs = 0.0;
        for _ in 0..30 {
            std::thread::sleep(Duration::from_secs(1));
            secs += feed_stats(&mut c, opened).0; // drain so the 20 s feed never fills
        }
        close(live);
        secs += feed_stats(&mut c, opened).0;
        println!("silence: {secs:.2} s of samples in 30 s");
        assert!(secs > 29.0, "packets stopped during silence");
    }

    /// The exclusion check: this test process plays a tone while it records. The tone is ours, so the normal
    /// (everyone-but-us) loopback must not hear it, while a control loopback of only our process must.
    /// `cargo test system_excludes_own_audio -- --ignored --nocapture`. Other sound may play (the check is for 440 Hz).
    /// Runs from `src-tauri/` and needs a working mic (it records through the real engine).
    /// If the default output is a relay app's virtual device (FxSound, Voicemeeter, Sonar...), that app re-plays our
    /// tone under its own process, and the loopback rightly hears it; set `TALAAN_TONE_DEVICE` to a real output.
    #[test]
    #[ignore]
    fn system_excludes_own_audio() {
        let _tone = play_tone();
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Through the real engine: them.wav must not contain the tone.
        let dir = std::env::temp_dir().join("talaan-system-exclusion");
        let _ = std::fs::remove_dir_all(&dir);
        let storage = Storage::new(dir).unwrap();
        let mut e = Engine::default();
        let st = e
            .start(&storage, "other", "Call", Box::new(MicSource::default()), Box::new(SystemTap::default()), host_now_ns(), chrono::Local::now().fixed_offset())
            .unwrap();
        println!("system audio: {}", st.system_audio);
        std::thread::sleep(std::time::Duration::from_secs(3));
        let id = e.stop(&storage, host_now_ns()).unwrap();
        let path = storage.dir(&id).unwrap().join("them.wav");
        let (secs, level, ours) = wav_stats(&path, 440.0);
        println!("excluded: {} · {secs:.2} s · RMS {level:.4} · 440 Hz {ours:.4}", path.display());

        // The raw loopback feed, before the engine: same check, plus whether the stamps are on host time.
        let raw = |target: Target| {
            let (p, mut c) = crate::recorder::feed::feed();
            let feed: SharedFeed = Arc::new(std::sync::Mutex::new(p));
            let opened = host_now_ns();
            let (live, _) = open(target, &feed).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(2));
            close(live);
            feed_stats(&mut c, opened)
        };
        let (rsecs, rlevel, rours, skew) = raw(Target::EveryoneButUs);
        println!("raw (everyone but us): {rsecs:.2} s · RMS {rlevel:.4} · 440 Hz {rours:.4} · first stamp {skew} ms after open");

        // Control: a loopback of only this process hears the tone (amplitude 0.3 reads ~0.21).
        let (csecs, clevel, cours, cskew) = raw(Target::OnlyUs);
        println!("control (only us): {csecs:.2} s · RMS {clevel:.4} · 440 Hz {cours:.4} · first stamp {cskew} ms after open");

        assert!(cours > 0.1, "the control loopback should hear our tone");
        assert!(ours < 0.01, "them.wav contains our own tone ({ours:.4})");
        assert!(skew.abs() < 1_000, "QPC stamps should be on host time");
    }

    /// Median of `now - stamp` (ms, signed) over the first buffers a feed delivers, polled every 2 ms.
    /// Also returns the (stamp, rms) of every buffer so a burst can be located.
    fn lag_and_levels(c: &mut crate::recorder::feed::FeedConsumer, lags: &mut Vec<i64>, levels: &mut Vec<(u64, f64)>) {
        let mut buf = vec![];
        while let Some(h) = c.pop(&mut buf) {
            let now = host_now_ns();
            lags.push((now as i64 - h.host_ns as i64) / 1_000_000);
            let x: Vec<f64> = buf.iter().map(|v| *v as f64).collect();
            levels.push((h.host_ns, rms(&x)));
        }
    }

    fn median(v: &[i64]) -> i64 {
        let mut s = v.to_vec();
        s.sort();
        s[s.len() / 2]
    }

    /// Both tracks' stamps sit on the host clock: the median of `now - stamp` over the first ~50 buffers of each
    /// source (the stamp-to-delivery lag) is under 200 ms. A separate process (PowerShell) plays a Windows sound so
    /// the loopback has a burst to find; its arrival on the "Them" clock is printed against the moment we spawned it.
    /// Without speakers the mic can't hear the burst, so true cross-track alignment is not measured here.
    /// `cargo test tracks_line_up_smoke -- --ignored --nocapture` from `src-tauri/`, with a working mic.
    #[test]
    #[ignore]
    fn tracks_line_up_smoke() {
        use crate::recorder::source::AudioSource;
        let (mp, mut mc) = crate::recorder::feed::feed();
        let (sp, mut sc) = crate::recorder::feed::feed();
        let mut mic = MicSource::default();
        let mut tap = SystemTap::default();
        mic.start(Arc::new(std::sync::Mutex::new(mp))).unwrap();
        tap.start(Arc::new(std::sync::Mutex::new(sp))).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        // Discard the backlog that piled up while we slept: the lag is judged on buffers drained as they arrive.
        let mut scrap = (vec![], vec![]);
        lag_and_levels(&mut mc, &mut scrap.0, &mut scrap.1);
        lag_and_levels(&mut sc, &mut scrap.0, &mut scrap.1);

        let spawned = host_now_ns();
        let mut ps = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", r"(New-Object Media.SoundPlayer 'C:\Windows\Media\Windows Ding.wav').PlaySync()"])
            .spawn()
            .expect("powershell");

        let (mut mlag, mut mlev, mut slag, mut slev) = (vec![], vec![], vec![], vec![]);
        for _ in 0..3000 {
            std::thread::sleep(Duration::from_millis(2));
            lag_and_levels(&mut mc, &mut mlag, &mut mlev);
            lag_and_levels(&mut sc, &mut slag, &mut slev);
            if mlag.len() >= 50 && slag.len() >= 50 && ps.try_wait().unwrap().is_some() {
                break;
            }
        }
        let _ = ps.wait();
        mic.stop();
        tap.stop();

        let (m50, s50) = (&mlag[..mlag.len().min(50)], &slag[..slag.len().min(50)]);
        println!("mic: {} buffers, median stamp lag {} ms (max {})", mlag.len(), median(m50), m50.iter().max().unwrap());
        println!("them: {} buffers, median stamp lag {} ms (max {})", slag.len(), median(s50), s50.iter().max().unwrap());
        let base = slev.iter().take(10).map(|l| l.1).fold(0.0, f64::max);
        let peak = slev.iter().copied().fold((0, 0.0), |a, l| if l.1 > a.1 { l } else { a });
        let first = slev.iter().find(|l| l.1 > (base * 2.0).max(0.02));
        println!("loopback baseline RMS {base:.4} · peak {:.4} at {:.0} ms after spawn", peak.1, (peak.0 as f64 - spawned as f64) / 1e6);
        match first {
            Some((t, level)) => println!("burst on Them clock {:.0} ms after spawn (RMS {level:.3}; includes PowerShell startup)", (*t as f64 - spawned as f64) / 1e6),
            None => println!("no burst heard on the loopback (is the output muted?)"),
        }
        assert!(median(m50).abs() < 200, "mic lag {}", median(m50));
        assert!(median(s50).abs() < 200, "system lag {}", median(s50));
    }
}
