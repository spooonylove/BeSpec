# 🐧 Linux Setup Guide

Running a system audio visualizer on Linux requires a bit of configuration to ensure the application has the correct permissions to "hear" your desktop audio.

As of v1.8.0, BeSpec uses native **PipeWire** to automatically route and capture your active audio output.

---

## 1. Install Dependencies

Before running or building BeSpec, you need to ensure your system has the necessary audio and GUI libraries installed. Even though the app is a standalone binary, it relies on these system libraries.

### Ubuntu / Debian / Pop!_OS / Mint
```bash
sudo apt update
sudo apt install pkg-config libasound2-dev libdbus-1-dev libudev-dev libpipewire-0.3-dev libclang-dev
```

### Fedora
```bash
sudo dnf install pkgconf-pkg-config alsa-lib-devel dbus-devel systemd-devel pipewire-devel clang
```

### Arch Linux / Manjaro
```bash
sudo pacman -S pkgconf alsa-lib dbus systemd pipewire clang
```

---

## 2. Installation

We provide a pre-packaged installer that sets up the binary, icon, and application menu shortcut for you.

1.  **Download the Release:**
    Go to the [Releases Page](https://github.com/bespec-dev/bespec/releases) and download the file ending in `linux.tar.gz`.

2.  **Extract the Archive:**
    Right-click the file and select "Extract Here", or run:
    ```bash
    tar -xzvf bespec-*.linux.tar.gz
    ```

3.  **Run the Installer:**
    Open your terminal inside the extracted `bespec-dist` folder and run:
    ```bash
    ./install.sh
    ```
    * This installs the app to `~/.local/bin/bespec`.
    * This installs the icon to `~/.local/share/icons/bespec.png`.
    * This creates a shortcut in your Application Menu.

4.  **Launch:**
    You can now find **BeSpec** in your system's application launcher (Super/Windows Key) just like any other app.

---

## 3. Audio Capture & Privacy Indicators

BeSpec intelligently scans your PipeWire registry and automatically connects to the "Monitor" of your current default speakers or headphones. If you switch outputs (e.g., plugging in a USB headset), BeSpec will seamlessly follow the audio. 

**🎙️ Note on the "Microphone Active" OS Warning:**
When BeSpec is running, your desktop environment (GNOME, KDE, etc.) may show an orange "Microphone Active" or "Recording" privacy indicator in your system tray. 

* **Why this happens:** Linux treats capturing your speaker output (to draw the visualizer) with the same strict security level as capturing a physical microphone. 
* **Your privacy is safe:** BeSpec is explicitly reading the audio going *to your speakers*, not the audio from your room. All audio data is processed locally in real-time to draw the visualization and is immediately discarded.

*(If you ever want to verify exactly what BeSpec is listening to, you can install `pavucontrol`, go to the "Recording" tab, and see that it is bound to your Output Monitor rather than your physical mic).*

---

## 4. Wayland vs. X11

BeSpec runs natively on Wayland, but because Wayland compositors (like GNOME or KDE) handle windows differently than X11, you may notice the following:

* **Window Positioning:** On Wayland, the OS compositor decides where the window appears. BeSpec will not "force" itself to a saved X/Y coordinate on startup to avoid conflicts with your desktop's window management.
* **Position Saving:** Position saving is automatically disabled when a Wayland session is detected to prevent the configuration file from being overwritten with inaccurate coordinates.
* **Forcing XWayland:** If you require strict coordinate-based positioning or experience issues with window borders, you can force XWayland mode by setting this environment variable before launching:
    ```bash
    WINIT_UNIX_BACKEND=x11 bespec
    ```

---

## 5. KDE Plasma: Window Rules (Recommended)

If you are using the KDE Plasma desktop, it is highly recommended to use **Window Rules** to manage BeSpec’s behavior, especially on Wayland:

* **Bypass Compositor:** You can create a rule for BeSpec to "Ignore requested geometry" or "Force position/size" if the compositor is moving the window unexpectedly.
* **Keep Above:** Since BeSpec is a visualizer, setting a rule for "Keep Above" ensures it stays visible over your other windows.
* **Appearance:** You can use rules to force the window to be "No titlebar and frame" if you want a cleaner look regardless of the internal app settings.

---

## 6. Troubleshooting

### Permissions Errors
If you cannot capture audio at all, ensure your user is part of the audio group:
```bash
sudo usermod -a -G audio $USER
```
*(You will need to log out and log back in for this to take effect.)*