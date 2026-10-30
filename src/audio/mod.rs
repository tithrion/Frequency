//! Audio capture and host/device discovery.

pub mod capture;
pub mod host;

pub use capture::{AudioHandle, AudioSelection, AudioStatus};
pub use host::HostInfo;
