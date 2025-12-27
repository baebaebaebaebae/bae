mod cpal_output;
mod pcm_source;
pub mod progress;
pub mod reassembly; // Public for tests and internal use
pub mod service;

pub use pcm_source::PcmSource;

pub use progress::PlaybackProgress;
#[cfg(feature = "test-utils")]
#[allow(unused_imports)] // Used in tests
pub use reassembly::reassemble_track;
pub use service::{PlaybackHandle, PlaybackService, PlaybackState};
