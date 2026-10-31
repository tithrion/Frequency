//! Frequency — a fast, accurate, cross-platform musical instrument tuner.
//!
//! The crate is split into four layers:
//!
//! * [`audio`] — CPAL capture, host/device discovery and the lock-free sample feed.
//! * [`dsp`] — pitch detection (YIN, McLeod Pitch Method and an FFT fallback).
//! * [`config`] — persisted settings and instrument profiles (TOML).
//! * [`gui`] — the egui/eframe front end.
//!
//! [`music`] contains the small amount of music theory shared by all layers.

pub mod audio;
pub mod config;
pub mod dsp;
pub mod gui;
pub mod music;
