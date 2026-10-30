# PitchPerfect Developer Guide

This document describes the architecture, DSP pipeline, testing strategy and cross-compilation workflow for PitchPerfect.

## 1. Project Layout

```
pitchperfect/
├── Cargo.toml
├── README.md
├── LICENSE
├── docs/
│   ├── USER_GUIDE.md
│   └── DEVELOPER.md
├── examples/
│   └── benchmark.rs
├── src/
│   ├── main.rs            # native entry point
│   ├── lib.rs             # library surface (used by tests)
│   ├── music.rs           # notes, cents, target matching primitives
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── capture.rs     # CPAL stream thread + lock-free ring
│   │   └── host.rs        # host/device enumeration
│   ├── dsp/
│   │   ├── mod.rs
│   │   ├── pitch.rs       # detector selection: YIN, MPM, FFT fallback
│   │   ├── yin.rs         # YIN difference function and lag refinement
│   │   ├── engine.rs      # analyser worker + smoothing
│   │   └── window.rs      # Hann / Blackman-Harris
│   ├── config/
│   │   ├── mod.rs         # platform config paths
│   │   ├── settings.rs    # settings TOML
│   │   └── profiles.rs    # built-in/custom profiles TOML
│   └── gui/
│       ├── mod.rs
│       ├── app.rs         # top-level app and panels
│       ├── meter.rs       # custom cent meter painter
│       ├── editor.rs      # profile editor window
│       └── theme.rs       # dark palette and egui style
└── tests/
    ├── dsp_tests.rs
    └── profile_tests.rs
```

## 2. Concurrency Model

```
[Audio device callback] --push f32--> [rtrb::Producer]
                                            |
                                    lock-free ring
                                            |
                                    [rtrb::Consumer]
                                            |
                              [DSP thread: drain -> analyse]
                                            |
                             Arc<RwLock<Analysis>> snapshot
                                            |
                                  [GUI thread @60 FPS]
```

- **Audio thread** (`pitchperfect-audio`): owns the CPAL `Stream`. The stream callback is allocation-free and lock-free. Multi-channel input is averaged to mono and pushed into an `rtrb` ring buffer. Device/host changes are sent as commands over `std::sync::mpsc`; the stream is dropped and rebuilt without restarting the application. A new ring consumer is handed to the DSP thread for each stream.
- **DSP thread** (`pitchperfect-dsp`): drains the ring into a bounded `VecDeque` (16,384 samples max), analyses the most recent window every 25 ms, applies the inertia filter, and publishes an `Analysis` snapshot under an `RwLock`.
- **GUI thread**: reads the snapshot once per frame, computes the target note and smoothed meter position, and never blocks on audio I/O. `ctx.request_repaint_after(16 ms)` keeps the needle animating at 60 FPS even without input events.

Design rule: no allocation, locking or logging inside the CPAL callback. Ring overruns are harmless because the DSP always consumes the newest window.

## 3. DSP Pipeline

### 3.1 Input and windowing

- Preferred sample rates: 44.1 kHz or 48 kHz, selected from the device's supported ranges.
- Detection window: 1024–8192 samples (default 4096). Larger windows resolve low frequencies such as E1 (~41.2 Hz) but increase latency.
- The FFT fallback applies a symmetric Hann window (`dsp::window::hann`). Blackman-Harris is available for low-sidelobe spectral analysis.

### 3.2 YIN (primary)

The YIN implementation lives in `dsp/yin.rs`; `dsp/pitch.rs` re-exports it and
combines it with the fallback detectors. For each lag τ up to
`min(N/2, sample_rate / 25 Hz)`:

1. Difference function `d(τ) = Σ (x[i] − x[i+τ])²`.
2. Cumulative mean normalized difference `d'(τ) = d(τ) · τ / Σ_{j=1..τ} d(j)`.
3. Choose the first local minimum below the threshold (0.15), or the global minimum.
4. Refine τ with parabolic interpolation and compute `f = sample_rate / τ`.
5. Confidence = `1 − d'(τ)`.

YIN is robust against octave errors on harmonic-rich plucked strings and is the fastest path for clean signals.

### 3.3 McLeod Pitch Method (fallback)

MPM computes the normalized square difference function (NSDF):

```
n(τ) = 2 · Σ x[i]·x[i+τ] / Σ (x[i]² + x[i+τ]²)
```

Positive lobes after the first zero crossing are inspected. The first peak above 90% of the key maximum is selected and refined by parabolic interpolation. MPM is used when YIN confidence is below 0.70.

### 3.4 FFT fallback

If both time-domain methods are weak, a Hann-windowed radix-2 FFT is computed (`rustfft`), the strongest bin in 25 Hz–5 kHz is found, and the peak is refined using log-magnitude parabolic interpolation. This path is intentionally a fallback: its absolute frequency resolution is limited by the window length.

The final estimate is the highest-confidence candidate above a 0.20 threshold.

### 3.5 Smoothing and cents

The inertia filter is:

```
f_rendered = α · f_detected + (1 − α) · f_previous
```

`α` is exposed in Settings as **Pitch inertia** in the range 0.05–0.50. A jump greater than 100 cents resets the filter so that switching strings is immediate.

Target matching:

- **Generic**: nearest MIDI note in A0–C8 using `midi = round(69 + 12·log2(f / A4))`.
- **Profile**: the string whose resolved frequency minimizes `|cents|`.
- **Deviation**: `cent = 1200 · log2(f_detected / f_target)`.

The GUI additionally smooths the meter position (`28%` of the remaining distance per frame) and switches the color to green at `|cents| ≤ 3`.

## 4. Configuration and Profiles

All paths are resolved through `directories::BaseDirs` (`config::config_dir`):

| Platform | Directory |
|---|---|
| Linux | `~/.config/pitchperfect` |
| macOS | `~/Library/Application Support/pitchperfect` |
| Windows | `%APPDATA%\pitchperfect` |

- `settings.toml` is written by `Settings::save`; every value is sanitized to a safe range on load.
- `profiles.toml` contains only custom profiles. Built-ins are generated at runtime and can never be overwritten by the loader. A custom profile whose name collides with a built-in is skipped with a warning.
- Profiles are validated again at save time: 1–99 strings, unique non-reserved name, valid notes or positive Hz frequencies.

## 5. Building and Testing

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The test suite covers:

- YIN/MPM/FFT detection of sine and harmonic tones at 44.1 and 48 kHz.
- Low E1 detection with a 4096-sample window.
- All standard guitar strings.
- Silence/noise rejection.
- Note parsing (sharps, flats, negative octaves).
- Profile targeting, validation, TOML round-trips and duplication.
- Cent/frequency inverse conversions and settings clamping.

### Benchmarking

A dependency-free micro-benchmark is included:

```bash
cargo run --release --example benchmark
```

It reports milliseconds per detection for the full `detect` pipeline, YIN, MPM and FFT on a 4096-sample 440 Hz sine. For deeper profiles use `perf` on Linux:

```bash
perf record --call-graph dwarf ./target/release/pitchperfect
perf report
```

or `cargo flamegraph` (after installing `flamegraph`). Audio callback performance is best measured with `perf` while the tuner is running; the callback only converts samples and pushes into the ring.

### Debugging audio

```bash
RUST_LOG=debug cargo run
```

`cpal` and `alsa` diagnostics are emitted by the audio thread. The application logs stream configuration, selected host/device and DSP attach events.

## 6. Cross-Compilation

The code targets `x86_64` and `aarch64`/`arm64` on Linux, macOS and Windows. CPAL and eframe depend on platform system libraries, so cross-compilation requires the corresponding SDK/sysroot.

### Linux x86_64 → Linux aarch64

Install the cross toolchain and target:

```bash
rustup target add aarch64-unknown-linux-gnu
sudo apt install gcc-aarch64-linux-gnu g++-aarch64-linux-gnu \
  libasound2-dev:arm64
```

Configure Cargo in `.cargo/config.toml`:

```toml
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
```

Then:

```bash
cargo build --release --target aarch64-unknown-linux-gnu
```

`cross` is often easier because it provides a container with the required libraries:

```bash
cargo install cross
cross build --release --target aarch64-unknown-linux-gnu
```

### macOS (Intel and Apple Silicon)

Build natively on macOS. For a universal binary:

```bash
rustup target add x86_64-apple-darwin aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
lipo -create \
  target/x86_64-apple-darwin/release/pitchperfect \
  target/aarch64-apple-darwin/release/pitchperfect \
  -output pitchperfect-universal
```

### Windows (MSVC)

Build natively with the MSVC toolchain:

```powershell
rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
cargo build --release --target aarch64-pc-windows-msvc
```

For cross-building from Linux, `cross build --release --target x86_64-pc-windows-gnu` works if the MinGW dependencies are available. WASAPI itself must be tested on Windows.

### Release packaging

The release profile uses `opt-level = 3`, full LTO, one codegen unit and `panic = "abort"`. Archive the executable together with `README.md`, the `LICENSE` file and `docs/` for distribution.

## 7. Extending

- **New detector**: add a function returning `PitchEstimate` in `dsp/pitch.rs` and include it in the candidate selection in `detect`.
- **New built-in profile**: add an entry in `config::profiles::builtin_profiles` and a test in `tests/profile_tests.rs`.
- **UI changes**: keep painting in `gui/meter.rs`, editor widgets in `gui/editor.rs`, and application state in `gui/app.rs`.
- **Audio backends**: CPAL hosts are compiled in by platform; `audio/host.rs` enumerates whatever is available. The Linux JACK host is behind the `jack` Cargo feature (enable with `--features jack`) so the default build only needs ALSA/`libasound`. Avoid holding CPAL objects on the GUI thread.

## 8. License

PitchPerfect is licensed under the GNU General Public License v3.0 (GPL-3.0). Contributions are accepted under the same terms.
