//! Host-side encodings shared by fixtures and AWS F1 runners.
#[cfg(feature = "aws-f1-sdk")]
pub mod aws;
pub mod encoding;
pub mod queue;
