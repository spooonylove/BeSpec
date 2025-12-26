# **🐧 Linux Setup Guide for BeAnal**

Running a system audio visualizer on Linux requires a bit more configuration than Windows or macOS due to the way Linux handles audio security and routing.

This guide covers installation, dependencies, and how to route your desktop audio into BeAnal.

## **1\. Install Dependencies**

Before running BeAnal, you need to ensure your system has the necessary audio and GUI libraries installed.

### **Ubuntu / Debian / Pop\!\_OS**

Open a terminal and run:

sudo apt update  
sudo apt install pkg-config libasound2-dev libdbus-1-dev libudev-dev pavucontrol

* libasound2-dev: ALSA Audio backend.  
* libdbus-1-dev: Required for media player controls (MPRIS).  
* pavucontrol: The standard GUI for routing audio streams (critical for Step 3).

### **Fedora**

sudo dnf install pkgconf-pkg-config alsa-lib-devel dbus-devel systemd-devel pavucontrol

### **Arch Linux**

sudo pacman \-S pkgconf alsa-lib dbus systemd pavucontrol

## **2\. Installation**

### **Option A: Using the Release Binary**

1. Download beanal-linux from the \[suspicious link removed\].  
2. Open your terminal in the downloads folder.  
3. **Make it executable:** Linux blocks downloaded binaries by default.  
   chmod \+x beanal-linux

4. Run it:  
   ./beanal-linux

### **Option B: Building from Source**

If you cloned the repository:

cargo run \--release

## **3\. 🔊 Audio Routing (Crucial Step)**

By default, Linux applications "Capture" audio from your **Microphone**. To visualize music from Spotify, YouTube, or your browser, you must tell the system to capture the **Monitor** of your speakers instead.

**If you see a flat line or only noise when playing music, follow these steps:**

1. **Start BeAnal** and play some music (Spotify, YouTube, etc.).  
2. Open **PulseAudio Volume Control** (Run pavucontrol in your terminal or app menu).  
3. Navigate to the **Recording** tab.  
4. Find **BeAnal** in the list of recording apps.  
5. Click the drop-down menu next to it (it usually says "Built-in Audio Analog Stereo").  
6. Select **"Monitor of Built-in Audio Analog Stereo"** (or "Monitor of \[Your Headphones\]").

**Note:** If you are using **PipeWire**, this setting is usually remembered for next time.

## **4\. Troubleshooting**

### **The App Freezes on Startup**

* **Cause:** BeAnal might be trying to probe a "sleeping" HDMI port or a raw hardware device.  
* **Fix:** We have patched this in recent versions, but if it persists, try disabling unused HDMI audio profiles in your system settings.

### **"ALSA lib pcm\_dmix.c... unable to open slave"**

* **Cause:** These are harmless warnings from the underlying audio driver when it scans virtual devices.  
* **Fix:** You can safely ignore them. If the app runs, it works.

### **Permissions Errors**

* If you cannot capture audio, ensure your user is part of the audio group:  
  sudo usermod \-a \-G audio $USER

  *(You will need to log out and log back in for this to take effect).*