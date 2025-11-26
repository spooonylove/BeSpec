# BeAnal (Rust Edition)

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)]()
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-blue)]()

A high-performance, (soon to be) cross-platform, real-time audio spectrum visualizer written in **Rust**.

**BeAnal** listens to your system audio loopback (what you hear from your speakers) and renders a customizable frequency spectrum overlay on your desktop. It is designed to be minimal, beautiful, and efficient.

> **Note:** This is a modern rewrite of the original [C#/WPF implementation](https://github.com/BeSpec-Dev/beanal-legacy), leveraging Rust and `egui` for lower latency and cross-platform compatibility.

![BeAnal Demo](docs/images/main_window_animation01.gif)
---

## ✨ Features

* **⚡ High Performance:** Built with `egui` (immediate mode GUI) and `realfft` for blazing fast rendering and audio processing.
* **🎧 Cross-Platform Audio:** Uses `cpal` to capture system audio on Windows (WASAPI), Linux (ALSA/Pulse/Jack), and macOS (CoreAudio).
* **🎛️ Deep Customization:**
    * **Dynamic FFT:** Adjust window size (512 to 4096 samples) to balance latency vs. frequency resolution.
    * **Visuals:** Tune bar counts (16-512), sensitivity, noise floor, and opacity.
    * **Responsiveness:** Configure Attack/Release times for bars and Peak Hold/Decay times for indicators.
    * **Theming:** Select from 25+ preset color schemes (Winamp, Synthwave, Cyberpunk) or use dynamic "Rainbow" mode.
* **🖥️ Modern UI:**
    * **Borderless Window:** A clean, chrome-less window that floats on your desktop.
    * **Multi-Window Settings:** A dedicated, non-blocking settings window with modern "Pill" style navigation tabs.
    * **Performance HUD:** Optional overlay displaying real-time FPS, FFT processing time, and latency metrics.

## 🚀 Getting Started

### Prerequisites
You will need the [Rust toolchain](https://www.rust-lang.org/tools/install) installed.

### Build and Run
1.  Clone the repository:
    ```bash
    git clone [https://github.com/BeSpec-Dev/beanal.git](https://github.com/BeSpec-Dev/beanal.git)
    cd beanal
    ```
2.  Run in release mode (recommended for smooth 60FPS audio visualization):
    ```bash
    cargo run --release
    ```

## 🎮 Controls & Usage

* **Move:** Click and drag anywhere on the visualizer background to move the window.
* **Resize:** Click and drag the **bottom-right corner** (indicated by subtle grip lines).
* **Context Menu:** **Right-click** anywhere on the window to open the main menu.
    * **⚙ Settings:** Opens the advanced configuration window.
    * **❌ Exit:** Closes the application.

## ⚙️ Configuration

The settings window is organized into tabs for easy navigation:

| Tab | Description |
| :--- | :--- |
| **🎨 Visual** | Adjust bar count, spacing, and transparency. Toggle Peak indicators and aggregation modes (Peak vs. Average). |
| **🔊 Audio** | Fine-tune the FFT engine. Adjust sensitivity, noise floor (-dB), and smoothing (Attack/Release). |
| **🌈 Colors** | Choose from 25+ retro and modern color presets. The UI themes itself to match your selection! |
| **🪟 Window** | Toggle "Always on Top", window decorations (Title Bar), and the performance stats overlay. |
| **📊 Stats** | View nerdy details like Sample Rate, Frequency Resolution (Hz/Bin), and internal Latency. |

## 🛠️ Architecture

BeAnal uses a concurrent architecture to ensure the UI never stutters, even under heavy audio load:

* **Audio Thread:** Captures raw samples via `cpal` and normalizes formats (I16/U16/F32).
* **FFT Thread:** Processes signals using `realfft`, applying Hann windowing and smoothing logic.
* **GUI Thread:** Renders the visualization at 60+ FPS using `egui` + `wgpu`.
* **State Management:** Threads communicate via `crossbeam_channel` for high-speed audio data and `Arc<Mutex<SharedState>>` for configuration synchronization.

## 🤝 Contributing

Contributions are welcome! This project is a learning journey into Rust audio programming.

1.  Fork the project.
2.  Create your feature branch (`git checkout -b feature/AmazingFeature`).
3.  Commit your changes (`git commit -m 'Add some AmazingFeature'`).
4.  Push to the branch (`git push origin feature/AmazingFeature`).
5.  Open a Pull Request.

## 📄 License

Distributed under the MIT License. See `LICENSE` for more information.