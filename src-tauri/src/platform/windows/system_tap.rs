//! System audio ("them") through WASAPI process loopback: everything the PC plays except Talaan's own process
//! tree, mixed by Windows (Windows 10 2004+). Not tied to an output device, so an output switch only needs the
//! new device's name.
//!
//! One capture thread per start: it joins the MTA, asks MMCSS for "Pro Audio" scheduling, waits on the WASAPI
//! event (200 ms timeout so a stop is noticed), drains every packet and pushes it with `try_lock`, so it never
//! blocks, like the Mac IOProc.

use super::clock::{host_now_ns, hns_to_ns};
use super::com;
use crate::recorder::source::{AudioSource, SharedFeed};
use ::windows::core::w;
use ::windows::Win32::System::Threading::{AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

/// Process loopback has no mix format of its own; Windows converts to what we ask for.
const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
/// 20 ms of buffer, in 100 ns units.
const BUFFER_HNS: i64 = 200_000;

struct Live {
    stop: Arc<AtomicBool>,
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

/// The samples to push for one packet: as captured, or zeros when Windows flagged it silent.
pub fn packet_samples(data: &[f32], silent: bool, out: &mut Vec<f32>) {
    out.clear();
    if silent {
        out.resize(data.len(), 0.0);
    } else {
        out.extend_from_slice(data);
    }
}

/// The default output's name, for the UI ("Speakers (Realtek(R) Audio)").
fn output_name() -> String {
    com::init();
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

fn capture(target: Target, feed: SharedFeed, stop: Arc<AtomicBool>, ready: mpsc::Sender<Result<(), String>>) {
    com::init();
    let setup = || -> Result<_, wasapi::WasapiError> {
        // `include_tree = false` is PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE: everyone but Talaan.
        let include_tree = !matches!(target, Target::EveryoneButUs);
        let mut client = wasapi::AudioClient::new_application_loopback_client(std::process::id(), include_tree)?;
        let format = wasapi::WaveFormat::new(32, 32, &wasapi::SampleType::Float, RATE as usize, CHANNELS as usize, None);
        let mode = wasapi::StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: BUFFER_HNS };
        client.initialize_client(&format, &wasapi::Direction::Capture, &mode)?;
        let event = client.set_get_eventhandle()?;
        let capture = client.get_audiocaptureclient()?;
        client.start_stream()?;
        Ok((client, event, capture))
    };
    let (client, event, capture) = match setup() {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(format!("Couldn't start system audio capture (needs Windows 10 2004 or later): {e}")));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    let _mmcss = Mmcss::pro_audio();
    // f32-aligned from the start; WASAPI copies bytes into it.
    let mut buf: Vec<f32> = vec![];
    let mut samples: Vec<f32> = Vec::with_capacity(RATE as usize);
    while !stop.load(Ordering::Relaxed) {
        if event.wait_for_event(200).is_err() {
            continue;
        }
        loop {
            let frames = match capture.get_next_packet_size() {
                Ok(Some(n)) if n > 0 => n as usize,
                _ => break,
            };
            buf.resize(frames * CHANNELS as usize, 0.0);
            let Ok((read, info)) = capture.read_from_device(as_bytes_mut(&mut buf)) else { break };
            if read == 0 {
                break;
            }
            packet_samples(&buf[..read as usize * CHANNELS as usize], info.flags.silent, &mut samples);
            // The QPC position is the capture time of the first frame, on the same clock as the mic.
            let host_ns = if info.flags.timestamp_error || info.timestamp == 0 { host_now_ns() } else { hns_to_ns(info.timestamp) };
            if let Ok(mut f) = feed.try_lock() {
                f.push(host_ns, RATE, CHANNELS, &samples);
            }
        }
    }
    let _ = client.stop_stream();
}

fn open(target: Target, feed: &SharedFeed) -> Result<(Live, String), String> {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let (f, s) = (feed.clone(), stop.clone());
    let thread = std::thread::Builder::new().name("talaan-loopback".into()).spawn(move || capture(target, f, s, tx)).map_err(|e| e.to_string())?;
    match rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(Ok(())) => Ok((Live { stop, thread }, output_name())),
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

fn close(live: Live) {
    live.stop.store(true, Ordering::Relaxed);
    let _ = live.thread.join();
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
        let mut out = vec![];
        packet_samples(&[0.5, -0.5, 0.25, 0.0], true, &mut out);
        assert_eq!(out, vec![0.0; 4]);
        packet_samples(&[0.5, -0.5], false, &mut out);
        assert_eq!(out, vec![0.5, -0.5]);
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

    /// The exclusion check: this test process plays a tone while it records. The tone is ours, so the normal
    /// (everyone-but-us) loopback must not hear it, while a control loopback of only our process must.
    /// `cargo test system_excludes_own_audio -- --ignored --nocapture`. Other sound may play (the check is for 440 Hz).
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
}
