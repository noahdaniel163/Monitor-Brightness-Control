use crossbeam_channel::{Receiver, Sender};
use ddc_hi::{Ddc, Display};
use std::{collections::HashMap, thread, time::Duration};

pub enum Command {
    Refresh,
    Set { id: String, code: u8, value: u16 },
}

#[derive(Clone)]
pub struct MonitorSnapshot {
    pub id: String,
    pub name: String,
    pub supported: bool,
    pub brightness_supported: bool,
    pub contrast_supported: bool,
    pub brightness: u16,
    pub contrast: u16,
    pub brightness_max: u16,
    pub contrast_max: u16,
    pub error: Option<String>,
}

impl MonitorSnapshot {
    pub fn error(message: String) -> Self {
        Self {
            id: String::new(),
            name: "No DDC/CI monitor detected".into(),
            supported: false,
            brightness_supported: false,
            contrast_supported: false,
            brightness: 0,
            contrast: 0,
            brightness_max: 100,
            contrast_max: 100,
            error: Some(message),
        }
    }
}
pub enum WorkerEvent {
    Snapshot(Vec<MonitorSnapshot>),
    Error(String),
}

pub fn spawn(commands: Receiver<Command>, events: Sender<WorkerEvent>) {
    thread::Builder::new()
        .name("ddc-worker".into())
        .spawn(move || {
            let mut displays: Vec<Display> = Vec::new();
            let mut maxima: HashMap<(String, u8), u16> = HashMap::new();
            loop {
                match commands.recv_timeout(Duration::from_millis(20)) {
                    Ok(Command::Refresh) => scan(&mut displays, &events, &mut maxima),
                    Ok(Command::Set { id, code, value }) => {
                        // Khi keo nhanh, bo qua cac gia tri trung gian da cu trong hang doi.
                        let mut latest = HashMap::new();
                        latest.insert((id, code), value);
                        while let Ok(command) = commands.try_recv() {
                            match command {
                                Command::Refresh => scan(&mut displays, &events, &mut maxima),
                                Command::Set { id, code, value } => {
                                    latest.insert((id, code), value);
                                }
                            }
                        }
                        for ((id, code), value) in latest {
                            set_value(&mut displays, &mut maxima, &events, &id, code, value);
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("failed to start DDC worker");
}

fn scan(
    displays: &mut Vec<Display>,
    events: &Sender<WorkerEvent>,
    maxima: &mut HashMap<(String, u8), u16>,
) {
    *displays = Display::enumerate();
    maxima.clear();
    let mut snapshots = Vec::new();
    for display in displays.iter_mut() {
        let id = display.info.id.clone();
        let name = display
            .info
            .model_name
            .clone()
            .or_else(|| display.info.manufacturer_id.clone())
            .unwrap_or_else(|| id.clone());
        let brightness_result = display.handle.get_vcp_feature(0x10);
        let contrast_result = display.handle.get_vcp_feature(0x12);
        match (brightness_result, contrast_result) {
            (Ok(brightness), Ok(contrast)) => {
                maxima.insert((id.clone(), 0x10), brightness.maximum());
                maxima.insert((id.clone(), 0x12), contrast.maximum());
                snapshots.push(MonitorSnapshot {
                    id,
                    name,
                    supported: true,
                    brightness_supported: true,
                    contrast_supported: true,
                    brightness: percent(brightness.value(), brightness.maximum()),
                    contrast: percent(contrast.value(), contrast.maximum()),
                    brightness_max: 100,
                    contrast_max: 100,
                    error: None,
                });
            }
            (brightness, contrast) => {
                let brightness_supported = brightness.is_ok();
                let contrast_supported = contrast.is_ok();
                let brightness = brightness.ok();
                let contrast = contrast.ok();
                if let Some(value) = &brightness {
                    maxima.insert((id.clone(), 0x10), value.maximum());
                }
                if let Some(value) = &contrast {
                    maxima.insert((id.clone(), 0x12), value.maximum());
                }
                snapshots.push(MonitorSnapshot {
                    id,
                    name,
                    supported: brightness_supported || contrast_supported,
                    brightness_supported,
                    contrast_supported,
                    brightness: brightness
                        .as_ref()
                        .map(|v| percent(v.value(), v.maximum()))
                        .unwrap_or(0),
                    contrast: contrast
                        .as_ref()
                        .map(|v| percent(v.value(), v.maximum()))
                        .unwrap_or(0),
                    brightness_max: 100,
                    contrast_max: 100,
                    error: if brightness_supported || contrast_supported {
                        None
                    } else {
                        Some("DDC/CI brightness and contrast are unavailable.".into())
                    },
                });
            }
        }
    }
    if snapshots.is_empty() {
        snapshots.push(MonitorSnapshot::error(
            "No DDC/CI-compatible monitor found.".into(),
        ));
    }
    let _ = events.send(WorkerEvent::Snapshot(snapshots));
}

fn percent(value: u16, max: u16) -> u16 {
    if max == 0 {
        0
    } else {
        ((value as u32 * 100) / max as u32).min(100) as u16
    }
}

fn set_value(
    displays: &mut [Display],
    maxima: &mut HashMap<(String, u8), u16>,
    events: &Sender<WorkerEvent>,
    id: &str,
    code: u8,
    value: u16,
) {
    if let Some(display) = displays.iter_mut().find(|d| d.info.id == id) {
        let result = maxima
            .get(&(id.to_owned(), code))
            .copied()
            .ok_or_else(|| "VCP range has not been read yet".to_owned())
            .and_then(|maximum| {
                display
                    .handle
                    .set_vcp_feature(code, ((value as u32 * maximum as u32) / 100) as u16)
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = result {
            let _ = events.send(WorkerEvent::Error(format!(
                "Cannot write VCP 0x{code:02X}: {error}"
            )));
        }
    }
}
