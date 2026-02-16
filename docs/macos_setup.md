# macOS Setup Guide

Unlike Windows or Linux, macOS requires two distinct steps to get a spectrum analyzer working:
1. **Installation:** Installing the unsigned application.
2. **Audio Routing:** Setting up a "loopback" so the app can hear your system audio.

---

## Phase 1: Installation

### Option A: Homebrew (Recommended)
If you already use Homebrew, this is the easiest way. It handles the download and can often bypass the security warnings automatically.

```bash
# 1. Add the BeSpec tap
brew tap bespec-dev/tap

# 2. Install the app (removes quarantine automatically)
brew install --cask bespec --no-quarantine
```

### Option B: Manual Download (.dmg)
1.  Go to the [Releases Page](https://github.com/bespec-dev/bespec/releases).
2.  Download the correct file for your Mac:
    * **Apple Silicon (M1/M2/M3/M4):** Download `macos-silicon.dmg`
    * **Intel Macs:** Download `macos-intel.dmg`
3.  Open the `.dmg` file and drag **BeSpec.app** into your **Applications** folder.

### ⚠️ Handling Security Permissions
Because BeSpec is free and open-source, it is not signed with a paid Apple Developer certificate. macOS may try to block it.

* **The "Right-Click" Trick:** On the very first launch, **Right-Click** (or Control+Click) the app icon and select **Open**. This gives you an "Open" button in the warning dialog.
* **Microphone Access:** When prompted, click **Allow** for Microphone access. (macOS considers all audio input, even internal loopbacks, to be a "Microphone").

---

## Phase 2: Audio Routing Setup

To visualize music from audio sources on your MacOS device, you need to create a virtual link between your speakers and BeSpec. We recommend using **BlackHole**.

### Step 1: Install BlackHole
You need the **2-channel** version.

* **Via Homebrew:** `brew install blackhole-2ch`
* **Manual:** Download from [existential.audio](https://existential.audio/blackhole/)

### Step 2: Create a Multi-Output Device
This step creates a virtual "splitter" that sends audio to your speakers AND to BeSpec simultaneously.

1.  Open **Audio MIDI Setup** (Cmd+Space, type "Audio MIDI Setup").
2.  Click the **+** (plus) icon in the bottom-left corner and select **Create Multi-Output Device**.
3.  In the list on the right, check the boxes for:
    * **Built-in Output** (or your Headphones/External DAC).
    * **BlackHole 2ch**.
4.  **Drift Correction:** Check the box next to **BlackHole 2ch**. This ensures your audio stays in sync with your speakers.

![Screenshot: Audio MIDI Setup showing Multi-Output Device configuration]

### Step 3: Route System Audio
1.  Open **System Settings** > **Sound**.
2.  Under the **Output** tab, select the **Multi-Output Device** you just created.

> **Note:** When using a Multi-Output device, macOS disables the volume keys on your keyboard. You must control volume via your physical speaker buttons or inside the specific music app.

![Screenshot: macOS System Settings Sound Output tab]

---

## Phase 3: App Configuration

This is the most common mistake! You must send audio to one place, but listen from another.

1.  Open **BeSpec**.
2.  In the audio device dropdown, select: **BlackHole 2ch**.

**⚠️ DO NOT select "Multi-Output Device" inside BeSpec.**
The Multi-Output device is for *output only*. If you select it as an input, you will see a flat line. You must listen to the *destination* (BlackHole) that the Multi-Output device is feeding.

![Screenshot: BeSpec application with BlackHole 2ch selected in the dropdown]

---

## Troubleshooting

**"App is damaged and can't be opened"**
If the app refuses to launch, run this command in your Terminal to remove the Apple quarantine flag:
```bash
xattr -cr /Applications/BeSpec.app
```

**No Audio / Flat Spectrum**
1.  Check that **System Output** is set to "Multi-Output Device".
2.  Check that **BeSpec Input** is set to "BlackHole 2ch".
3.  Go to **System Settings > Privacy & Security > Microphone** and ensure BeSpec is toggled **ON**.

**Resetting Permissions**
If you accidentally clicked "Don't Allow" for the microphone, force a reset by running this command:
```bash
tccutil reset Microphone com.bespec-dev.bespec
```