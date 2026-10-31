//! Audio host and input-device discovery.

use cpal::traits::{DeviceTrait, HostTrait};

/// Label used by the driver drop-down for automatic host selection.
pub const AUTO_HOST: &str = "Automatic";

#[derive(Clone, Debug, Default)]
pub struct HostInfo {
    pub name: String,
    pub devices: Vec<String>,
}

impl HostInfo {
    /// A host is usable when it exposes at least one input device.
    pub fn is_usable(&self) -> bool {
        !self.devices.is_empty()
    }
}

/// Whether `libjack` can be loaded on this machine.
///
/// `jack-sys` uses dynamic loading, but touching any JACK function panics when
/// the shared library is missing. Probe once so a build with `--features jack`
/// degrades to ALSA/PipeWire instead of crashing on systems without JACK.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_runtime_available() -> bool {
    use std::sync::OnceLock;

    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(
        || match unsafe { libloading::Library::new("libjack.so.0") } {
            Ok(_library) => true,
            Err(error) => {
                log::debug!("libjack.so.0 could not be loaded: {error}");
                false
            }
        },
    )
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn jack_runtime_available() -> bool {
    true
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
        // A binary built with `--features jack` loads libjack dynamically. If
        // the library is missing, even constructing the JACK host would panic,
        // so hide the host entirely and let the automatic selection fall back.
        if name.eq_ignore_ascii_case("JACK") && !jack_runtime_available() {
            log::debug!("audio host 'JACK' skipped: libjack is not installed");
            continue;
        }
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

/// True when a JACK server answered during host enumeration.
///
/// CPAL only creates JACK devices after a client connection to the server
/// succeeds, so a `JACK` host without devices means that no server is running.
/// This makes the regular [`scan`] double as the startup test on Linux builds
/// compiled with `--features jack`.
pub fn jack_server_running(hosts: &[HostInfo]) -> bool {
    hosts
        .iter()
        .any(|host| host.name.eq_ignore_ascii_case("JACK") && host.is_usable())
}

/// Choose the default host when the user lets Frequency decide.
///
/// JACK wins when its server is reachable; otherwise ALSA (which also carries
/// PulseAudio and PipeWire devices) is preferred, with any usable host as the
/// last resort.
pub fn automatic_host(hosts: &[HostInfo]) -> Option<&HostInfo> {
    hosts
        .iter()
        .find(|host| host.name.eq_ignore_ascii_case("JACK") && host.is_usable())
        .or_else(|| {
            hosts
                .iter()
                .find(|host| host.name == "ALSA" && host.is_usable())
        })
        .or_else(|| hosts.iter().find(|host| host.is_usable()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(name: &str, devices: &[&str]) -> HostInfo {
        HostInfo {
            name: name.to_string(),
            devices: devices.iter().map(|device| device.to_string()).collect(),
        }
    }

    #[test]
    fn jack_is_preferred_while_its_server_is_running() {
        let hosts = vec![
            host("ALSA", &["default"]),
            host("JACK", &["cpal_client_in"]),
        ];
        assert!(jack_server_running(&hosts));
        assert_eq!(
            automatic_host(&hosts).map(|host| host.name.as_str()),
            Some("JACK")
        );
    }

    #[test]
    fn alsa_is_used_when_jack_is_compiled_in_but_not_running() {
        let hosts = vec![host("ALSA", &["default"]), host("JACK", &[])];
        assert!(!jack_server_running(&hosts));
        assert_eq!(
            automatic_host(&hosts).map(|host| host.name.as_str()),
            Some("ALSA")
        );
    }

    #[test]
    fn automatic_host_falls_back_to_the_first_usable_host() {
        let hosts = vec![host("WASAPI", &["Microphone"]), host("ALSA", &[])];
        assert_eq!(
            automatic_host(&hosts).map(|host| host.name.as_str()),
            Some("WASAPI")
        );
        assert!(automatic_host(&[]).is_none());
    }
}
