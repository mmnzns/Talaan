//! "Talaan is quitting": set before the helpers are shut down (quit, or an update about to replace them), so the
//! workers can tell a job that ended because of that from one that really failed, and leave its meeting as it is
//! (it resumes at the next launch, as in 1.1.1) instead of marking it failed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

static QUITTING: OnceLock<Arc<AtomicBool>> = OnceLock::new();

/// The process-wide flag, for the workers.
pub fn flag() -> Arc<AtomicBool> {
    QUITTING.get_or_init(Arc::default).clone()
}

/// Talaan is going away now.
pub fn begin() {
    flag().store(true, Ordering::SeqCst);
}
