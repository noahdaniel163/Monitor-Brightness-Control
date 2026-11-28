#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ddc_worker;

use crossbeam_channel::{unbounded, Receiver, Sender};
use ddc_worker::{Command, MonitorSnapshot, WorkerEvent};
use eframe::egui::{self, Color32, RichText, Vec2};
use image::ImageReader;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const TITLE: &str = "Display Control";
const VERSION: &str = "v0.1.0";
const ACCENT: Color32 = Color32::from_rgb(88, 166, 255);
const BG: Color32 = Color32::from_rgb(15, 18, 24);
const CARD: Color32 = Color32::from_rgb(25, 30, 40);
const TRACK: Color32 = Color32::from_rgb(52, 61, 76);

fn fluent_slider(ui: &mut egui::Ui, value: &mut u16, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 22.0),
        egui::Sense::click_and_drag(),
    );
    let ratio = (*value as f32 / 100.0).clamp(0.0, 1.0);
    let mut response = response;
    if enabled && (response.is_pointer_button_down_on() || response.clicked()) {
        if let Some(x) = response.interact_pointer_pos() {
            let next =
                (((x.x - rect.left()) / rect.width()).clamp(0.0, 1.0) * 100.0).round() as u16;
            if next != *value {
                *value = next;
                response.mark_changed();
            }
        }
    }
    let fill = if enabled {
        ACCENT
    } else {
        Color32::from_rgb(76, 82, 94)
    };
    let track = if enabled {
        TRACK
    } else {
        Color32::from_rgb(39, 44, 53)
    };
    let y = rect.center().y;
    ui.painter().line_segment(
        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
        egui::Stroke::new(6.0_f32, track),
    );
    ui.painter().line_segment(
        [
            egui::pos2(rect.left(), y),
            egui::pos2(rect.left() + rect.width() * ratio, y),
        ],
        egui::Stroke::new(6.0_f32, fill),
    );
    ui.painter()
        .circle_filled(egui::pos2(rect.left() + rect.width() * ratio, y), 7.0, fill);
    response
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MonitorControlType {
    Brightness,
    Contrast,
}

struct MonitorControlRow<'a> {
    monitor_id: &'a str,
    control: MonitorControlType,
    enabled: bool,
}

impl<'a> MonitorControlRow<'a> {
    fn show(
        &self,
        ui: &mut egui::Ui,
        value: &mut u16,
        on_change: &mut dyn FnMut(&str, MonitorControlType, u16),
    ) {
        let (icon, label, color) = match self.control {
            MonitorControlType::Brightness => ("☀", "Brightness", Color32::from_rgb(255, 205, 104)),
            MonitorControlType::Contrast => ("◐", "Contrast", Color32::from_rgb(178, 154, 255)),
        };
        let row_width = ui.available_width().max(120.0);
        ui.allocate_ui_with_layout(
            egui::vec2(row_width, 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_sized(
                    [104.0, 22.0],
                    egui::Label::new(
                        RichText::new(format!("{}  {}", icon, label))
                            .small()
                            .color(color),
                    ),
                );
                let slider_width = (ui.available_width() - 48.0).max(32.0);
                let changed = ui
                    .allocate_ui(egui::vec2(slider_width, 22.0), |ui| {
                        fluent_slider(ui, value, self.enabled)
                    })
                    .inner
                    .changed();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_sized(
                        [48.0, 22.0],
                        egui::Label::new(RichText::new(format!("{}%", *value)).strong()),
                    );
                });
                if changed {
                    on_change(self.monitor_id, self.control, *value);
                }
            },
        );
    }
}

fn load_egui_icon() -> Arc<egui::IconData> {
    let image = ImageReader::new(std::io::Cursor::new(include_bytes!("icon.ico")))
        .with_guessed_format()
        .expect("invalid icon")
        .decode()
        .expect("cannot decode icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    Arc::new(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn config_path() -> Result<PathBuf, String> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .map(|p| p.join("config.json"))
        .ok_or_else(|| "Cannot resolve the application directory for config.json".into())
}
#[derive(Clone, Serialize, Deserialize, Default)]
struct MonitorConfig {
    #[serde(default)]
    custom_name: Option<String>,
    #[serde(default)]
    brightness: Option<u16>,
    #[serde(default)]
    contrast: Option<u16>,
    #[serde(default)]
    brightness_supported: Option<bool>,
    #[serde(default)]
    contrast_supported: Option<bool>,
}
#[derive(Clone, Serialize, Deserialize)]
struct WindowConfig {
    width: f32,
    height: f32,
    x: f32,
    y: f32,
    maximized: bool,
}
impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 430.0,
            height: 850.0,
            x: 100.0,
            y: 100.0,
            maximized: false,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct SettingsConfig {
    theme: String,
    always_on_top: bool,
    auto_detect: bool,
    compact_mode: bool,
}
impl Default for SettingsConfig {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            always_on_top: false,
            auto_detect: true,
            compact_mode: false,
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Default)]
struct AppConfig {
    #[serde(default)]
    window: WindowConfig,
    #[serde(default)]
    monitors: HashMap<String, MonitorConfig>,
    #[serde(default)]
    settings: SettingsConfig,
}
fn load_config() -> (AppConfig, Option<String>) {
    let path = match config_path() {
        Ok(path) => path,
        Err(error) => return (AppConfig::default(), Some(error)),
    };
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(config) => (config, None),
            Err(error) => (
                AppConfig::default(),
                Some(format!("config.json is invalid: {error}")),
            ),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let config = AppConfig::default();
            (config.clone(), save_config(&config).err())
        }
        Err(error) => (
            AppConfig::default(),
            Some(format!("Cannot read config.json: {error}")),
        ),
    }
}
fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = config_path()?;
    let temp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&temp, text).map_err(|e| format!("Cannot write config.json: {e}"))?;
    fs::rename(&temp, &path).map_err(|e| format!("Cannot replace config.json atomically: {e}"))
}

struct MonitorApp {
    tx: Sender<Command>,
    rx: Receiver<WorkerEvent>,
    monitors: Vec<MonitorSnapshot>,
    config: AppConfig,
    config_error: Option<String>,
    editing: Option<String>,
    draft_name: String,
    settings_open: bool,
    always_on_top: bool,
    auto_detect: bool,
    last_refresh: Instant,
}

impl MonitorApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.window_fill = BG;
        visuals.panel_fill = BG;
        visuals.faint_bg_color = Color32::from_rgb(31, 37, 48);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(39, 46, 59);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(53, 66, 86);
        visuals.widgets.active.bg_fill = Color32::from_rgb(64, 126, 205);
        visuals.selection.bg_fill = ACCENT;
        cc.egui_ctx.set_visuals(visuals);
        let (tx, commands) = unbounded();
        let (events, rx) = unbounded();
        ddc_worker::spawn(commands, events);
        let _ = tx.send(Command::Refresh);
        let (config, config_error) = load_config();
        let always_on_top = config.settings.always_on_top;
        let auto_detect = config.settings.auto_detect;
        Self {
            tx,
            rx,
            monitors: Vec::new(),
            config,
            config_error,
            editing: None,
            draft_name: String::new(),
            settings_open: false,
            always_on_top,
            auto_detect,
            last_refresh: Instant::now(),
        }
    }
    fn refresh(&mut self) {
        self.last_refresh = Instant::now();
        let _ = self.tx.send(Command::Refresh);
    }
    fn send_value(&self, id: &str, code: u8, value: u16) {
        let _ = self.tx.send(Command::Set {
            id: id.to_owned(),
            code,
            value,
        });
    }
    fn set_name(&mut self, id: String) {
        let name = self.draft_name.trim();
        self.config.monitors.entry(id).or_default().custom_name = if name.is_empty() {
            None
        } else {
            Some(name.to_owned())
        };
        self.save_config();
        self.editing = None;
    }
    fn save_config(&mut self) {
        self.config.settings.always_on_top = self.always_on_top;
        self.config.settings.auto_detect = self.auto_detect;
        if let Err(error) = save_config(&self.config) {
            self.config_error = Some(error);
        }
    }

    fn capture_window(&mut self, ctx: &egui::Context) {
        let viewport = ctx.input(|input| input.viewport().clone());
        if let Some(rect) = viewport.inner_rect {
            let size = rect.size();
            let position = viewport.outer_rect.map(|r| r.min).unwrap_or(rect.min);
            if size.x.is_finite()
                && size.y.is_finite()
                && position.x.is_finite()
                && position.y.is_finite()
            {
                let width = size.x.clamp(360.0, 1600.0);
                let height = size.y.clamp(430.0, 1600.0);
                let x = position.x.clamp(-3000.0, 10000.0);
                let y = position.y.clamp(-3000.0, 10000.0);
                let changed = (self.config.window.width - width).abs() > 1.0
                    || (self.config.window.height - height).abs() > 1.0
                    || (self.config.window.x - x).abs() > 1.0
                    || (self.config.window.y - y).abs() > 1.0
                    || self.config.window.maximized != viewport.maximized.unwrap_or(false);
                if changed {
                    self.config.window = WindowConfig {
                        width,
                        height,
                        x,
                        y,
                        maximized: viewport.maximized.unwrap_or(false),
                    };
                    self.save_config();
                }
            }
        }
    }
}

impl eframe::App for MonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                WorkerEvent::Snapshot(mut list) => {
                    for monitor in &mut list {
                        let saved_id = monitor.id.clone();
                        let (brightness_supported, contrast_supported) =
                            (monitor.brightness_supported, monitor.contrast_supported);
                        let (saved_brightness, saved_contrast) = {
                            let saved = self.config.monitors.entry(saved_id).or_default();
                            saved.brightness_supported = Some(brightness_supported);
                            saved.contrast_supported = Some(contrast_supported);
                            (saved.brightness, saved.contrast)
                        };
                        if let Some(value) = saved_brightness {
                            monitor.brightness = value;
                            if monitor.brightness_supported {
                                self.send_value(&monitor.id, 0x10, value);
                            }
                        }
                        if let Some(value) = saved_contrast {
                            monitor.contrast = value;
                            if monitor.contrast_supported {
                                self.send_value(&monitor.id, 0x12, value);
                            }
                        }
                    }
                    self.monitors = list
                }
                WorkerEvent::Error(message) => self.monitors.push(MonitorSnapshot::error(message)),
            }
        }
        if self.auto_detect && self.last_refresh.elapsed() > Duration::from_secs(3) {
            self.refresh();
        }
        self.capture_window(ctx);
        ctx.request_repaint_after(Duration::from_millis(100));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| {
                self.header(ctx, ui);
                ui.add_space(12.0);
                self.content(ctx, ui);
                ui.add_space(12.0);
                self.footer(ui);
            });
    }
}

impl MonitorApp {
    fn header(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let response = ui
            .horizontal(|ui| {
                ui.label(RichText::new("▣").size(25.0).color(ACCENT));
                ui.vertical(|ui| {
                    ui.label(RichText::new(TITLE).size(18.0).strong());
                    ui.label(
                        RichText::new(format!(
                            "{} display{} connected",
                            self.monitors.len(),
                            if self.monitors.len() == 1 { "" } else { "s" }
                        ))
                        .small()
                        .color(Color32::from_rgb(155, 166, 184)),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("×").on_hover_text("Close").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("—").on_hover_text("Minimize").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    if ui.button("⚙").on_hover_text("Settings").clicked() {
                        self.settings_open = !self.settings_open;
                    }
                    if ui.button("↻").on_hover_text("Refresh displays").clicked() {
                        self.refresh();
                    }
                });
            })
            .response;
        if response.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if self.settings_open {
            egui::Window::new("Settings")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 58.0))
                .show(ctx, |ui| {
                    let changed = ui
                        .checkbox(&mut self.always_on_top, "Always on top")
                        .changed();
                    if ui
                        .checkbox(&mut self.auto_detect, "Auto-detect displays")
                        .changed()
                    {
                        self.save_config();
                    }
                    ui.label(
                        RichText::new("Names are saved per physical display ID.")
                            .small()
                            .color(Color32::GRAY),
                    );
                    if changed {
                        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                            if self.always_on_top {
                                egui::WindowLevel::AlwaysOnTop
                            } else {
                                egui::WindowLevel::Normal
                            },
                        ));
                    }
                });
        }
        if let Some(error) = &self.config_error {
            ui.label(
                RichText::new(error)
                    .small()
                    .color(Color32::from_rgb(240, 125, 110)),
            );
        }
    }

    fn content(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if self.monitors.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(RichText::new("Detecting displays…").size(18.0));
                ui.label("Make sure DDC/CI is enabled in the monitor menu.");
            });
            return;
        }
        let mut actions: Vec<(String, u8, u16)> = Vec::new();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Quick brightness")
                    .small()
                    .color(Color32::from_rgb(155, 166, 184)),
            );
            for value in [10, 20, 30, 40, 50, 60, 70, 80, 90] {
                let button = egui::Button::new(RichText::new(format!("{}%", value)).small())
                    .fill(Color32::from_rgb(35, 44, 58))
                    .corner_radius(egui::CornerRadius::same(12));
                if ui.add(button).clicked() {
                    for monitor in &mut self.monitors {
                        if monitor.brightness_supported {
                            monitor.brightness = value;
                            actions.push((monitor.id.clone(), 0x10, value));
                        }
                    }
                }
            }
            let apply = egui::Button::new(RichText::new("Apply to all").small().strong())
                .fill(Color32::from_rgb(47, 83, 127))
                .corner_radius(egui::CornerRadius::same(12));
            if ui.add(apply).clicked() {
                if let Some(value) = self
                    .monitors
                    .iter()
                    .find(|m| m.brightness_supported)
                    .map(|m| m.brightness)
                {
                    for monitor in &mut self.monitors {
                        if monitor.brightness_supported {
                            monitor.brightness = value;
                            actions.push((monitor.id.clone(), 0x10, value));
                        }
                    }
                }
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Quick contrast")
                    .small()
                    .color(Color32::from_rgb(155, 166, 184)),
            );
            for value in [10, 20, 30, 40, 50, 60, 70, 80, 90] {
                let button = egui::Button::new(RichText::new(format!("{}%", value)).small())
                    .fill(Color32::from_rgb(35, 44, 58))
                    .corner_radius(egui::CornerRadius::same(12));
                if ui.add(button).clicked() {
                    for monitor in &mut self.monitors {
                        if monitor.contrast_supported {
                            monitor.contrast = value;
                            actions.push((monitor.id.clone(), 0x12, value));
                        }
                    }
                }
            }
            let apply = egui::Button::new(RichText::new("Apply to all").small().strong())
                .fill(Color32::from_rgb(74, 61, 116))
                .corner_radius(egui::CornerRadius::same(12));
            if ui.add(apply).clicked() {
                if let Some(value) = self
                    .monitors
                    .iter()
                    .find(|m| m.contrast_supported)
                    .map(|m| m.contrast)
                {
                    for monitor in &mut self.monitors {
                        if monitor.contrast_supported {
                            monitor.contrast = value;
                            actions.push((monitor.id.clone(), 0x12, value));
                        }
                    }
                }
            }
        });
        ui.add_space(10.0);
        for index in 0..self.monitors.len() {
            let monitor = &mut self.monitors[index];
            let id = monitor.id.clone();
            let name = self
                .config
                .monitors
                .get(&id)
                .and_then(|m| m.custom_name.clone())
                .unwrap_or_else(|| format!("Display {}", index + 1));
            egui::Frame::new()
                .fill(CARD)
                .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(42, 51, 65)))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("▣").size(22.0).color(ACCENT));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(name).size(16.0).strong());
                            ui.label(
                                RichText::new(&monitor.name)
                                    .small()
                                    .color(Color32::from_rgb(165, 175, 190)),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(if monitor.supported {
                                    "● Connected"
                                } else {
                                    "● Unavailable"
                                })
                                .small()
                                .color(if monitor.supported {
                                    Color32::from_rgb(100, 210, 145)
                                } else {
                                    Color32::from_rgb(240, 155, 90)
                                }),
                            );
                        });
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{}%", monitor.brightness))
                                .size(30.0)
                                .strong(),
                        );
                        ui.label(RichText::new("CURRENT LEVEL").small().color(Color32::GRAY));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Edit name").clicked() {
                                self.editing = Some(id.clone());
                                self.draft_name = self
                                    .config
                                    .monitors
                                    .get(&id)
                                    .and_then(|m| m.custom_name.clone())
                                    .unwrap_or_default();
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("☀")
                                .size(17.0)
                                .color(Color32::from_rgb(255, 205, 104)),
                        );
                    });
                    ui.add_space(8.0);
                    MonitorControlRow {
                        monitor_id: &id,
                        control: MonitorControlType::Brightness,
                        enabled: monitor.brightness_supported,
                    }
                    .show(
                        ui,
                        &mut monitor.brightness,
                        &mut |monitor_id, control, value| {
                            actions.push((
                                monitor_id.to_owned(),
                                if control == MonitorControlType::Brightness {
                                    0x10
                                } else {
                                    0x12
                                },
                                value,
                            ));
                        },
                    );
                    ui.add_space(4.0);
                    MonitorControlRow {
                        monitor_id: &id,
                        control: MonitorControlType::Contrast,
                        enabled: monitor.contrast_supported,
                    }
                    .show(
                        ui,
                        &mut monitor.contrast,
                        &mut |monitor_id, control, value| {
                            actions.push((
                                monitor_id.to_owned(),
                                if control == MonitorControlType::Brightness {
                                    0x10
                                } else {
                                    0x12
                                },
                                value,
                            ));
                        },
                    );
                    if !monitor.contrast_supported {
                        ui.label(
                            RichText::new("Contrast: Not supported")
                                .small()
                                .color(Color32::GRAY),
                        );
                    }
                    if let Some(error) = &monitor.error {
                        ui.label(
                            RichText::new(error)
                                .small()
                                .color(Color32::from_rgb(240, 125, 110)),
                        );
                    }
                });
            ui.add_space(9.0);
        }
        let changed_values = !actions.is_empty();
        for (id, code, value) in actions {
            if id == "*" {
                for monitor in &self.monitors {
                    if (code == 0x10 && monitor.brightness_supported)
                        || (code == 0x12 && monitor.contrast_supported)
                    {
                        self.send_value(&monitor.id, code, value);
                        let entry = self.config.monitors.entry(monitor.id.clone()).or_default();
                        if code == 0x10 {
                            entry.brightness = Some(value);
                        } else {
                            entry.contrast = Some(value);
                        }
                    }
                }
            } else {
                self.send_value(&id, code, value);
            }
            if id != "*" {
                let entry = self.config.monitors.entry(id).or_default();
                if code == 0x10 {
                    entry.brightness = Some(value);
                } else {
                    entry.contrast = Some(value);
                }
            }
        }
        if changed_values {
            self.save_config();
        }
        if let Some(id) = self.editing.clone() {
            egui::Window::new("Rename display")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Display name");
                    let response = ui.text_edit_singleline(&mut self.draft_name);
                    response.request_focus();
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.set_name(id.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            self.editing = None;
                        }
                    });
                });
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(if self.auto_detect {
                    "● Auto-detecting"
                } else {
                    "○ Auto-detect off"
                })
                .small()
                .color(if self.auto_detect {
                    Color32::from_rgb(100, 210, 145)
                } else {
                    Color32::GRAY
                }),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(VERSION).small().color(Color32::GRAY));
            });
        });
    }
}

fn main() -> eframe::Result {
    let (startup_config, _) = load_config();
    let window = startup_config.window;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(Vec2::new(
                window.width.clamp(360.0, 1600.0),
                window.height.clamp(430.0, 1600.0),
            ))
            .with_position(egui::pos2(
                window.x.clamp(-3000.0, 10000.0),
                window.y.clamp(-3000.0, 10000.0),
            ))
            .with_maximized(window.maximized)
            .with_min_inner_size(Vec2::new(360.0, 430.0))
            .with_icon(load_egui_icon()),
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        options,
        Box::new(|cc| Ok(Box::new(MonitorApp::new(cc)))),
    )
}
