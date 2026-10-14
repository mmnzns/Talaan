//! Recording two aligned tracks (mic = "me", system audio = "them"). Cross-platform; OS code lives in `platform/`.

pub mod clock;
pub mod engine;
pub mod feed;
pub mod levels;
pub mod pipeline;
pub mod resample;
pub mod session;
pub mod source;
pub mod timeline;
pub mod wav;
