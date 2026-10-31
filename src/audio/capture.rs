//! CPAL capture with a dedicated stream thread and a lock-free sample ring.

use crate::dsp::AudioFeed;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    Device, FromSample, SampleFormat, SampleRate, SizedSample, Stream, StreamConfig,
    SupportedStreamConfig,
};
use rtrb::{Producer, RingBuffer};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const RING_CAPACITY: usize = 1 << 16;
const RETRY_INTERVAL: Duration = Duration::from_secs(2);

/// Requested capture source. `None` means the system default.
#[derive(Clone, Debug, Default)]
pub struct AudioSelection {
    pub host: Option<String>,
    pub device: Option<String>,
    pub preferred_sample_rate: u32,
}

/// Observable state of the capture stream.
#[derive(Clone, Debug)]
pub struct AudioStatus {
    pub host: String,
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub running: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
}

impl Default for AudioStatus {
    fn default() -> Self {
        Self {
            host: "System default".to_string(),
            device: String::new(),
            sample_rate: 0,
            channels: 0,
            running: false,
            error: None,
            notice: None,
        }
    }
}

enum AudioCommand {
    Select(AudioSelection),
    Shutdown,
}

/// Handle used by the GUI to switch devices and inspect stream state.
pub struct AudioHandle {
    command_tx: Sender<AudioCommand>,
    status: Arc<Mutex<AudioStatus>>,
    thread: Option<JoinHandle<()>>,
}

impl AudioHandle {
    pub fn spawn(selection: AudioSelection, feed_tx: Sender<AudioFeed>) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(AudioStatus::default()));
        let thread_status = Arc::clone(&status);
        let thread = thread::Builder::new()
            .name("frequency-audio".to_string())
            .spawn(move || audio_loop(selection, feed_tx, command_rx, thread_status))
            .expect("failed to spawn audio thread");

        Self {
            command_tx,
            status,
            thread: Some(thread),
        }
    }

    pub fn select(&self, selection: AudioSelection) {
        let _ = self.command_tx.send(AudioCommand::Select(selection));
    }

    pub fn status(&self) -> Arc<Mutex<AudioStatus>> {
        Arc::clone(&self.status)
    }

    pub fn snapshot(&self) -> AudioStatus {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Drop for AudioHandle {
    fn drop(&mut self) {
        let _ = self.command_tx.send(AudioCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn audio_loop(
    mut selection: AudioSelection,
    feed_tx: Sender<AudioFeed>,
    command_rx: Receiver<AudioCommand>,
    status: Arc<Mutex<AudioStatus>>,
) {
    let mut stream = build_stream(&selection, &feed_tx, &status);
    let mut last_attempt = Instant::now();

    loop {
        match command_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(AudioCommand::Select(new_selection)) => {
                selection = new_selection;
                drop(stream.take());
                stream = build_stream(&selection, &feed_tx, &status);
                last_attempt = Instant::now();
            }
            Ok(AudioCommand::Shutdown) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if stream.is_none() && last_attempt.elapsed() >= RETRY_INTERVAL {
                    last_attempt = Instant::now();
                    stream = build_stream(&selection, &feed_tx, &status);
                }
            }
        }
    }

    drop(stream);
    set_status(&status, |state| {
        state.running = false;
    });
}

fn set_status(status: &Arc<Mutex<AudioStatus>>, update: impl FnOnce(&mut AudioStatus)) {
    let mut state = status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    update(&mut state);
}

fn build_stream(
    selection: &AudioSelection,
    feed_tx: &Sender<AudioFeed>,
    status: &Arc<Mutex<AudioStatus>>,
) -> Option<Stream> {
    let mut notice: Option<String> = None;

    // Resolve the host. An explicitly requested host that is unavailable falls
    // back to the system default instead of taking the application down.
    let requested_host = selection.host.as_deref();
    let host_id = requested_host.and_then(|requested| {
        cpal::available_hosts()
            .into_iter()
            .find(|id| id.name().eq_ignore_ascii_case(requested))
    });
    let mut host_label = "System default".to_string();
    let host = match host_id {
        Some(id) => match cpal::host_from_id(id) {
            Ok(host) => {
                if let Some(requested) = requested_host {
                    host_label = requested.to_string();
                }
                host
            }
            Err(error) => {
                notice = Some(format!(
                    "audio host '{}' unavailable: {error}",
                    requested_host.unwrap_or("unknown")
                ));
                cpal::default_host()
            }
        },
        None => {
            if let Some(requested) = requested_host {
                notice = Some(format!("audio host '{requested}' not found; using default"));
            }
            cpal::default_host()
        }
    };

    // Resolve the device by name, then fall back to the default input.
    let requested_device = selection.device.as_deref();
    let mut matched_device = None;
    if let Some(requested) = requested_device {
        if let Ok(devices) = host.input_devices() {
            matched_device = devices
                .into_iter()
                .find(|device| device.name().map(|name| name == requested).unwrap_or(false));
        }
    }
    let matched_requested_device = matched_device.is_some();
    let device = matched_device.or_else(|| host.default_input_device());
    let Some(device) = device else {
        set_status(status, |state| {
            state.host = host_label;
            state.device = requested_device.unwrap_or("").to_string();
            state.running = false;
            state.error = Some("no audio input device is available".to_string());
            state.notice = notice;
        });
        return None;
    };

    let device_name = device
        .name()
        .unwrap_or_else(|_| "Unknown input".to_string());
    if requested_device.is_some() && !matched_requested_device {
        let extra = format!(
            "input '{requested}' not found; using '{device_name}'",
            requested = requested_device.unwrap_or_default()
        );
        notice = Some(match notice {
            Some(existing) => format!("{existing}; {extra}"),
            None => extra,
        });
    }

    let config = match choose_config(&device, selection.preferred_sample_rate) {
        Ok(config) => config,
        Err(error) => {
            set_status(status, |state| {
                state.host = host_label;
                state.device = device_name;
                state.running = false;
                state.error = Some(error);
                state.notice = notice;
            });
            return None;
        }
    };

    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let sample_format = config.sample_format();
    let stream_config: StreamConfig = config.clone().into();
    let (producer, consumer) = RingBuffer::<f32>::new(RING_CAPACITY);

    set_status(status, |state| {
        state.host = host_label.clone();
        state.device = device_name.clone();
        state.sample_rate = sample_rate;
        state.channels = channels;
        state.running = false;
        state.error = None;
        state.notice = notice.clone();
    });

    let error_status = Arc::clone(status);
    let error_callback = move |error: cpal::StreamError| {
        log::error!("audio stream error: {error}");
        set_status(&error_status, |state| {
            state.running = false;
            state.error = Some(format!("audio stream error: {error}"));
        });
    };

    let build_result = match sample_format {
        SampleFormat::F32 => build_input_stream::<f32>(
            &device,
            &stream_config,
            channels as usize,
            producer,
            error_callback,
        ),
        SampleFormat::F64 => build_input_stream::<f64>(
            &device,
            &stream_config,
            channels as usize,
            producer,
            error_callback,
        ),
        SampleFormat::I8 => build_input_stream::<i8>(
            &device,
            &stream_config,
            channels as usize,
            producer,
            error_callback,
        ),
        SampleFormat::I16 => build_input_stream::<i16>(
            &device,
            &stream_config,
            channels as usize,
            producer,
            error_callback,
        ),
        SampleFormat::I32 => build_input_stream::<i32>(
            &device,
            &stream_config,
            channels as usize,
            producer,
            error_callback,
        ),
        SampleFormat::I64 => build_input_stream::<i64>(
            &device,
            &stream_config,
            channels as usize,
            producer,
            error_callback,
        ),
        SampleFormat::U8 => build_input_stream::<u8>(
            &device,
            &stream_config,
            channels as usize,
            producer,
            error_callback,
        ),
        SampleFormat::U16 => build_input_stream::<u16>(
            &device,
            &stream_config,
            channels as usize,
            producer,
            error_callback,
        ),
        SampleFormat::U32 => build_input_stream::<u32>(
            &device,
            &stream_config,
            channels as usize,
            producer,
            error_callback,
        ),
        SampleFormat::U64 => build_input_stream::<u64>(
            &device,
            &stream_config,
            channels as usize,
            producer,
            error_callback,
        ),
        other => {
            set_status(status, |state| {
                state.running = false;
                state.error = Some(format!("unsupported sample format: {other:?}"));
            });
            return None;
        }
    };

    let stream = match build_result {
        Ok(stream) => stream,
        Err(error) => {
            set_status(status, |state| {
                state.running = false;
                state.error = Some(format!("could not open input stream: {error}"));
            });
            return None;
        }
    };

    if let Err(error) = stream.play() {
        set_status(status, |state| {
            state.running = false;
            state.error = Some(format!("could not start input stream: {error}"));
        });
        return None;
    }

    if let Err(error) = feed_tx.send(AudioFeed {
        consumer,
        sample_rate: sample_rate as f32,
        channels,
    }) {
        set_status(status, |state| {
            state.running = false;
            state.error = Some(format!("DSP worker is not running: {error}"));
        });
        return None;
    }

    set_status(status, |state| {
        state.running = true;
        state.error = None;
    });
    log::info!(
        "capture started: host='{}', device='{}', {} Hz, {} channel(s), {:?}",
        host_label,
        device_name,
        sample_rate,
        channels,
        sample_format
    );

    Some(stream)
}

fn choose_config(device: &Device, preferred_rate: u32) -> Result<SupportedStreamConfig, String> {
    let default = device
        .default_input_config()
        .map_err(|error| format!("could not query default input configuration: {error}"))?;

    let Ok(ranges) = device.supported_input_configs() else {
        return Ok(default);
    };
    let ranges: Vec<_> = ranges.collect();
    if ranges.is_empty() {
        return Ok(default);
    }

    let preferred_rates = [
        preferred_rate,
        if preferred_rate == 44_100 {
            48_000
        } else {
            44_100
        },
        48_000,
        44_100,
    ];

    for rate in preferred_rates {
        let within = |range: &&cpal::SupportedStreamConfigRange| {
            range.min_sample_rate().0 <= rate && rate <= range.max_sample_rate().0
        };
        if let Some(range) = ranges
            .iter()
            .filter(within)
            .find(|range| range.sample_format() == SampleFormat::F32)
        {
            return Ok(range.with_sample_rate(SampleRate(rate)));
        }
        if let Some(range) = ranges.iter().find(within) {
            return Ok(range.with_sample_rate(SampleRate(rate)));
        }
    }

    Ok(default)
}

fn build_input_stream<T>(
    device: &Device,
    config: &StreamConfig,
    channels: usize,
    mut producer: Producer<f32>,
    error_callback: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = channels.max(1);
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            for frame in data.chunks(channels) {
                let mut sum = 0.0_f32;
                for sample in frame {
                    sum += <f32 as FromSample<T>>::from_sample_(*sample);
                }
                let mono = sum / frame.len() as f32;
                let _ = producer.push(mono);
            }
        },
        error_callback,
        None,
    )
}
