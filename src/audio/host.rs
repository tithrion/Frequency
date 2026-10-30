//! Audio host and input-device discovery.

use cpal::traits::{DeviceTrait, HostTrait};

#[derive(Clone, Debug, Default)]
pub struct HostInfo {
    pub name: String,
    pub devices: Vec<String>,
}

/// Enumerate every compiled-in host and its input devices.
///
/// Enumeration is deliberately fault tolerant: a host such as JACK may be
/// compiled in but unavailable at runtime, and a single failing host must not
/// hide the others.
pub fn scan() -> Vec<HostInfo> {
    let mut result = Vec::new();
    for host_id in cpal::available_hosts() {
        let name = host_id.name().to_string();
        let host = match cpal::host_from_id(host_id) {
            Ok(host) => host,
            Err(error) => {
                log::debug!("audio host '{name}' is unavailable: {error}");
                result.push(HostInfo {
                    name,
                    devices: Vec::new(),
                });
                continue;
            }
        };

        let mut devices = Vec::new();
        match host.input_devices() {
            Ok(iter) => {
                for device in iter {
                    if let Ok(device_name) = device.name() {
                        if !device_name.is_empty() && !devices.contains(&device_name) {
                            devices.push(device_name);
                        }
                    }
                }
            }
            Err(error) => {
                log::debug!("could not enumerate input devices for '{name}': {error}");
            }
        }

        if let Some(default) = host.default_input_device() {
            if let Ok(default_name) = default.name() {
                if !default_name.is_empty() && !devices.contains(&default_name) {
                    devices.insert(0, default_name);
                }
            }
        }

        result.push(HostInfo { name, devices });
    }
    result
}
