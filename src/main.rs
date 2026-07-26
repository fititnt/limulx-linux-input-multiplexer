use evdev::uinput::VirtualDeviceBuilder;
use evdev::{Device, EventType, InputEvent, Key};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use log::{debug, info};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};
use tokio_stream::StreamExt;
use serde::Deserialize;
use std::fs;
use clap::{Parser, ValueEnum};

/// Movement Hotkey Layout Presets
/// 
/// Arrow keys (KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT) are ALWAYS included regardless of preset.
///
/// Layouts:
/// - `awsd`: WASD movement keys (KEY_W, KEY_A, KEY_S, KEY_D).
/// - `awsdqezc`: WASD + Diagonal movement keys (KEY_W, KEY_A, KEY_S, KEY_D, KEY_Q, KEY_E, KEY_Z, KEY_C).
/// - `numpad`: Numpad movement keys (KEY_KP1, KEY_KP2, KEY_KP3, KEY_KP4, KEY_KP5, KEY_KP6, KEY_KP7, KEY_KP8, KEY_KP9).
///   Note: Explicitly uses Numpad keycodes to prevent conflicts with main-row digits.
#[derive(ValueEnum, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum MoveHotkeysMode {
    Awsd,
    Awsdqezc,
    Numpad,
}

/// LIMux - Linux Input Multiplexer
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = "Multiplexes Linux input devices with rapid-fire capabilities.")]
struct Args {
    /// Sets a custom config file path (used if no CLI devices are provided)
    #[arg(short, long)]
    config: Option<String>,

    /// Turn on verbose output (displays debug info)
    #[arg(short, long)]
    verbose: bool,

    /// Initial delay in milliseconds (CLI mode). Set to <= 0 to disable virtual rapid-fire.
    #[arg(long, allow_hyphen_values = true)]
    initial_delay: Option<i64>,

    /// Rapid fire delay for default/non-movement keys in milliseconds. Default: 250ms.
    #[arg(long, allow_hyphen_values = true)]
    clock_default: Option<i64>,

    /// Rapid fire delay for movement keys in milliseconds. Default: 40ms.
    #[arg(long, allow_hyphen_values = true)]
    clock_move: Option<i64>,

    /// Preset for movement hotkeys that use `clock-move` speed. Options: awsd, awsdqezc, numpad.
    #[arg(long, value_enum)]
    move_hotkeys: Option<MoveHotkeysMode>,

    /// Add a device path to multiplex (can be used multiple times, e.g., -d /dev/input/event5 -d /dev/input/event12)
    #[arg(short, long = "device")]
    devices: Option<Vec<String>>,
}

/// Configuration structure
#[derive(Deserialize, Debug, Clone)]
struct Config {
    initial_delay_ms: i64,
    #[serde(default = "default_clock_default")]
    clock_default_ms: i64,
    #[serde(default = "default_clock_move")]
    clock_move_ms: i64,
    move_hotkeys: Option<MoveHotkeysMode>,
    devices: Vec<DeviceConfig>,
}

fn default_clock_default() -> i64 { 250 }
fn default_clock_move() -> i64 { 40 }

#[derive(Deserialize, Debug, Clone)]
struct DeviceConfig {
    path: String,
    /// Optional list of key codes. If None, rapid-fire applies to all keys.
    rapid_fire_keys: Option<Vec<u16>>,
}

#[derive(Debug)]
enum UinputMsg {
    Event(InputEvent),
}

/// Constructs the set of evdev keycodes corresponding to the chosen movement preset.
/// Arrow keys are automatically included in all sets.
fn build_move_key_set(mode: Option<&MoveHotkeysMode>) -> HashSet<u16> {
    let mut keys = HashSet::new();

    // ALWAYS include standard Arrow Keys
    keys.insert(Key::KEY_UP.code());
    keys.insert(Key::KEY_DOWN.code());
    keys.insert(Key::KEY_LEFT.code());
    keys.insert(Key::KEY_RIGHT.code());

    if let Some(m) = mode {
        match m {
            MoveHotkeysMode::Awsd => {
                keys.insert(Key::KEY_W.code());
                keys.insert(Key::KEY_A.code());
                keys.insert(Key::KEY_S.code());
                keys.insert(Key::KEY_D.code());
            }
            MoveHotkeysMode::Awsdqezc => {
                keys.insert(Key::KEY_W.code());
                keys.insert(Key::KEY_A.code());
                keys.insert(Key::KEY_S.code());
                keys.insert(Key::KEY_D.code());
                keys.insert(Key::KEY_Q.code());
                keys.insert(Key::KEY_E.code());
                keys.insert(Key::KEY_Z.code());
                keys.insert(Key::KEY_C.code());
            }
            MoveHotkeysMode::Numpad => {
                // Keypad numbers 1, 3, 4, 5, 6, 7, 8, 9
                keys.insert(Key::KEY_KP1.code());
                keys.insert(Key::KEY_KP2.code());
                keys.insert(Key::KEY_KP3.code());
                keys.insert(Key::KEY_KP4.code());
                keys.insert(Key::KEY_KP5.code());
                keys.insert(Key::KEY_KP6.code());
                keys.insert(Key::KEY_KP7.code());
                keys.insert(Key::KEY_KP8.code());
                keys.insert(Key::KEY_KP9.code());
            }
        }
    }

    keys
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // 1. Configure verbose mode dynamically
    if args.verbose {
        std::env::set_var("RUST_LOG", "debug");
    } else if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    // 2. Determine configuration source (CLI overrides TOML)
    let config = if let Some(cli_devices) = args.devices {
        info!("Running in CLI configuration mode.");
        let initial_delay_ms = args.initial_delay.unwrap_or(200);
        let clock_default_ms = args.clock_default.unwrap_or(250);
        let clock_move_ms = args.clock_move.unwrap_or(40);
        
        let devices = cli_devices.into_iter().map(|path| DeviceConfig {
            path,
            rapid_fire_keys: None,
        }).collect();

        Config {
            initial_delay_ms,
            clock_default_ms,
            clock_move_ms,
            move_hotkeys: args.move_hotkeys,
            devices,
        }
    } else {
        let config_path = args.config.unwrap_or_else(|| "config.toml".to_string());
        info!("Loading configuration from file: {}", config_path);
        
        let config_contents = fs::read_to_string(&config_path)
            .unwrap_or_else(|_| panic!("Failed to read config file: {}. Please create it or use CLI arguments.", config_path));
        
        toml::from_str(&config_contents).expect("Failed to parse TOML configuration")
    };

    let global_initial_delay_ms = config.initial_delay_ms;
    let clock_default_ms = config.clock_default_ms;
    let clock_move_ms = config.clock_move_ms;

    let initial_delay = Duration::from_millis(global_initial_delay_ms.max(0) as u64);
    let duration_default = Duration::from_millis(clock_default_ms.max(0) as u64);
    let duration_move = Duration::from_millis(clock_move_ms.max(0) as u64);

    let move_key_set = build_move_key_set(config.move_hotkeys.as_ref());

    let (tx, mut rx) = mpsc::channel::<UinputMsg>(1024);

    // Global tracking of active key holds across all devices (Key Code -> Device Path)
    let global_pressed_keys: Arc<Mutex<HashMap<u16, String>>> = Arc::new(Mutex::new(HashMap::new()));

    // 3. Configure the virtual uinput
    let mut keys = evdev::AttributeSet::<Key>::new();

    // Keys to watch for
    for i in 1..255 {
        keys.insert(Key::new(i));
    }

    let mut virtual_device = VirtualDeviceBuilder::new()?
        .name("LIMulx Linux Multiplexer Virtual Keyboard")
        .with_keys(&keys)?
        .build()?;

    // Main thread for dispatching events to uinput
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match msg {
                UinputMsg::Event(ev) => {
                    let _ = virtual_device.emit(&[ev]);
                }
            }
        }
    });

    // 4. Process each physical device
    let mut handles = vec![];

    for device_config in config.devices {
        let tx_clone = tx.clone();
        let path = device_config.path.clone();
        let rapid_fire_keys = device_config.rapid_fire_keys.clone();
        let global_pressed_keys = global_pressed_keys.clone();
        let move_key_set = move_key_set.clone();

        let handle = tokio::spawn(async move {
            let mut device = Device::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", path));
            
            // Requests exclusive access (prevents KWin from reading the original duplicate input)
            device.grab().expect("EVIOCGRAB error");

            // Converts the synchronous device into an asynchronous Tokio stream.
            let mut event_stream = device.into_event_stream().expect("Failed to create event stream");

            // Manages the per-key repeat timers for this device.
            let active_timer: Arc<Mutex<Option<(u16, JoinHandle<()>)>>> = Arc::new(Mutex::new(None));

            info!("Listening to device: {}", path);

            // Asynchronous loop does not block the worker thread
            while let Some(Ok(ev)) = event_stream.next().await {
                if ev.event_type() == EventType::KEY {
                    let key_code = ev.code();
                    let value = ev.value(); // 1 = Down, 0 = Up, 2 = Repeat

                    // Select rapid-fire interval based on key type (movement vs. default)
                    let is_move_key = move_key_set.contains(&key_code);
                    let active_interval_ms = if is_move_key { clock_move_ms } else { clock_default_ms };
                    let rapid_fire_delay = if is_move_key { duration_move } else { duration_default };

                    // Determine if rapid-fire applies
                    let should_rapid_fire = if global_initial_delay_ms <= 0 || active_interval_ms <= 0 {
                        false
                    } else {
                        match &rapid_fire_keys {
                            Some(allowed_keys) => allowed_keys.contains(&key_code),
                            None => true,
                        }
                    };

                    // Lock the single timer slot instead of the HashMap.
                    let mut timer_lock = active_timer.lock().await;

                    if value == 1 { // Real Key Down
                        debug!("REAL              : [{:?}] Key DOWN", key_code);

                        // Register key press in global state tracker
                        {
                            let mut global_keys = global_pressed_keys.lock().await;
                            global_keys.insert(key_code, path.clone());
                        }

                         // Send original keydown
                        let _ = tx_clone.send(UinputMsg::Event(ev)).await;

                        if should_rapid_fire {
                            if let Some((_, handle)) = timer_lock.take() {
                                handle.abort();
                            }

                            let tx_timer = tx_clone.clone();
                            let timer_handle = tokio::spawn(async move {
                                sleep(initial_delay).await;
                                debug!("VIRTUAL RAPID-FIRE: [{:?}] Started (Interval: {}ms)", key_code, active_interval_ms);

                                loop {
                                    // Virtual Key Up
                                    // sleep(rapid_fire_delay).await;
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::KEY, key_code, 0))).await;
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::SYNCHRONIZATION, 0, 0))).await;

                                    // Virtual Key Down
                                    sleep(rapid_fire_delay).await;
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::KEY, key_code, 1))).await;
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::SYNCHRONIZATION, 0, 0))).await;
                                }
                            });
                            *timer_lock = Some((key_code, timer_handle));
                        }
                    } else if value == 0 { // Real Key Up
                        debug!("REAL              : [{:?}] Key UP", key_code);

                        // Remove key press from global state tracker
                        {
                            let mut global_keys = global_pressed_keys.lock().await;
                            global_keys.remove(&key_code);
                        }

                        if let Some((active_key, handle)) = timer_lock.take() {
                            if active_key == key_code {
                                handle.abort();
                                debug!("VIRTUAL RAPID-FIRE: [{:?}] Aborted", key_code);
                            } else {
                                *timer_lock = Some((active_key, handle));
                            }
                        }
                        let _ = tx_clone.send(UinputMsg::Event(ev)).await;
                    } else if value == 2 { // Physical Device Repeat
                        if !should_rapid_fire {
                            // Check how many keys are held across all devices
                            let global_keys = global_pressed_keys.lock().await;
                            
                            if global_keys.len() <= 1 {
                                debug!("REAL              : [{:?}] Key REPEAT (Native Proxy)", key_code);
                                let _ = tx_clone.send(UinputMsg::Event(ev)).await;
                            } else {
                                // MULTIPLE KEYS ACTIVE: Force synthetic Up/Down to interleave cross-device inputs
                                debug!("REAL              : [{:?}] Key REPEAT (Interleaved Burst)", key_code);
                                let _ = tx_clone.send(UinputMsg::Event(InputEvent::new(EventType::KEY, key_code, 0))).await;
                                let _ = tx_clone.send(UinputMsg::Event(InputEvent::new(EventType::SYNCHRONIZATION, 0, 0))).await;
                                let _ = tx_clone.send(UinputMsg::Event(InputEvent::new(EventType::KEY, key_code, 1))).await;
                            }
                        }
                    }
                } else {
                    // Passes through other events (SYN, REL, ABS, MSC) intact.
                    let _ = tx_clone.send(UinputMsg::Event(ev)).await;
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    Ok(())
}