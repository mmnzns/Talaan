//! Recording two aligned tracks (mic = "me", system audio = "them"). Cross-platform; OS code lives in `platform/`.

pub mod clock;
pub mod levels;
pub mod pipeline;
pub mod resample;
pub mod timeline;
pub mod wav;
