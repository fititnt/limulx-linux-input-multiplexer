use evdev::uinput::VirtualDeviceBuilder;
use evdev::{Device, EventType, InputEvent, Key};
// Removed HashMap import as it is no longer needed
use std::sync::Arc;
use log::{debug, info};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};
use tokio_stream::StreamExt;
use serde::Deserialize;
use std::fs;
use clap::Parser;

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

    /// Initial delay in milliseconds (CLI mode)
    #[arg(long)]
    initial_delay: Option<u64>,

    /// Rapid fire delay in milliseconds (CLI mode)
    #[arg(long)]
    rapid_fire_delay: Option<u64>,

    /// Add a device path to multiplex (can be used multiple times, e.g., -d /dev/input/event5 -d /dev/input/event12)
    #[arg(short, long = "device")]
    devices: Option<Vec<String>>,
}

/// Configuration structure
#[derive(Deserialize, Debug, Clone)]
struct Config {
    initial_delay_ms: u64,
    rapid_fire_delay_ms: u64,
    devices: Vec<DeviceConfig>,
}

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
        let rapid_fire_delay_ms = args.rapid_fire_delay.unwrap_or(50);
        
        let devices = cli_devices.into_iter().map(|path| DeviceConfig {
            path,
            rapid_fire_keys: None, // CLI mode defaults to applying rapid-fire to all keys
        }).collect();

        Config {
            initial_delay_ms,
            rapid_fire_delay_ms,
            devices,
        }
    } else {
        let config_path = args.config.unwrap_or_else(|| "config.toml".to_string());
        info!("Loading configuration from file: {}", config_path);
        
        let config_contents = fs::read_to_string(&config_path)
            .unwrap_or_else(|_| panic!("Failed to read config file: {}. Please create it or use CLI arguments.", config_path));
        
        toml::from_str(&config_contents).expect("Failed to parse TOML configuration")
    };

    let initial_delay = Duration::from_millis(config.initial_delay_ms);
    let rapid_fire_delay = Duration::from_millis(config.rapid_fire_delay_ms);

    let (tx, mut rx) = mpsc::channel::<UinputMsg>(1024);

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

        let handle = tokio::spawn(async move {
            let mut device = Device::open(&path).unwrap_or_else(|_| panic!("Failed to open {}", path));
            
            // Requests exclusive access (prevents KWin from reading the original duplicate input)
            device.grab().expect("EVIOCGRAB error");

            // Converts the synchronous device into an asynchronous Tokio stream.
            let mut event_stream = device.into_event_stream().expect("Failed to create event stream");

            // Manages the per-key repeat timers for this device.
            // CHANGED: Instead of a HashMap, track at most one active key and its timer to mimic typical input handle on Windows
            let active_timer: Arc<Mutex<Option<(u16, JoinHandle<()>)>>> = Arc::new(Mutex::new(None));

            info!("Listening to device: {}", path);

            // Asynchronous loop does not block the worker thread
            while let Some(Ok(ev)) = event_stream.next().await {
                if ev.event_type() == EventType::KEY {
                    let key_code = ev.code();
                    let value = ev.value(); // 1 = Down, 0 = Up, 2 = Repeat (ignored by original OS)

                    // Determine if the current key is allowed to rapid-fire
                    let should_rapid_fire = match &rapid_fire_keys {
                        Some(allowed_keys) => allowed_keys.contains(&key_code),
                        None => true,
                    };

                    // CHANGED: Lock the single timer slot instead of the HashMap.
                    let mut timer_lock = active_timer.lock().await;

                    if value == 1 { // Only real key down
                        // Send original keydown
                        let _ = tx_clone.send(UinputMsg::Event(ev)).await;

                        if should_rapid_fire {
                            // ADDED: Abort the existing repeating key on this device, if any, so only the last held key repeats.
                            if let Some((_, handle)) = timer_lock.take() {
                                handle.abort();
                            }

                            let tx_timer = tx_clone.clone();
                            let timer_handle = tokio::spawn(async move {
                                // Await INITIAL_DELAY. If released before this, will not rapid fire
                                sleep(initial_delay).await;
                                debug!("VIRTUAL RAPID-FIRE: [{:?}] Started", key_code);

                                loop {
                                    sleep(rapid_fire_delay).await;
                                    // Virtual Key Up
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::KEY, key_code, 0))).await;
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::SYNCHRONIZATION, 0, 0))).await;
                                    
                                    sleep(rapid_fire_delay).await;
                                    // Virtual Key Down
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::KEY, key_code, 1))).await;
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::SYNCHRONIZATION, 0, 0))).await;
                                }
                            });
                            // CHANGED: Store the new key code and its timer as the active one.
                            *timer_lock = Some((key_code, timer_handle));

                        }
                    } else if value == 0 { // Only real key up
                        debug!("REAL              : [{:?}] Key UP", key_code);

                        // CHANGED: Only abort if the released key matches the currently repeating key.
                        if let Some((active_key, handle)) = timer_lock.take() {
                            if active_key == key_code {
                                handle.abort();
                                debug!("VIRTUAL RAPID-FIRE: [{:?}] Aborted", key_code);
                            } else {
                                // ADDED: It was a different key, put the timer back to keep repeating the active one.
                                *timer_lock = Some((active_key, handle));
                            }
                        }
                        // Send the original keyup
                        let _ = tx_clone.send(UinputMsg::Event(ev)).await;
                    }
                } else {
                    // Passes through other events (SYN, REL, ABS) intact.
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