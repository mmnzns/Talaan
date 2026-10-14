//! System audio ("them") through a Core Audio global process tap (macOS 14.2+).
//!
//! A tap of every process except Talaan feeds a private aggregate device; an IOProc on that device
//! copies the audio into the feed. When the default output changes, the aggregate is rebuilt.

use super::clock::{host_now_ns, host_ticks_to_ns};
use super::coreaudio::*;
use crate::recorder::source::{AudioSource, SharedFeed};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AllocAnyThread;
use objc2_core_audio::*;
use objc2_core_audio_types::{kAudioFormatFlagIsFloat, AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};
use std::ffi::CStr;
use std::ptr::NonNull;

type IoBlock = RcBlock<dyn Fn(NonNull<AudioTimeStamp>, NonNull<AudioBufferList>, NonNull<AudioTimeStamp>, NonNull<AudioBufferList>, NonNull<AudioTimeStamp>)>;

struct Live {
    tap: AudioObjectID,
    aggregate: AudioObjectID,
    proc_id: AudioDeviceIOProcID,
    _block: IoBlock,
}

#[derive(Default)]
pub struct SystemTap {
    feed: Option<SharedFeed>,
    live: Option<Live>,
}

// The block and IDs are only touched from the thread that owns the recorder, and the IOProc is stopped
// before anything is released.
unsafe impl Send for SystemTap {}

fn key(k: &CStr) -> Retained<NSString> {
    NSString::from_str(k.to_str().unwrap_or_default())
}

fn dict(pairs: Vec<(Retained<NSString>, Retained<AnyObject>)>) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<&NSString> = pairs.iter().map(|(k, _)| &**k).collect();
    let values: Vec<&AnyObject> = pairs.iter().map(|(_, v)| &**v).collect();
    NSDictionary::from_slices(&keys, &values)
}

fn obj<T: objc2::Message>(v: Retained<T>) -> Retained<AnyObject> {
    unsafe { Retained::cast_unchecked(v) }
}

fn open(feed: &SharedFeed) -> Result<(Live, String), String> {
    unsafe {
        // 1. The tap: everything except Talaan, mixed to mono.
        let exclude: Vec<Retained<NSNumber>> = own_process_object().map(|p| vec![NSNumber::new_u32(p)]).unwrap_or_default();
        let exclude = NSArray::from_retained_slice(&exclude);
        let desc = CATapDescription::initMonoGlobalTapButExcludeProcesses(CATapDescription::alloc(), &exclude);
        desc.setPrivate(true);
        desc.setMuteBehavior(CATapMuteBehavior::Unmuted);
        desc.setName(&NSString::from_str("Talaan"));
        let tap_uuid = desc.UUID().UUIDString().to_string();
        let mut tap: AudioObjectID = 0;
        check(AudioHardwareCreateProcessTap(Some(&desc), &mut tap), "AudioHardwareCreateProcessTap")?;

        // 2. A private aggregate device around the current output device, with the tap as its input.
        let output = default_output_device()?;
        let output_uid = device_uid(output)?;
        let output_name = device_name(output);
        let sub_device = dict(vec![(key(kAudioSubDeviceUIDKey), obj(NSString::from_str(&output_uid)))]);
        let sub_tap = dict(vec![(key(kAudioSubTapDriftCompensationKey), obj(NSNumber::new_bool(true))), (key(kAudioSubTapUIDKey), obj(NSString::from_str(&tap_uuid)))]);
        let description = dict(vec![
            (key(kAudioAggregateDeviceNameKey), obj(NSString::from_str("Talaan System Audio"))),
            (key(kAudioAggregateDeviceUIDKey), obj(NSString::from_str(&format!("talaan-tap-{tap_uuid}")))),
            (key(kAudioAggregateDeviceMainSubDeviceKey), obj(NSString::from_str(&output_uid))),
            (key(kAudioAggregateDeviceIsPrivateKey), obj(NSNumber::new_bool(true))),
            (key(kAudioAggregateDeviceIsStackedKey), obj(NSNumber::new_bool(false))),
            (key(kAudioAggregateDeviceTapAutoStartKey), obj(NSNumber::new_bool(true))),
            (key(kAudioAggregateDeviceSubDeviceListKey), obj(NSArray::from_retained_slice(&[sub_device]))),
            (key(kAudioAggregateDeviceTapListKey), obj(NSArray::from_retained_slice(&[sub_tap]))),
        ]);
        let cf: &CFDictionary = &*(Retained::as_ptr(&description) as *const CFDictionary);
        let mut aggregate: AudioObjectID = 0;
        if let Err(e) = check(AudioHardwareCreateAggregateDevice(cf, NonNull::from(&mut aggregate)), "AudioHardwareCreateAggregateDevice") {
            AudioHardwareDestroyProcessTap(tap);
            return Err(e);
        }

        // 3. The tap's format (follows the output device).
        let format: AudioStreamBasicDescription = match get(tap, kAudioTapPropertyFormat, kAudioObjectPropertyScopeGlobal) {
            Ok(f) => f,
            Err(e) => {
                AudioHardwareDestroyAggregateDevice(aggregate);
                AudioHardwareDestroyProcessTap(tap);
                return Err(e);
            }
        };
        if format.mFormatFlags & kAudioFormatFlagIsFloat == 0 {
            AudioHardwareDestroyAggregateDevice(aggregate);
            AudioHardwareDestroyProcessTap(tap);
            return Err(format!("Unexpected system audio format (flags {:#x})", format.mFormatFlags));
        }
        let rate = format.mSampleRate as u32;

        // 4. The IOProc: copy input buffers into the feed. No allocation, no blocking.
        let feed = feed.clone();
        let block: IoBlock = RcBlock::new(move |_now: NonNull<AudioTimeStamp>, input: NonNull<AudioBufferList>, input_time: NonNull<AudioTimeStamp>, _out: NonNull<AudioBufferList>, _out_time: NonNull<AudioTimeStamp>| {
            let abl = input.as_ref();
            if abl.mNumberBuffers == 0 {
                return;
            }
            let buf = abl.mBuffers[0];
            if buf.mData.is_null() || buf.mDataByteSize == 0 {
                return;
            }
            let data = std::slice::from_raw_parts(buf.mData as *const f32, buf.mDataByteSize as usize / 4);
            let ticks = input_time.as_ref().mHostTime;
            let host_ns = if ticks == 0 { host_now_ns() } else { host_ticks_to_ns(ticks) };
            if let Ok(mut f) = feed.try_lock() {
                f.push(host_ns, rate, buf.mNumberChannels.max(1) as u16, data);
            }
        });
        let mut proc_id: AudioDeviceIOProcID = None;
        let block_ptr = RcBlock::as_ptr(&block) as AudioDeviceIOBlock;
        if let Err(e) = check(AudioDeviceCreateIOProcIDWithBlock(NonNull::from(&mut proc_id), aggregate, None, block_ptr), "AudioDeviceCreateIOProcIDWithBlock") {
            AudioHardwareDestroyAggregateDevice(aggregate);
            AudioHardwareDestroyProcessTap(tap);
            return Err(e);
        }
        if let Err(e) = check(AudioDeviceStart(aggregate, proc_id), "AudioDeviceStart") {
            AudioDeviceDestroyIOProcID(aggregate, proc_id);
            AudioHardwareDestroyAggregateDevice(aggregate);
            AudioHardwareDestroyProcessTap(tap);
            return Err(e);
        }
        Ok((Live { tap, aggregate, proc_id, _block: block }, output_name))
    }
}

fn close(live: Live) {
    unsafe {
        AudioDeviceStop(live.aggregate, live.proc_id);
        AudioDeviceDestroyIOProcID(live.aggregate, live.proc_id);
        AudioHardwareDestroyAggregateDevice(live.aggregate);
        AudioHardwareDestroyProcessTap(live.tap);
    }
}

impl AudioSource for SystemTap {
    fn start(&mut self, feed: SharedFeed) -> Result<String, String> {
        self.feed = Some(feed.clone());
        let (live, name) = open(&feed)?;
        self.live = Some(live);
        Ok(name)
    }

    fn restart(&mut self) -> Result<String, String> {
        if let Some(live) = self.live.take() {
            close(live);
        }
        let feed = self.feed.clone().ok_or("System audio not started")?;
        let (live, name) = open(&feed)?;
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
    use crate::recorder::engine::Engine;
    use crate::recorder::mic::MicSource;
    use crate::storage::Storage;

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
        println!("mic: {} · system audio: {}", st.mic_name, st.system_audio);
        let mut peak = (0.0f32, 0.0f32);
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let (a, b) = e.levels();
            peak = (peak.0.max(a), peak.1.max(b));
        }
        let id = e.stop(&storage, host_now_ns()).unwrap();
        let d = storage.dir(&id).unwrap();
        println!("wrote {} · peak me {:.2} · peak them {:.2}", d.display(), peak.0, peak.1);
    }
}
