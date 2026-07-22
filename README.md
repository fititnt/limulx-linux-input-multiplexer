# linux-input-multiplexer
Linux input multiplexer with independent per-device auto-repeat.
Bypasses compositor Seat limitations to enable true Windows-like raw input
and asynchronous/fast multi-device response
(TL;DR: also good competitive gaming).


## Compile

```
#...
```


```
RUST_LOG=debug cargo run
```

## Finding your inout devices

```
# 1.
cat /proc/bus/input/devices

# 2. (recommended)
sudo evtest

# 3. Persystent naming
ls -l /dev/input/by-id/
```


<!--

sudo RUST_LOG=debug ./target/debug/input_multiplexer


# 1. Build the debug version
cargo build

# 2. Run the generated binary with root privileges and the logger enabled
sudo RUST_LOG=debug ./target/debug/linux-input-multiplexer
-->