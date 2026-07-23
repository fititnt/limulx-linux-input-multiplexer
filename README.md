# linux-input-multiplexer
Linux input multiplexer with independent per-device auto-repeat.
Bypasses compositor Seat limitations to enable true Windows-like raw input
and asynchronous/fast multi-device response
(TL;DR: also good competitive gaming).


## Quickstart usage

```
# compile release version
cargo build --release

# Example usage with command line arguments and -v verbose
sudo ./target/release/linux-input-multiplexer -v --initial-delay 250 --rapid-fire-delay 60 -d /dev/input/event5 -d /dev/input/event12

# Example usage with command line arguments and -v verbose and arguments saved as file
sudo ./target/release/linux-input-multiplexer -v -c config.example.toml
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
sudo RUST_LOG=debug ./target/debug/linux-input-multiplexer -c config.example.toml



cargo build --release
sudo ./target/release/linux-input-multiplexer -v --initial-delay 250 --rapid-fire-delay 60 -d /dev/input/event5 -d /dev/input/event12

-->

## FAQ

### Error "EVIOCGRAB error: Os { code: 16, kind: ResourceBusy, message: "Device or resource busy" }"

This error likely indicates that another application is already claiming your device (keyboard, mouse, footswitch...).

- Did you left more than one instance of this app running?
- Are you with permission to "lock" the device?
  - If you select also you main keyboard, you may try run as sudo
  - TODO: not to self, investigate if adding running user to some usergroup may allow not run as sudo.