//! Realtime-safe handoff from an audio callback to its writer thread: no locks, no allocation on push.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    /// Capture time of the first frame, in host nanoseconds (mach time on macOS).
    pub host_ns: u64,
    pub rate: u32,
    pub channels: u16,
    pub frames: u32,
}

pub struct FeedProducer {
    headers: rtrb::Producer<Header>,
    samples: rtrb::Producer<f32>,
    dropped: Arc<AtomicU64>,
}

pub struct FeedConsumer {
    headers: rtrb::Consumer<Header>,
    samples: rtrb::Consumer<f32>,
    #[allow(dead_code)] // read by diagnostics and tests
    dropped: Arc<AtomicU64>,
}

/// About 20 s of 48 kHz stereo.
pub fn feed() -> (FeedProducer, FeedConsumer) {
    feed_with_capacity(2_000_000, 8_192)
}

pub fn feed_with_capacity(samples: usize, headers: usize) -> (FeedProducer, FeedConsumer) {
    let (hp, hc) = rtrb::RingBuffer::new(headers);
    let (sp, sc) = rtrb::RingBuffer::new(samples);
    let dropped = Arc::new(AtomicU64::new(0));
    (FeedProducer { headers: hp, samples: sp, dropped: dropped.clone() }, FeedConsumer { headers: hc, samples: sc, dropped })
}

impl FeedProducer {
    /// Called from the audio callback. If the writer fell behind, the buffer is dropped (the timeline turns it into silence).
    pub fn push(&mut self, host_ns: u64, rate: u32, channels: u16, interleaved: &[f32]) -> bool {
        if self.headers.slots() < 1 || self.samples.slots() < interleaved.len() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let frames = (interleaved.len() / channels.max(1) as usize) as u32;
        let _ = self.samples.push_entire_slice(interleaved);
        let _ = self.headers.push(Header { host_ns, rate, channels, frames });
        true
    }
}

impl FeedConsumer {
    /// Next buffer, written into `buf` (reused to avoid allocation).
    pub fn pop(&mut self, buf: &mut Vec<f32>) -> Option<Header> {
        let h = self.headers.pop().ok()?;
        let n = h.frames as usize * h.channels.max(1) as usize;
        buf.resize(n, 0.0);
        let _ = self.samples.pop_entire_slice(buf);
        Some(h)
    }

    pub fn is_empty(&self) -> bool {
        self.headers.slots() == 0
    }

    #[allow(dead_code)]
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffers_arrive_in_order_with_their_headers() {
        let (mut p, mut c) = feed_with_capacity(100, 4);
        assert!(p.push(10, 48_000, 2, &[1.0, 2.0, 3.0, 4.0]));
        assert!(p.push(20, 48_000, 1, &[5.0]));
        let mut buf = vec![];
        assert_eq!(c.pop(&mut buf), Some(Header { host_ns: 10, rate: 48_000, channels: 2, frames: 2 }));
        assert_eq!(buf, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(c.pop(&mut buf).unwrap().host_ns, 20);
        assert_eq!(buf, vec![5.0]);
        assert!(c.pop(&mut buf).is_none());
        assert!(c.is_empty());
    }

    #[test]
    fn a_full_feed_drops_and_counts() {
        let (mut p, c) = feed_with_capacity(4, 4);
        assert!(p.push(0, 16_000, 1, &[0.0; 4]));
        assert!(!p.push(1, 16_000, 1, &[0.0; 1]));
        assert_eq!(c.dropped(), 1);
    }
}
