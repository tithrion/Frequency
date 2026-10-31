# Frequency User Guide

Frequency is a chromatic and instrument-specific tuner for acoustic and electric instruments. It listens through your system's audio input, detects the fundamental pitch in real time, and shows how far you are from the selected target in cents.

## 1. Quick Start

### Install a pre-built binary

Download the archive for your platform from the project releases page, unpack it, and run the `frequency` executable. No installer is required.

### Build and run from source

Requirements: Rust 1.88 or newer (stable), a C toolchain, and the audio/GUI development packages for your OS.

```bash
cargo run --release
```

On Linux, install the usual development headers first:

```bash
# Debian / Ubuntu
sudo apt install libasound2-dev libudev-dev libx11-dev libxcursor-dev \
  libxrandr-dev libxi-dev libgl1-mesa-dev libwayland-dev libxkbcommon-dev

# Fedora
sudo dnf install alsa-lib-devel libudev-devel libX11-devel libXcursor-devel \
  libXrandr-devel libXi-devel mesa-libGL-devel wayland-devel libxkbcommon-devel
```

The release profile enables LTO and single codegen unit for maximum DSP speed:

```bash
cargo build --release
./target/release/frequency
```

## 2. Audio Setup

### Linux (ALSA / JACK / PulseAudio)

- Frequency talks to ALSA directly by default; PulseAudio and PipeWire expose ALSA-compatible devices, so they usually work without extra configuration.
- Use `alsamixer` or `pavucontrol` to verify that the intended microphone/interface is not muted and is selected as the default input.
- For JACK, install `libjack-jackd2-dev` (Debian/Ubuntu) or `jack-audio-connection-kit-devel` (Fedora) and build with `cargo run --release --features jack`. With that feature compiled in, the `Automatic` driver (the default) checks at startup whether a JACK server is running and uses JACK when it is; otherwise it falls back to ALSA/PipeWire. An explicit driver choice in the status bar is always honoured.
- If your user cannot access the device, add yourself to the `audio` group and log out/in:

  ```bash
  sudo usermod -aG audio "$USER"
  ```

- Real-time scheduling is optional. For lower latency, configure `rtkit` or add `@audio - rtprio 95` and `@audio - memlock unlimited` to `/etc/security/limits.conf`.

### macOS (CoreAudio)

- macOS asks for microphone permission the first time Frequency opens an input stream. Choose **Allow**.
- If you denied it, open **System Settings → Privacy & Security → Microphone** and enable Frequency (or your terminal, when running from source).
- Select the desired interface in **Audio MIDI Setup** and then in Frequency's device drop-down.

### Windows (WASAPI)

- Windows 10/11 may ask for microphone access on first launch. Choose **Allow**.
- If prompted was missed, open **Settings → Privacy & security → Microphone** and enable microphone access for desktop apps and Frequency.
- Use the Windows sound control panel to select the correct recording device and disable "listen to this device" for the tuner input.

## 3. Using the Tuner

### Reading the display

- **Large note name** — the target note or string pitch. In Generic mode this is the nearest chromatic note.
- **Frequency below it** — the exact target frequency for the current A4 reference.
- **Cent meter** — from −50 cents (quarter tone flat) to +50 cents (quarter tone sharp). The left side is flat, the right side is sharp.
- **Center marker color** — vibrant green when within ±3 cents; amber otherwise.
- **Status text** — "Tune up" when flat, "Tune down" when sharp, "In tune" when within ±3 cents.

The displayed needle is smoothed with a configurable inertia filter. A slow, steady needle makes it easier to see the average pitch; a fast needle reacts quickly to note attacks.

### Generic mode

Generic mode automatically selects the closest chromatic note from A0 to C8. When the detected pitch crosses the midpoint between two semitones (±50 cents), the target changes to the next semitone. The reference pitch for A4 can be set from 415 Hz to 466 Hz in Settings (default 440 Hz), which also updates all target frequencies.

### Instrument profiles

Choose a profile from the **Profile** drop-down in the top bar:

| Profile | Strings |
|---|---|
| Guitar (Standard 6-String) | E2, A2, D3, G3, B3, E4 |
| Bouzouki (Irish/Greek 4-Course) | G2, D3, A3, E4 |
| Mandolin | G3, D4, A4, E5 |
| E-Bass (4-String) | E1, A1, D2, G2 |

In profile mode the target label identifies the string, for example `3 - D3`. The tuner locks onto the nearest string in cents, which lets you tune one string at a time without the target jumping to a neighbouring string.

### Tuning tips

- Pluck the string normally and let the note ring. Avoid touching the string while reading.
- Tune slightly below the target and bring the string up to pitch; this keeps the string seated on the nut and bridge.
- Stretch or press strings gently when settling a new set. Re-check tuning after a few minutes.
- For low bass strings, use a larger detection window (4096 or 8192 samples) in Settings.

## 4. Custom Profiles

Open **Profiles** in the top bar to manage profiles.

- **New profile** — create a tuning with 1 to 99 strings.
- **Edit** — change a custom profile's name, string count, notes, or frequencies.
- **Duplicate** — make a copy of any profile, including built-ins, then edit the copy.
- **Delete** — remove a custom profile. Built-in profiles cannot be deleted.
- Click a profile name to activate it.

In the editor, each string can use either:

- **Scientific pitch notation** — for example `E2`, `C#3`, `Bb4`, or `C-1`. Flats and sharps are accepted.
- **Raw frequency (Hz)** — for microtonal, historical or custom instruments. A raw frequency overrides the note name.

Profiles are saved to the platform configuration directory:

- Linux: `~/.config/frequency/profiles.toml`
- macOS: `~/Library/Application Support/frequency/profiles.toml`
- Windows: `%APPDATA%\frequency\profiles.toml`

The file is standard TOML:

```toml
version = 1
[[profiles]]
name = "Open D"
strings = [
  { note = "D2" },
  { note = "A2" },
  { note = "D3" },
  { note = "F#3" },
  { note = "A3" },
  { note = "D4" },
]
```

## 5. Settings

- **Reference A4** — 415–466 Hz. Changing it recalculates every target frequency.
- **Pitch inertia (α)** — 0.05–0.50. Lower values produce a slower, more stable needle; higher values follow rapid pitch changes. The formula is `new = α · detected + (1 − α) · previous`.
- **Detection window** — 1024, 2048, 4096 or 8192 samples. Larger windows improve low-frequency accuracy at the cost of latency.
- **Preferred sample rate** — request 44.1 kHz or 48 kHz from the audio device when supported.
- **Driver / Device** — switch audio backends and inputs without restarting.

Settings are saved to `settings.toml` in the same configuration directory.

## 6. Troubleshooting

| Symptom | Solution |
|---|---|
| No devices listed | Click the refresh (⟳) button; verify OS microphone permissions and that the device is connected. |
| Tuner shows no pitch | Increase input gain, move closer to the instrument, and check the level in the status line. |
| Reading jumps octaves | Use a profile or the generic mode with a clean single note; reduce room noise and avoid overlapping strings. |
| Low bass not detected | Select a 4096 or 8192 sample window and use a 48 kHz stream. |
| JACK not picked up | Build with `--features jack`, choose the `Automatic` driver, and start the JACK server before launching Frequency. |
| Crackling or dropouts | Increase the audio buffer in the OS/PipeWire settings; close other audio-heavy apps. |
| Settings not saved | Ensure the configuration directory is writable. Frequency logs the error in the terminal. |

## 7. Accuracy and Latency

YIN/MPM estimate the period of the waveform and refine it with parabolic interpolation. On clean tones the frequency error is typically well under one cent. The theoretical minimum latency is set by the detection window: 2048 samples is about 46 ms at 44.1 kHz, and 4096 samples is about 85 ms. The visual needle smooths individual estimates, so tuning decisions remain easy even with a longer window.
