# macOS Audio Setup Guide

Unlike Windows or Linux, macOS does not inherently allow applications to "hear" the system audio output (loopback) due to security restrictions.

To use **BeAnal** with your system audio (e.g., music from Spotify, YouTube, or Apple Music), you need to set up a virtual loopback driver. We recommend **BlackHole**, which is free, open-source, and low-latency.

## Step 1: Install BlackHole

You need the **2-channel** version of BlackHole.

**Option A: Via Homebrew (Recommended)**
Open your Terminal and run:
```bash
brew install blackhole-2ch
```

**Option B: Manual Download**
Download the installer from [existential.audio](https://existential.audio/blackhole/) and follow the install prompts.

---

## Step 2: Create a Multi-Output Device

This step creates a virtual "splitter" that sends audio to your speakers AND to BeAnal simultaneously.

1. Open **Audio MIDI Setup** (Cmd+Space, type "Audio MIDI Setup").
2. Click the **+** (plus) icon in the bottom-left corner.
3. Select **Create Multi-Output Device**.
4. In the list on the right, check the boxes for:
   - **Built-in Output** (or your Headphones/External DAC).
   - **BlackHole 2ch**.
5. **Critical Settings:**
   - **Master Device:** Set this to your physical speakers (e.g., "Built-in Output"). This ensures your audio clock matches your hardware.
   - **Drift Correction:** Check the box next to **BlackHole 2ch**. This tells macOS to keep BlackHole in sync with your speakers.

---

## Step 3: Route System Audio

1. Open **System Settings** > **Sound**.
2. Under the **Output** tab, select the **Multi-Output Device** you just created.

> **Note:** When using a Multi-Output device, macOS disables the volume keys on your keyboard. You must control volume directly via your physical speaker buttons or inside the specific music app
