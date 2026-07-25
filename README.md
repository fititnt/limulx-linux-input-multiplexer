# LIMulx Linux input multiplexer (v0.3.0 preview)
Linux input multiplexer with independent per-device auto-repeat.
Bypasses compositor Seat limitations to enable true Windows-like raw input
and asynchronous/fast multi-device response
(TL;DR: also good competitive gaming).


> **TL;DR:** Using this program while holding down "W" on a keyboard,
> side button "7" on a gaming mouse, and "F13" on a footswitch will output
> "W 7 F13 W 7 F13 W 7 F13 W 7 F13..." instead of only "F13 F13 F13 F13...".
> This makes desktop Linux behave more like Windows,
> without requiring cross-platform apps or games to (which would be a good idea) rework their internal input logic on Linux.


```bash
## 1. Download the latest release
# check https://github.com/fititnt/limulx-linux-input-multiplexer/releases for the version. As example:
wget https://github.com/fititnt/limulx-linux-input-multiplexer/releases/download/v0.3.0/linux-input-multiplexer-amd64

## 2. Make the downloaded binary executable
chmod +x limulx-linux-input-multiplexer-amd64

## 3. Run the application (replace the /dev/input/eventX paths with your own)
# Use rapid fire from the software
sudo ./limulx-linux-input-multiplexer-amd64 -v --initial-delay 200 --rapid-fire-delay 50 -d /dev/input/event5 -d /dev/input/event12 -d /dev/input/event10

# ALTERNATIVE: disable software rapid fire, and use the one from your OS
sudo ./limulx-linux-input-multiplexer-amd64 -v --initial-delay 200 --rapid-fire-delay -1 -d /dev/input/event5 -d /dev/input/event12 -d /dev/input/event10
```

## About LIMulx

### What This App Does

Applications (most commonly competitive games) that expect fast and reactive inputs often exhibit different behavior between Windows and Linux.
On Windows, developers typically optimize for performance using the Raw Input API.
On Linux, the input subsystem accurately reports *which* keys are pressed or released across multiple devices,
but the display server (X11/Wayland) will typically only generate automatic "repeat" events for the *most recently pressed* key.

**To developers:** To mimic Windows behavior natively,
Linux applications (which already receive raw key press/release events)
need to manually compute and generate repeat events for all held keys,
rather than relying on the OS's single-key repeat timer.
If your application does this, your users do not need this app.

**To Linux users:** This app creates a virtual keyboard named `LIMulx Linux Multiplexer Virtual Keyboard` which computes the final result of multiple hardware inputs you select
(e.g., your footswitch, gaming mouse side buttons, gamepad, main keyboard).
It reports the initial key presses,
and then (while the buttons are held) fires repeat events for all of them,
alternating between the different hardware inputs instead of just repeating the last one.

#### Is this a macro? Does it offer an unfair advantage?

This app is open-source, works exactly as described above,
and is intended solely as a fallback for apps that do not handle Linux Human Interface Devices (HID) properly.

Other than adjusting the repeat rate (a feature already built into the OS),
it does not allow the end user to customize behavior. It stops repeating as soon as the physical hardware reports a key release.

**To developers:** If your software already handles Linux inputs correctly and you are tuning anti-cheat measures,
you can check for this virtual keyboard's device name and instruct the user to disable it before running your game.

#### Can this steal my information?

This software is written in Rust by @fititnt.
It can be reviewed and compiled from source without needing to download the pre-compiled binaries from GitHub (which are provided purely for the sake of simplicity).

While it is generally a bad practice to install executables from untrusted sources,
this one requires special caution because it reads keystrokes.
Please do not install binaries from unauthorized forks you dont already know the authors,
as a malicious actor could alter the code or add an insecure dependency to log your inputs.


## Quickstart usage

@TODO improve this part

```
# compile release version
cargo build --release

# Example usage with command line arguments and -v verbose
sudo ./target/release/limulx-linux-input-multiplexer -v --initial-delay 200 --rapid-fire-delay 50 -d /dev/input/event5 -d /dev/input/event12 -d /dev/input/event10

# Example usage with command line arguments and -v verbose and arguments saved as file
sudo ./target/release/limulx-linux-input-multiplexer-v -c config.example.toml
```

## Finding your inout devices

```
# 1.
cat /proc/bus/input/devices

# 2. (recommended)
sudo evtest

# 3. Persistent naming
ls -l /dev/input/by-id/
```

<!--

sudo RUST_LOG=debug ./target/debug/input_multiplexer


# 1. Build the debug version
cargo build
sudo RUST_LOG=debug ./target/debug/limulx-linux-input-multiplexer r -c config.example.toml



cargo build --release
sudo ./target/release/limulx-linux-input-multiplexer r -v --initial-delay 250 --rapid-fire-delay 60 -d /dev/input/event5 -d /dev/input/event12 -d /dev/input/event10

On Windows:
SPI_SETKEYBOARDSPEED; from 0 (2.5 r/s) to 31 (30/s); maximum on windows around 30.3r/s or 33ms repeat
SPI_SETKEYBOARDDELAY: 0 (250s) to 3 (1s)


-->

## FAQ

### Error "EVIOCGRAB error: Os { code: 16, kind: ResourceBusy, message: "Device or resource busy" }"

This error likely indicates that another application is already claiming your device (keyboard, mouse, footswitch...).

- Did you left more than one instance of this app running?
- Are you with permission to "lock" the device?
  - If you select also you main keyboard, you may try run as sudo
  - TODO: not to self, investigate if adding running user to some usergroup may allow not run as sudo.