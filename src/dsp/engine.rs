//! The worker thread that turns the lock-free sample feed into smoothed pitch
//! readings for the GUI.

use crate::dsp::pitch::{self, PitchMethod};
use crate::music::cents_between;
use rtrb::Consumer;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// A newly-created capture stream handed from the audio thread to the DSP
/// thread. Switching devices replaces the ring-buffer consumer.
pub struct AudioFeed {
    pub consumer: Consumer<f32>,
    pub sample_rate: f32,
    pub channels: u16,
}

/// Latest result published by the DSP thread.
#[derive(Clone, Debug)]
pub struct Analysis {
    /// Un-smoothed frequency from the detector.
    pub raw_frequency: Option<f32>,
    /// Pitch after the exponential inertia filter.
    pub frequency: Option<f32>,
    pub confidence: f32,
    pub rms: f32,
    pub method: Option<PitchMethod>,
    pub sample_rate: f32,
    pub buffer_size: usize,
    pub updated_at: Option<Instant>,
}

impl Default for Analysis {
    fn default() -> Self {
        Self {
            raw_frequency: None,
            frequency: None,
            confidence: 0.0,
            rms: 0.0,
            method: None,
            sample_rate: 0.0,
            buffer_size: 0,
            updated_at: None,
        }
    }
}

impl Analysis {
    pub fn has_signal(&self) -> bool {
        self.frequency.is_some()
    }

    pub fn is_fresh(&self, max_age: Duration) -> bool {
        self.updated_at
            .is_some_and(|updated| updated.elapsed() <= max_age)
    }
}

/// Shared, lock-free control knobs for the DSP worker.
#[derive(Debug)]
pub struct DspControl {
    alpha: AtomicU32,
    window_size: AtomicUsize,
    min_rms: AtomicU32,
    stop: AtomicBool,
}

impl Default for DspControl {
    fn default() -> Self {
        Self {
            alpha: AtomicU32::new(0.25_f32.to_bits()),
            window_size: AtomicUsize::new(4096),
            min_rms: AtomicU32::new(0.001_f32.to_bits()),
            stop: AtomicBool::new(false),
        }
    }
}

impl DspControl {
    pub fn alpha(&self) -> f32 {
        f32::from_bits(self.alpha.load(Ordering::Relaxed))
    }

    pub fn set_alpha(&self, value: f32) {
        let value = if value.is_finite() {
            value.clamp(0.05, 0.5)
        } else {
            0.25
        };
        self.alpha.store(value.to_bits(), Ordering::Relaxed);
    }

    pub fn window_size(&self) -> usize {
        self.window_size.load(Ordering::Relaxed)
    }

    pub fn set_window_size(&self, value: usize) {
        self.window_size
            .store(value.clamp(256, 16_384), Ordering::Relaxed);
    }

    pub fn min_rms(&self) -> f32 {
        f32::from_bits(self.min_rms.load(Ordering::Relaxed))
    }

    pub fn set_min_rms(&self, value: f32) {
        let value = if value.is_finite() {
            value.clamp(0.0, 0.1)
        } else {
            0.001
        };
        self.min_rms.store(value.to_bits(), Ordering::Relaxed);
    }
}

/// Handle to the DSP worker thread and its latest analysis.
pub struct AnalyzerHandle {
    snapshot: Arc<RwLock<Analysis>>,
    control: Arc<DspControl>,
    thread: Option<JoinHandle<()>>,
}

impl AnalyzerHandle {
    pub fn spawn(feed_receiver: Receiver<AudioFeed>) -> Self {
        let snapshot = Arc::new(RwLock::new(Analysis::default()));
        let control = Arc::new(DspControl::default());
        let thread_snapshot = Arc::clone(&snapshot);
        let thread_control = Arc::clone(&control);
        let thread = thread::Builder::new()
            .name("frequency-dsp".to_string())
            .spawn(move || run(feed_receiver, thread_snapshot, thread_control))
            .expect("failed to spawn DSP thread");

        Self {
            snapshot,
            control,
            thread: Some(thread),
        }
    }

    pub fn snapshot(&self) -> Arc<RwLock<Analysis>> {
        Arc::clone(&self.snapshot)
    }

    pub fn latest(&self) -> Analysis {
        self.snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn control(&self) -> Arc<DspControl> {
        Arc::clone(&self.control)
    }

    pub fn set_alpha(&self, alpha: f32) {
        self.control.set_alpha(alpha);
    }

    pub fn set_window_size(&self, window_size: usize) {
        self.control.set_window_size(window_size);
    }

    pub fn set_min_rms(&self, min_rms: f32) {
        self.control.set_min_rms(min_rms);
    }
}

impl Drop for AnalyzerHandle {
    fn drop(&mut self) {
        self.control.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

const MAX_BUFFER: usize = 16_384;
const ANALYSIS_INTERVAL: Duration = Duration::from_millis(25);
const IDLE_SLEEP: Duration = Duration::from_millis(4);
const SIGNAL_LOST_AFTER: Duration = Duration::from_millis(350);
const TARGET_JUMP_CENTS: f32 = 100.0;

fn run(
    feed_receiver: Receiver<AudioFeed>,
    snapshot: Arc<RwLock<Analysis>>,
    control: Arc<DspControl>,
) {
    let mut feed: Option<AudioFeed> = None;
    let mut samples = VecDeque::with_capacity(MAX_BUFFER);
    let mut smoothed: Option<f32> = None;
    let mut last_analysis = Instant::now() - ANALYSIS_INTERVAL;
    let mut last_pitch = Instant::now();

    while !control.stop.load(Ordering::Relaxed) {
        while let Ok(new_feed) = feed_receiver.try_recv() {
            log::info!(
                "DSP attached to stream: {:.0} Hz, {} channel(s)",
                new_feed.sample_rate,
                new_feed.channels
            );
            let sample_rate = new_feed.sample_rate;
            feed = Some(new_feed);
            samples.clear();
            smoothed = None;
            last_pitch = Instant::now();
            last_analysis = Instant::now() - ANALYSIS_INTERVAL;
            if let Ok(mut shared) = snapshot.write() {
                *shared = Analysis {
                    sample_rate,
                    updated_at: Some(Instant::now()),
                    ..Analysis::default()
                };
            }
        }

        let Some(active_feed) = feed.as_ref() else {
            thread::sleep(IDLE_SLEEP);
            continue;
        };

        // When the capture stream is dropped (device switch or failure), the
        // producer disappears. Clear the reading instead of showing a stale
        // pitch until the next stream attaches.
        if active_feed.consumer.is_abandoned() {
            let sample_rate = active_feed.sample_rate;
            let had_signal = snapshot
                .read()
                .map(|analysis| analysis.frequency.is_some() || analysis.rms > 0.0)
                .unwrap_or(true);
            if had_signal {
                let analysis = Analysis {
                    sample_rate,
                    updated_at: Some(Instant::now()),
                    ..Analysis::default()
                };
                if let Ok(mut shared) = snapshot.write() {
                    *shared = analysis;
                }
            }
            feed = None;
            samples.clear();
            smoothed = None;
            thread::sleep(IDLE_SLEEP);
            continue;
        }

        let Some(active_feed) = feed.as_mut() else {
            thread::sleep(IDLE_SLEEP);
            continue;
        };

        let mut received = false;
        for _ in 0..MAX_BUFFER {
            match active_feed.consumer.pop() {
                Ok(sample) => {
                    samples.push_back(sample);
                    received = true;
                    if samples.len() > MAX_BUFFER {
                        samples.pop_front();
                    }
                }
                Err(_) => break,
            }
        }

        if received && last_analysis.elapsed() >= ANALYSIS_INTERVAL && samples.len() >= 128 {
            last_analysis = Instant::now();
            let window_size = control.window_size().min(samples.len()).max(128);
            let start = samples.len().saturating_sub(window_size);
            let window: Vec<f32> = samples.iter().skip(start).copied().collect();
            let level = pitch::rms(&window);
            let min_rms = control.min_rms();

            let estimate = if level >= min_rms {
                pitch::detect(&window, active_feed.sample_rate)
            } else {
                None
            };

            let mut raw_frequency = None;
            let mut method = None;
            let mut confidence = 0.0;

            if let Some(estimate) = estimate {
                raw_frequency = Some(estimate.frequency);
                method = Some(estimate.method);
                confidence = estimate.confidence;

                let previous = smoothed.unwrap_or(estimate.frequency);
                let jump = cents_between(estimate.frequency, previous).abs() > TARGET_JUMP_CENTS;
                let alpha = if jump { 1.0 } else { control.alpha() };
                smoothed = Some(alpha * estimate.frequency + (1.0 - alpha) * previous);
                last_pitch = Instant::now();
            } else if level < min_rms || last_pitch.elapsed() >= SIGNAL_LOST_AFTER {
                smoothed = None;
            }

            let analysis = Analysis {
                raw_frequency,
                frequency: smoothed,
                confidence,
                rms: level,
                method,
                sample_rate: active_feed.sample_rate,
                buffer_size: window_size,
                updated_at: Some(Instant::now()),
            };
            if let Ok(mut shared) = snapshot.write() {
                *shared = analysis;
            }
        }

        if !received {
            thread::sleep(IDLE_SLEEP);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_values_are_clamped() {
        let control = DspControl::default();
        control.set_alpha(5.0);
        assert_eq!(control.alpha(), 0.5);
        control.set_window_size(2);
        assert_eq!(control.window_size(), 256);
        control.set_min_rms(-1.0);
        assert_eq!(control.min_rms(), 0.0);
    }
}
