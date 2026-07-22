use evdev::uinput::VirtualDeviceBuilder;
use evdev::{Device, EventType, InputEvent, Key};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

// Defina o intervalo do "spam" (ex: 50ms = 20 clicks por segundo)
const RAPID_FIRE_DELAY: Duration = Duration::from_millis(50);

// Mensagens enviadas para a thread central do uinput
#[derive(Debug)]
enum UinputMsg {
    Event(InputEvent),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Underlining real events (replace by real eventX)
    // Tip: use `cat /proc/bus/input/devices` para achar os IDs corretos.
    let devices = vec![
        // "/dev/input/eventX", // Teclado Principal
        // "/dev/input/eventY", // Mouse Redragon
        // "/dev/input/eventZ", // Footswitch


        // ls -l /dev/input/by-id/
        //     usb-04d9_USB_Gaming_Mouse-if01-event-kbd -> ../event5
        //     usb-PCsensor_FootSwitch-event-kbd -> ../event12
        // "/dev/input/eventX", // Teclado Principal
        "/dev/input/event5", // Mouse Redragon
        "/dev/input/event12", // Footswitch
    ];

    let (tx, mut rx) = mpsc::channel::<UinputMsg>(1024);

    // 2. Configurar o dispositivo virtual uinput
    let mut keys = evdev::AttributeSet::<Key>::new();
    // Adicione todas as teclas que o dispositivo virtual precisa suportar
    for i in 1..255 {
        keys.insert(Key::new(i));
    }

    let mut virtual_device = VirtualDeviceBuilder::new()?
        .name("LIMux Linux Multiplexer Virtual Keyboard")
        .with_keys(&keys)?
        .build()?;

    // Thread central para despachar eventos para o uinput
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match msg {
                UinputMsg::Event(ev) => {
                    let _ = virtual_device.emit(&[ev]);
                }
            }
        }
    });

    // 3. Processar cada dispositivo físico
    let mut handles = vec![];

    for dev_path in devices {
        let tx_clone = tx.clone();
        let path = dev_path.to_string();

        let handle = tokio::spawn(async move {
            let mut device = Device::open(&path).expect(&format!("Falha ao abrir {}", path));
            
            // Requests exclusive access (prevents KWin from reading the original duplicate input)
            device.grab().expect("Falha no EVIOCGRAB");

            // Manages the per-key repeat timers for this device.
            let active_timers: Arc<Mutex<HashMap<u16, JoinHandle<()>>>> = Arc::new(Mutex::new(HashMap::new()));

            loop {
                // Fetch events blocking
                // for ev in device.fetch_events().unwrap_or_default() {
                // for ev in device.fetch_events().unwrap_or(Vec::new()) {
                for ev in device.fetch_events().expect("Falha ao buscar eventos do dispositivo") {
                    if ev.event_type() == EventType::KEY {
                        let key_code = ev.code();
                        let value = ev.value(); // 1 = Down, 0 = Up, 2 = Repeat (ignored pelo SO original)

                        let mut timers = active_timers.lock().await;

                        if value == 1 {
                            // Envia o keydown original
                            let _ = tx_clone.send(UinputMsg::Event(ev)).await;

                            // If have to, maybe create a filter to only rapid-fire mouse/footswitch keys?
                            let tx_timer = tx_clone.clone();
                            let timer_handle = tokio::spawn(async move {
                                loop {
                                    sleep(RAPID_FIRE_DELAY).await;
                                    // Simula Key Up
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::KEY, key_code, 0))).await;
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::SYNCHRONIZATION, 0, 0))).await;
                                    sleep(RAPID_FIRE_DELAY).await;
                                    // Simula Key Down
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::KEY, key_code, 1))).await;
                                    let _ = tx_timer.send(UinputMsg::Event(InputEvent::new(EventType::SYNCHRONIZATION, 0, 0))).await;
                                }
                            });
                            timers.insert(key_code, timer_handle);

                        } else if value == 0 {
                            // Cancels the timer if it exists
                            if let Some(handle) = timers.remove(&key_code) {
                                handle.abort();
                            }
                            // Send the keyup original
                            let _ = tx_clone.send(UinputMsg::Event(ev)).await;
                        }
                    } else {
                        // Passes through other events (SYN, REL, ABS) intact.
                        let _ = tx_clone.send(UinputMsg::Event(ev)).await;
                    }
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