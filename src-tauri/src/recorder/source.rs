//! Where audio comes from. The mic uses cpal (all platforms); system audio is OS-specific (`platform/`).

use super::feed::FeedProducer;
use std::sync::{Arc, Mutex};

/// The callback side shares the producer through a mutex it only `try_lock`s, so a device switch can
/// hand the same producer to a new stream without blocking the audio thread.
pub type SharedFeed = Arc<Mutex<FeedProducer>>;

pub trait AudioSource: Send {
    /// Starts delivering buffers into `feed`. Returns a short device name for the UI.
    fn start(&mut self, feed: SharedFeed) -> Result<String, String>;
    /// Rebuilds on the current default device (after a device change). Returns the new device name.
    fn restart(&mut self) -> Result<String, String>;
    fn stop(&mut self);
    /// True once the source stopped delivering for good on its own (it gave up reconnecting).
    fn failed(&self) -> bool {
        false
    }
}
