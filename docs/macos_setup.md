# macOS Audio Setup Guide

Unlike Windows or Linux, macOS does not inherently allow applications to "hear" the system audio output (loopback) due to security restrictions.

To use **BeSpec** with your system audio (e.g., music from Spotify, YouTube, or Apple Music), you need to set up a virtual loopback driver. We recommend **BlackHole**, which is free, open-source, and low-latency.

## Step 1: Install BlackHole

You need the **2-channel** version of BlackHole.

**Option A: Via Homebrew (Recommended)**
Open your Terminal and run:

```bash
brew install blackhole-2ch
```

**Option B: Manual Download**
Download the installer from [existential.audio](https://existential.audio/blackhole/) and follow the install prompts.

## Step 2: Create a Multi-Output Device

This step creates a virtual "splitter" that sends audio to your speakers AND to BeSpec simultaneously.

1. Open **Audio MIDI Setup** (Cmd+Space, type "Audio MIDI Setup").

2. Click the **+** (plus) icon in the bottom-left corner.

3. Select **Create Multi-Output Device**.

4. In the list on the right, check the boxes for:

   * **Built-in Output** (or your Headphones/External DAC).

   * **BlackHole 2ch**.

5. **Critical Settings:**

   * **Master Device:** Set this to your physical speakers (e.g., "Built-in Output"). This ensures your audio clock matches your hardware.

   * **Drift Correction:** Check the box next to **BlackHole 2ch**. This tells macOS to keep BlackHole in sync with your speakers.

## Step 3: Route System Audio

1. Open **System Settings** > **Sound**.

2. Under the **Output** tab, select the **Multi-Output Device** you just created.

> **Note:** When using a Multi-Output device, macOS disables the volume keys on your keyboard. You must control volume directly via your physical speaker buttons or inside the specific music app.

## Troubleshooting: "App is damaged and can't be opened"

If you receive an error stating that **"BeSpec.app is damaged and can't be opened"** when trying to run the application, this is a standard macOS security message for apps that are not signed with an Apple Developer ID.

To fix this, you need to remove the "quarantine" attribute from the downloaded file:

1. Move the **BeSpec** app to your `/Applications` folder (or wherever you prefer).

2. Open your **Terminal**.

3. Run the following command:

   ```bash
   xattr -cr /Applications/BeSpec.app
   ```

   *(Note: If you placed the app somewhere else, replace the path accordingly. You can also type `xattr -cr ` and drag the app icon into the terminal window to auto-fill the path.)*

4. Launch the app again. It should now open normally.