# Frequency Tuner (`frequency`)

## 1. Executive Summary & Goals

You are tasked with building **`frequency`**, a high-performance, cross-platform, visually appealing musical tuner written in Rust. The application must adhere strictly to the Unix philosophy: do one thing (tuning audio instruments accurately and responsively) and do it exceptionally well.

### Key Targets
- **Languages/Tech Stack**: Rust (edition 2021+), `cpal` / `rodio` (audio host context), `egui` / `eframe` (GPU-accelerated native GUI), `rustfft` / `pitch-detection` (YIN or McLeod Pitch Method), `serde` + `toml` (configuration & profiles).
- **Cross-Platform Support**: Linux (`ALSA`, `JACK`, `PulseAudio`), macOS (`CoreAudio`), Windows (`WASAPI`). Architectures: `x86_64` and `aarch64` / `arm64`.
- **Licensing & Open Source**: Licensed under GPL-3.0 (GNU General Public License v3.0).

---

## 2. Architecture & Technical Design

### 2.1 Concurrency Model & Audio Pipeline
To guarantee low latency (< 20ms pitch detection lag) and 60 FPS fluid rendering without UI frame drops, decouple audio DSP from rendering using a dedicated lock-free ring buffer (e.g., `rtrb`).

```
[ Audio Hardware / Host (CPAL) ]
              │ (Audio Callbacks - Lock-free)
              ▼
    [ Lock-Free RingBuffer ]
              │
              ▼
  [ Worker Thread: DSP Engine ]
   ├── Windowing (Hann / Blackman-Harris)
   ├── Pitch Detection (YIN / McLeod Pitch Method)
   ├── Smoothing / Inertia Filter (Exponential Moving Average)
   └── Cent Deviation Calculation
              │
              ▼ (Atomic/Arc Swap or Channel)
  [ GUI Thread: egui / eframe Engine (60 FPS) ]
```

### 2.2 DSP & Pitch Detection Algorithm
1. **Sample Rate**: Prefer 44.1 kHz or 48 kHz standard input.
2. **Buffer Size**: 2048 to 4096 samples (tradeoff between low-frequency precision, e.g., Low E-Bass E1 ~41.2 Hz, and processing delay).
3. **Algorithm**: Implement **YIN** or **McLeod Pitch Method (MPM)** with auto-correlation fallback.
4. **Frequency Calculation**:
   $$\text{cent} = 1200 \cdot \log_2\left(\frac{f_{\text{detected}}}{f_{\text{target}}}\right)$$
5. **Pitch Inertia Filter**:
   $$f_{\text{rendered}} = \alpha \cdot f_{\text{detected}} + (1 - \alpha) \cdot f_{\text{previous}}$$
   Where $\alpha \in [0.05, 0.5]$ configured via UI "Inertia/Damping" settings.

---

## 3. Core Requirements & Features

### 3.1 Audio Input Management
- **Default Input**: Automatically connect to the system default audio input device upon startup.
- **Device & Host Selection**: UI drop-down menu allowing dynamic switching between available audio drivers/hosts (e.g., Linux: ALSA, JACK; macOS: CoreAudio; Windows: WASAPI) and specific audio input devices without crashing or requiring application restart.

### 3.2 Pitch Display & Target Matching
- **Target Selection**:
  - **Generic Mode**: Dynamically locks to the closest chromatic pitch ($A_0$ to $C_8$) based on equal temperament centered around reference $A_4$ (default $440\text{ Hz}$, adjustable $415\text{ Hz} - 466\text{ Hz}$). Automatically transitions to the next semitone once the signal crosses a $\pm 50\text{ cent}$ threshold ($\pm \text{quarter tone}$).
  - **Instrument Profile Mode**: Restricts target matching to specified string pitch targets, displaying string index/labels.
- **Visual Metering**:
  - Range: $-50\text{ cents}$ (quarter tone flat) to $+50\text{ cents}$ (quarter tone sharp).
  - Target note name displayed centrally in bold typography.
  - Marker dynamics: Smoothly shifts across the cent scale.
  - **Color Indicator**: Amber/Orange when $|\text{cent}| > 3$, transitioning sharply/smoothly to **Vibrant Green** when $|\text{cent}| \le 3$.

### 3.3 Profiles & Profile Editor
- **Built-In Profiles**:
  - **Generic**: Auto-detects all chromatic notes.
  - **Guitar (Standard 6-String)**: $E_2, A_2, D_3, G_3, B_3, E_4$
  - **Bouzouki (Irish/Greek 4-Course)**: $G_2, D_3, A_3, E_4$
  - **Mandolin**: $G_3, D_4, A_4, E_5$
  - **E-Bass (4-String)**: $E_1, A_1, D_2, G_2$
- **Custom Profile Editor**:
  - User can create, edit, duplicate, and delete custom profiles.
  - String count support: 1 to 99 strings/notes.
  - Per-string note definition (Scientific pitch notation, e.g., `C#3`, or raw Hz frequency override).
  - Save custom profiles into standard TOML format (`~/.config/frequency/profiles.toml` or OS equivalent).

---

## 4. UI/UX Specification

The interface should feature a sleek, modern dark mode aesthetic built with `egui`.

```
+-----------------------------------------------------------------------+
|  Frequency Tuner                           [Settings ⚙] [Profiles ▾]|
+-----------------------------------------------------------------------+
|                                                                       |
|                           Target String: [ 3 - D3 ]                   |
|                                                                       |
|                                  D3                                   |
|                              146.83 Hz                                |
|                                                                       |
|   -50c            -25c           0c           +25c           +50c     |
|    |---------------+--------------|--------------+---------------|    |
|                                   ▲                                   |
|                             [   GREEN   ]                             |
|                           (In Tune: +1.2c)                            |
|                                                                       |
+-----------------------------------------------------------------------+
| Driver: [ ALSA ▾ ]  Device: [ Built-in Microphone ▾ ]  A4 = [ 440 Hz ]|
+-----------------------------------------------------------------------+
```

---

## 5. File & Directory Layout

Structure the Rust project as follows:

```
frequency/
├── Cargo.toml
├── README.md
├── LICENSE
├── docs/
│   ├── USER_GUIDE.md
│   └── DEVELOPER.md
├── src/
│   ├── main.rs
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── capture.rs
│   │   └── host.rs
│   ├── dsp/
│   │   ├── mod.rs
│   │   ├── pitch.rs
│   │   ├── yin.rs
│   │   └── window.rs
│   ├── config/
│   │   ├── mod.rs
│   │   ├── settings.rs
│   │   └── profiles.rs
│   └── gui/
│       ├── mod.rs
│       ├── app.rs
│       ├── meter.rs
│       ├── editor.rs
│       └── theme.rs
└── tests/
    ├── dsp_tests.rs
    └── profile_tests.rs
```

---

## 6. Project Manifest (`Cargo.toml`)

Ensure the dependencies use stable, fast libraries:

```toml
[package]
name = "frequency"
version = "0.1.0"
edition = "2021"
authors = ["Frequency Contributors"]
description = "A fast, accurate, cross-platform musical instrument tuner."
license = "GPL-3.0-only"

[dependencies]
cpal = "0.15"
eframe = "0.27"
egui = "0.27"
rtrb = "0.3" # Lock-free ringbuffer
rustfft = "6.2"
pitch-detection = "0.3" # YIN / MPM implementations
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
directories = "5.0"
log = "0.4"
env_logger = "0.11"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
```

---

## 7. Complete Documentation Requirements

Create comprehensive documentation in `docs/` as part of the deliverable:

1. **`docs/USER_GUIDE.md`**:
   - Quick Start & Installation (Binary installation & building from source).
   - Audio Setup Guide (Linux ALSA/JACK permissions, macOS Microphone Privacy Settings, Windows Privacy Settings).
   - Tuner Usage (Understanding cent readings, tuning accuracy, custom profiles editor).

2. **`docs/DEVELOPER.md`**:
   - Architecture & DSP Pipeline deep dive.
   - Cross-compilation guide for `x86_64` and `aarch64` across Linux, macOS, and Windows.
   - Test suite execution (`cargo test`) and benchmarking DSP routines.

---

## 8. Implementation Steps for the AI Agent

1. **Phase 1: Audio Core**: Set up `cpal` to capture audio input asynchronously into an `rtrb` lock-free ring buffer.
2. **Phase 2: DSP Module**: Implement pitch detection (YIN/MPM) with sliding windowing. Write tests verifying accurate frequency calculation for standard pitch sine waves (e.g., $440\text{ Hz} \rightarrow A_4$).
3. **Phase 3: GUI Development**: Build the `egui` frontend with interactive cent-meter canvas rendering, responsive color updates ($\le 3\text{ cent}$ green highlight), and configurable dynamic display smoothing.
4. **Phase 4: Profiles & Settings**: Implement serialization/deserialization for custom instrument profiles and configuration persistence.
5. **Phase 5: Docs & Packaging**: Create build scripts/workflows and user/developer documentation files.
