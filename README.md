# BeAnal (Rust Edition)

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Release Build](https://github.com/BeSpec-Dev/beanal/actions/workflows/release.yml/badge.svg)](https://github.com/BeSpec-Dev/beanal/actions/workflows/release.yml)
[![CI](https://github.com/BeSpec-Dev/beanal/actions/workflows/ci.yaml/badge.svg)](https://github.com/BeSpec-Dev/beanal/actions/workflows/ci.yaml)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-blue)]()

A high-performance, cross-platform, real-time audio spectrum visualizer written in **Rust**.

**BeAnal** listens to your system audio loopback (what you hear from your speakers) and renders a customizable frequency spectrum overlay on your desktop. It is designed to be minimal, beautiful, and efficient.


![BeAnal Demo](docs/images/main_window_animation02.gif)
---

## ✨ Features

* **⚡ High Performance:** Built with `egui` (immediate mode GUI) and `realfft` for low-latency rendering and audio processing.
* **🎧 Cross-Platform Audio:**
    * Uses `cpal` to capture system audio on Windows (WASAPI), Linux (ALSA/Pulse/Jack), and macOS (CoreAudio).
    * **Hot-Swappable Devices:** Select specific input devices and refresh hardware lists on the fly without restarting.
* **🎛️ Deep Customization:**
    * **4 Visualization Modes:**
        * **📊 Solid Bars:** Classic smooth gradients with adjustable opacity.
        * **📟 Retro LED (Segmented):** Old-school segmented bars with customizable segment height and gap. Includes a "Fill to Peak" warning mode.
        * **📈 Line Spectrum:** A continuous, glowing frequency contour.
        * **〰️ Oscilloscope:** Real-time raw waveform monitoring (Time Domain).
    * **Optimized FFT Engine:** Uses a fixed 2048-point FFT for excellent frequency resolution across all sample rates (e.g., 23.4 Hz/bin @ 48kHz).
    * **Responsiveness:** Configure Attack/Release times for bars and Peak Hold/Decay mechanics.
* **🎨 Theming:**
    * **Presets:** Select from 25+ hand-crafted color schemes (Neon Tokyo, Cyberpunk, Winamp Classic, Molten Core).
    * **Custom:** Define your own Low/High/Peak colors to match your setup.
* **🎵 Media Integration:**
    * Now Playing Overlay: Elegantly displays current track details (Title, Artist, Album) in the corner of the visualizer.
    * Smart Behavior: Configurable to fade out after updates, remain persistent, or hide completely.
* 🔍 **Interactive Inspector:** Turn the visualizer into a precision analysis tool. Hover over the spectrum to activate a vertical crosshair that highlights specific frequency bins and displays exact Frequency (Hz) and Amplitude (dB) metrics.
* **🖥️ Modern UI:**
    * **Borderless Window:** A clean, chrome-less window that floats on your desktop with "Always on Top" and "Click-through" support.
    * **Persistent Settings:** Configuration is automatically saved to your OS's standard application data folder.
    * **Performance HUD:** Real-time overlay displaying FPS, FFT latency, and frequency resolution.

| **Solid Bars** | **Retro LED** |
| :---: | :---: |
| ![Solid Mode](docs/images/mode_solid.gif) | ![LED Mode](docs/images/mode_led.gif) |
| *Classic smooth gradients* | *Segmented bars with peak filling* |

| **Line Spectrum** | **Oscilloscope** |
| :---: | :---: |
| ![Line Mode](docs/images/mode_line.gif) | ![Scope Mode](docs/images/mode_scope.gif) |
| *Glowing frequency contour* | *Raw waveform monitoring* |

## 📚 Case Studies

BeAnal is designed for precision. See how it uncovers hidden artifacts in professional audio production:

* **[Queens of the Stone Age Analysis](./docs/case_study.md):** Detecting a 15.75 kHz CRT whine hidden in the outro of *I Was a Teenage Hand Model* using the 512-bar high-resolution mode.

## 🚀 Installation & Usage

### Option A: Pre-built Binaries
1.  Go to the [Releases Page](../../releases/latest).
2.  Download the executable for your OS:
    * **Windows:** `beanal-windows.exe`
    * **macOS:** `beanal-macos-silicon` (M1/M2) or `beanal-macos-intel`
    * **Linux:** `beanal-linux`
3.  Run the application!
    * *(Linux/macOS users may need to allow execution: `chmod +x beanal`)*

macOS Users: To visualize system audio, you must set up a loopback driver. See the [macOS Setup Guide](docs/macos_setup.md).

Linux Users: To visualize system audio, you must route audio into BeAnal via `pavucontro` or equivlant. See the [Linux Setup Guide](docs/linux_setup.md).

### Option B: Build from Source
If you prefer to build it yourself, you will need the [Rust toolchain](https://www.rust-lang.org/tools/install) installed.

1.  Clone the repository:
    ```bash
    git clone [https://github.com/BeSpec-Dev/beanal.git](https://github.com/BeSpec-Dev/beanal.git)
    cd beanal
    ```

    **Linux Dependencies:**
    If building from source, ensure you have the development headers installed:
    ```bash
    # Ubuntu/Debian
    sudo apt-get install libasound2-dev libudev-dev pkg-config
    ```
2.  Run in release mode:
    ```bash
    cargo run --release
    ```

## 🎮 Controls & Usage

* **Move:** Click and drag anywhere on the visualizer background to move the window.
* **Resize:** Click and drag the **bottom-right corner** (indicated by subtle grip lines).
* **Maximize:** Double-click the window background to toggle fullscreen.
* **Context Menu:** **Right-click** anywhere on the window to open the main menu.
    * **⚙ Settings:** Opens the advanced configuration window.
    * **❌ Exit:** Closes the application.

## ⚙️ Configuration

The settings window is organized into tabs for easy navigation:

| Tab | Description |
| :--- | :--- |
| **🎨 Visual** | Adjust bar count (16-512), spacing, and transparency. Toggle Inverted Mode (Top-Down) and Aggregation (Peak vs Average). |
| **🔊 Audio** | Hot-swappable input device selection, and fine-tune the FFT engine (Sensitivity, Noise Floor, Attack/Release). |
| **🌈 Colors** | Choose from 25+ retro and modern color presets. The UI themes itself to match your selection! |
| **🪟 Window** | Toggle "Always on Top", window decorations (Title Bar), and the performance stats overlay. |
| **📊 Stats** | View diagnostics like Sample Rate, specific Latency (ms), and connection health indicators. |

## 🛠️ Architecture

BeAnal uses a concurrent architecture to ensure the UI never stutters, even under heavy audio load:

* **Audio Thread:** Captures raw samples via `cpal` and normalizes formats (I16/U16/F32).
* **FFT Thread:** Processes signals using `realfft`, applying Hann windowing and smoothing logic.
* **GUI Thread:** Renders the visualization at 60+ FPS using `egui` + `wgpu`.
* **State Management:** Threads communicate via `crossbeam_channel` for high-speed audio data and `Arc<Mutex<SharedState>>` for configuration synchronization.

## 🔧 Troubleshooting & Logging

BeAnal runs silently by default. If you encounter issues, logs are automatically rotated daily and stored in your OS standard data directory:

* **Windows:** `%APPDATA%\BeAnal\logs\`
* **macOS:** `~/Library/Application Support/BeAnal/logs/`
* **Linux:** `~/.local/share/beanal/logs/`

### Debug Mode
To view granular details (like window resize events or specific FFT rebuild triggers), you can enable verbose logging via environment variables without recompiling:

**Windows (PowerShell):**
```powershell
$env:RUST_LOG="debug"; .\beanal.exe
```
**macOS / Linux:**
``` bash
RUST_LOG=debug ./beanal
```

## 🤝 Contributing

Contributions are welcome! This project is a learning journey into Rust audio programming.

1.  Fork the project.
2.  Create your feature branch (`git checkout -b feature/AmazingFeature`).
3.  Commit your changes (`git commit -m 'Add some AmazingFeature'`).
4.  Push to the branch (`git push origin feature/AmazingFeature`).
5.  Open a Pull Request.

## 📄 License

Distributed under the MIT License. See `LICENSE` for more information.