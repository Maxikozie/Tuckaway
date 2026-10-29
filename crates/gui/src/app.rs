//! Main window.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Ui};
use hidpp::backup;
use hidpp::device::LedZone;
use hidpp::export::ProfileExport;
use hidpp::keys;
use hidpp::layout;
use hidpp::memory::{OnboardMemory, Source};
use hidpp::onboard::{MODE_HOST, MODE_ONBOARD, ROM_BASE};
use hidpp::profile::{Binding, KeyCombo, Led, Profile, Rgb, Special, DPI_SLOTS};

use crate::keycapture::{self, Capture};
use crate::schematic;
use crate::theme;
use crate::worker::{Cmd, Ev, Loaded, Worker};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Buttons,
    Sensor,
    Lighting,
}

enum Conn {
    Searching,
    Connected,
    Lost(String),
}

struct DeviceView {
    name: String,
    id: String,
    firmware: Vec<String>,
    dpi_values: Vec<u16>,
    rates_ms: Vec<u8>,
    led_zones: Vec<LedZone>,
}

enum Confirm {
    Apply {
        lines: Vec<String>,
        sectors: Vec<u16>,
    },
    FactoryReset {
        lines: Vec<String>,
    },
    Restore(PathBuf),
}

struct Recording {
    capture: Capture,
    since: Instant,
    profile: usize,
    slot: usize,
    gshift: bool,
}

pub struct App {
    worker: Worker,
    conn: Conn,
    dev: Option<DeviceView>,
    original: Option<OnboardMemory>,
    edited: Option<OnboardMemory>,
    active_sector: u16,
    mode: u8,
    sel_profile: usize,
    sel_slot: Option<usize>,
    gshift_layer: bool,
    tab: Tab,
    busy: Option<String>,
    status: Option<(Color32, String)>,
    confirm: Option<Confirm>,
    recording: Option<Recording>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_worker(cc, Worker::spawn(cc.egui_ctx.clone()))
    }

    pub fn with_worker(cc: &eframe::CreationContext<'_>, worker: Worker) -> Self {
        theme::apply(&cc.egui_ctx);
        App {
            worker,
            conn: Conn::Searching,
            dev: None,
            original: None,
            edited: None,
            active_sector: 0,
            mode: 0,
            sel_profile: 0,
            sel_slot: Some(6),
            gshift_layer: false,
            tab: Tab::Buttons,
            busy: None,
            status: None,
            confirm: None,
            recording: None,
        }
    }

    fn send(&mut self, cmd: Cmd) {
        let _ = self.worker.tx.send(cmd);
    }

    fn dirty(&self) -> bool {
        matches!((&self.original, &self.edited), (Some(o), Some(e)) if o.profiles != e.profiles)
    }

    fn info(&mut self, s: impl Into<String>) {
        self.status = Some((theme::TEXT_DIM, s.into()));
    }

    fn error(&mut self, s: impl Into<String>) {
        self.status = Some((theme::ERR, s.into()));
    }

    fn on_loaded(&mut self, l: Loaded) {
        let keep_edits = matches!(&self.original, Some(o) if o.source_sectors == l.memory.source_sectors)
            && self.edited.is_some();
        if !keep_edits {
            self.edited = Some(l.memory.clone());
        }
        self.original = Some(l.memory);
        self.active_sector = l.active_sector;
        self.mode = l.mode;
        if self.dev.is_none() {
            if let Some(i) = self.profile_index_for_sector(l.active_sector) {
                self.sel_profile = i;
            }
        }
        self.dev = Some(DeviceView {
            name: l.name,
            id: l.id,
            firmware: l.firmware,
            dpi_values: l.dpi_values,
            rates_ms: l.rates_ms,
            led_zones: l.led_zones,
        });
        self.conn = Conn::Connected;
    }

    fn profile_index_for_sector(&self, sector: u16) -> Option<usize> {
        self.original
            .as_ref()?
            .profiles
            .iter()
            .position(|p| p.sector == sector)
    }

    fn drain_events(&mut self) {
        while let Ok(ev) = self.worker.rx.try_recv() {
            match ev {
                Ev::Searching => {
                    if !matches!(self.conn, Conn::Lost(_)) {
                        self.conn = Conn::Searching;
                    }
                }
                Ev::Connected(l) => self.on_loaded(*l),
                Ev::Disconnected(why) => {
                    self.conn = Conn::Lost(why);
                    self.busy = None;
                    self.stop_recording();
                }
                Ev::Active { sector, mode } => {
                    self.active_sector = sector;
                    self.mode = mode;
                }
                Ev::Busy(s) => self.busy = Some(s),
                Ev::Done(s) => {
                    self.busy = None;
                    self.status = Some((theme::OK, s));
                }
                Ev::Error(s) => {
                    self.busy = None;
                    self.error(s);
                }
            }
        }
    }

    fn stop_recording(&mut self) {
        if self.recording.take().is_some() {
            keycapture::stop();
        }
    }

    fn poll_recording(&mut self) {
        let Some(rec) = self.recording.as_mut() else {
            return;
        };
        let events = match keycapture::poll() {
            Some(e) => e,
            None => {
                self.recording = None;
                self.info("Recording stopped: window lost focus");
                return;
            }
        };
        let mut done = None;
        for (scan, pressed) in events {
            if let Some(k) = rec.capture.feed(scan, pressed) {
                done = Some(k);
                break;
            }
        }
        if let Some(k) = done {
            let (p, s, g) = (rec.profile, rec.slot, rec.gshift);
            self.stop_recording();
            if let Some(b) = self.binding_mut(p, s, g) {
                *b = Binding::Key(k);
            }
        } else if rec.since.elapsed() > CAPTURE_TIMEOUT {
            self.stop_recording();
            self.info("Recording timed out");
        }
    }

    fn binding_mut(&mut self, profile: usize, slot: usize, gshift: bool) -> Option<&mut Binding> {
        let p = &mut self.edited.as_mut()?.profiles.get_mut(profile)?.profile;
        if gshift {
            p.gshift_buttons.get_mut(slot)
        } else {
            p.buttons.get_mut(slot)
        }
    }

    fn active_for_apply(&self, edited: &OnboardMemory) -> u16 {
        edited
            .profiles
            .iter()
            .find(|p| p.sector == self.active_sector && p.enabled)
            .or_else(|| edited.profiles.iter().find(|p| p.enabled))
            .map(|p| p.sector)
            .unwrap_or(1)
    }

    fn request_apply(&mut self) {
        let (Some(o), Some(e)) = (&self.original, &self.edited) else {
            return;
        };
        match o.plan(e) {
            Ok(plan) if plan.is_empty() => self.info("Nothing to write"),
            Ok(plan) => {
                self.confirm = Some(Confirm::Apply {
                    lines: o.diff(e),
                    sectors: plan.iter().map(|w| w.sector).collect(),
                });
            }
            Err(err) => self.error(format!("Cannot apply: {err}")),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.drain_events();
        self.poll_recording();
        let ctx = ui.ctx().clone();

        egui::Panel::top("top")
            .frame(egui::Frame::new().fill(theme::SURFACE_1).inner_margin(10))
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(theme::SURFACE_1)
                    .inner_margin(egui::Margin::symmetric(10, 5)),
            )
            .show(ui, |ui| self.status_bar(ui));

        if matches!(self.conn, Conn::Connected) && self.edited.is_some() {
            egui::Panel::left("profiles")
                .resizable(false)
                .exact_size(210.0)
                .frame(egui::Frame::new().fill(theme::BG).inner_margin(10))
                .show(ui, |ui| self.profiles_panel(ui));
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::BG).inner_margin(12))
                .show(ui, |ui| self.main_area(ui));
        } else {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::BG))
                .show(ui, |ui| self.empty_state(ui));
        }

        self.confirm_dialog(&ctx);
        if self.busy.is_some() || self.recording.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        keycapture::stop();
    }
}

// ---------------------------------------------------------------------------
// Chrome
// ---------------------------------------------------------------------------

impl App {
    fn top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("MouseForge").strong().size(18.0));
            ui.add_space(12.0);
            let (dot, text) = match (&self.conn, &self.dev) {
                (Conn::Connected, Some(d)) => (theme::OK, format!("{}  ·  {}", d.name, d.id)),
                (Conn::Lost(why), _) => (theme::ERR, format!("Not connected: {why}")),
                _ => (theme::WARN, "Looking for the mouse…".to_string()),
            };
            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 4.5, dot);
            let r = ui.label(RichText::new(text).color(theme::TEXT_DIM));
            if let Some(d) = &self.dev {
                r.on_hover_text(d.firmware.join("\n"));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let connected = matches!(self.conn, Conn::Connected) && self.busy.is_none();
                ui.add_enabled_ui(connected, |ui| {
                    ui.menu_button("Backup & restore", |ui| self.backup_menu(ui));
                    let dirty = self.dirty();
                    if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
                        self.edited = self.original.clone();
                    }
                    let apply = egui::Button::new(RichText::new("Apply to mouse").strong().color(
                        if dirty {
                            Color32::WHITE
                        } else {
                            theme::TEXT_DIM
                        },
                    ))
                    .fill(if dirty {
                        theme::ACCENT
                    } else {
                        theme::SURFACE_2
                    });
                    if ui.add_enabled(dirty, apply).clicked() {
                        self.request_apply();
                    }
                    if dirty {
                        let n = self
                            .original
                            .as_ref()
                            .zip(self.edited.as_ref())
                            .map(|(o, e)| o.diff(e).len())
                            .unwrap_or(0);
                        ui.label(
                            RichText::new(format!(
                                "{n} unsaved change{}",
                                if n == 1 { "" } else { "s" }
                            ))
                            .color(theme::WARN),
                        );
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, theme::WARN);
                    }
                });
            });
        });
    }

    fn backup_menu(&mut self, ui: &mut Ui) {
        if ui.button("Back up now").clicked() {
            self.send(Cmd::BackupNow);
            ui.close();
        }
        if ui.button("Restore from backup…").clicked() {
            ui.close();
            if let Some(p) = rfd::FileDialog::new()
                .set_directory(backup::default_backup_dir())
                .add_filter("Backup", &["json"])
                .pick_file()
            {
                self.confirm = Some(Confirm::Restore(p));
            }
        }
        if ui.button("Open backups folder").clicked() {
            ui.close();
            let dir = backup::default_backup_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::process::Command::new("explorer").arg(dir).spawn();
        }
        ui.separator();
        let n = self.sel_profile + 1;
        if ui.button(format!("Export profile {n} as JSON…")).clicked() {
            ui.close();
            self.export_profile();
        }
        if ui
            .button(format!("Import JSON into profile {n}…"))
            .clicked()
        {
            ui.close();
            self.import_profile();
        }
        ui.separator();
        if ui.button("Factory reset…").clicked() {
            ui.close();
            if let Some(o) = &self.original {
                self.confirm = Some(Confirm::FactoryReset {
                    lines: o.diff(&o.factory_reset()),
                });
            }
        }
        if ui.button("Reload from mouse").clicked() {
            ui.close();
            self.send(Cmd::Reload);
        }
    }

    fn export_profile(&mut self) {
        let Some(e) = &self.edited else { return };
        let p = &e.profiles[self.sel_profile].profile;
        let export = ProfileExport::new(hidpp::device::VENDOR_ID, e.product_id, p);
        let name = format!("profile-{}.json", self.sel_profile + 1);
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Profile JSON", &["json"])
            .set_file_name(name)
            .save_file()
        {
            match export.save(&path) {
                Ok(()) => self.info(format!("Exported to {}", path.display())),
                Err(err) => self.error(format!("Export failed: {err}")),
            }
        }
    }

    fn import_profile(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Profile JSON", &["json"])
            .pick_file()
        else {
            return;
        };
        let sel = self.sel_profile;
        let result = ProfileExport::load(&path).and_then(|imp| {
            let target = &mut self.edited.as_mut().expect("loaded").profiles[sel].profile;
            imp.apply_to(target)
        });
        match result {
            Ok(()) => self.info(format!(
                "Imported {} into profile {} (not written yet)",
                path.display(),
                sel + 1
            )),
            Err(err) => self.error(format!("Import failed: {err}")),
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if let Some(b) = &self.busy {
                ui.spinner();
                ui.label(b);
            } else if let Some((c, s)) = &self.status {
                ui.label(RichText::new(s).color(*c));
            } else {
                ui.label(
                    RichText::new(
                        "Settings are stored on the mouse; nothing needs to keep running.",
                    )
                    .color(theme::TEXT_DIM),
                );
            }
        });
    }

    fn empty_state(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.35);
            match &self.conn {
                Conn::Lost(why) => {
                    ui.label(RichText::new("Mouse not connected").size(20.0));
                    ui.label(RichText::new(why).color(theme::TEXT_DIM));
                }
                _ => {
                    ui.spinner();
                    ui.label(RichText::new("Looking for the mouse…").size(18.0));
                }
            }
            ui.label(
                RichText::new("Plug it in over USB; it is picked up automatically.")
                    .color(theme::TEXT_DIM),
            );
        });
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let mut close = false;
        let mut go = false;
        egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
            ui.set_width(460.0);
            let (title, lines, extra): (&str, Vec<String>, String) = match c {
                Confirm::Apply { lines, sectors } => (
                    "Write these changes to the mouse?",
                    lines.clone(),
                    format!(
                        "Sectors: {}. A backup is saved first; every sector is read back and verified.",
                        sectors.iter().map(|s| format!("{s:#06x}")).collect::<Vec<_>>().join(", ")
                    ),
                ),
                Confirm::FactoryReset { lines } => (
                    "Reset every profile to the factory settings?",
                    if lines.is_empty() { vec!["Already at factory settings (sectors will still be rewritten if needed).".into()] } else { lines.clone() },
                    "Copies the ROM profiles into the user profiles. A backup is saved first.".into(),
                ),
                Confirm::Restore(p) => (
                    "Restore this backup to the mouse?",
                    vec![p.display().to_string()],
                    "Only user sectors with a valid checksum are written. A backup of the current state is saved first.".into(),
                ),
            };
            ui.heading(title);
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for l in &lines {
                    ui.label(format!("• {l}"));
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new(extra).color(theme::TEXT_DIM).small());
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Write").strong().color(Color32::WHITE)).fill(theme::ACCENT)).clicked() {
                    go = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        if go {
            match self.confirm.take() {
                Some(Confirm::Apply { .. }) => {
                    if let (Some(o), Some(e)) = (self.original.clone(), self.edited.clone()) {
                        let active = self.active_for_apply(&e);
                        self.busy = Some("Writing…".into());
                        self.send(Cmd::Apply {
                            base: Box::new(o),
                            edited: Box::new(e),
                            active,
                        });
                    }
                }
                Some(Confirm::FactoryReset { .. }) => {
                    self.busy = Some("Factory reset…".into());
                    self.edited = self.original.clone();
                    self.send(Cmd::FactoryReset);
                }
                Some(Confirm::Restore(p)) => {
                    self.busy = Some("Restoring…".into());
                    self.edited = self.original.clone();
                    self.send(Cmd::Restore(p));
                }
                None => {}
            }
        } else if close {
            self.confirm = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

impl App {
    fn profiles_panel(&mut self, ui: &mut Ui) {
        ui.label(RichText::new("PROFILES").small().color(theme::TEXT_DIM));
        ui.add_space(4.0);
        if self.mode == MODE_HOST {
            egui::Frame::new()
                .fill(theme::WARN.gamma_multiply(0.2))
                .corner_radius(6)
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Host mode: onboard profiles are not in use.")
                            .color(theme::WARN),
                    );
                    if ui.button("Switch to onboard mode").clicked() {
                        self.send(Cmd::SetOnboardMode);
                    }
                });
            ui.add_space(6.0);
        }
        let active = self.active_sector;
        let orig_enabled: Vec<bool> = self
            .original
            .as_ref()
            .map(|o| o.profiles.iter().map(|p| p.enabled).collect())
            .unwrap_or_default();
        let Some(e) = self.edited.as_mut() else {
            return;
        };
        let mut activate = None;
        for (i, slot) in e.profiles.iter_mut().enumerate() {
            let selected = self.sel_profile == i;
            let fill = if selected {
                theme::SURFACE_3
            } else {
                theme::SURFACE_1
            };
            // The card's own click sense is registered before its children,
            // so the checkbox and button inside keep their clicks.
            let card = ui.scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
                egui::Frame::new()
                    .fill(fill)
                    .corner_radius(8)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let mut title = format!("Profile {}", i + 1);
                            if !slot.profile.name.is_empty() {
                                title.push_str(&format!(" · {}", slot.profile.name));
                            }
                            let r = ui.add(
                                egui::Label::new(RichText::new(title).strong())
                                    .sense(egui::Sense::click()),
                            );
                            if r.clicked() {
                                self.sel_profile = i;
                            }
                            if slot.sector == active && self.mode == MODE_ONBOARD {
                                ui.label(RichText::new("ACTIVE").small().color(theme::OK));
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut slot.enabled, "Enabled");
                            let can_activate = slot.sector != active
                                && orig_enabled.get(i).copied().unwrap_or(false);
                            if can_activate
                                && ui
                                    .small_button("Make active")
                                    .on_hover_text("Switches the mouse to this profile now")
                                    .clicked()
                            {
                                activate = Some(slot.sector);
                            }
                        });
                        if let Source::Rom(s) = slot.source {
                            ui.label(
                                RichText::new(format!("showing factory data (ROM {s:#06x})"))
                                    .small()
                                    .color(theme::WARN),
                            );
                        }
                        let p = &slot.profile;
                        ui.label(
                            RichText::new(format!(
                                "{} DPI · {} Hz",
                                p.dpi
                                    .get(usize::from(p.default_dpi_index))
                                    .copied()
                                    .unwrap_or(0),
                                p.report_rate_hz()
                            ))
                            .small()
                            .color(theme::TEXT_DIM),
                        );
                    });
            });
            if card.response.clicked() {
                self.sel_profile = i;
            }
            ui.add_space(4.0);
        }
        if let Some(s) = activate {
            self.send(Cmd::Activate(s));
        }
        if active >= ROM_BASE {
            ui.label(
                RichText::new(format!("Active: factory profile {}", active - ROM_BASE))
                    .small()
                    .color(theme::TEXT_DIM),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Main area
// ---------------------------------------------------------------------------

impl App {
    fn main_area(&mut self, ui: &mut Ui) {
        let has_leds = self.dev.as_ref().is_some_and(|d| !d.led_zones.is_empty())
            && self
                .edited
                .as_ref()
                .is_some_and(|e| !e.profiles[self.sel_profile].profile.leds.is_empty());
        if !has_leds && self.tab == Tab::Lighting {
            self.tab = Tab::Buttons;
        }
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.tab,
                Tab::Buttons,
                RichText::new("Buttons").size(15.0),
            );
            ui.selectable_value(
                &mut self.tab,
                Tab::Sensor,
                RichText::new("DPI & report rate").size(15.0),
            );
            if has_leds {
                ui.selectable_value(
                    &mut self.tab,
                    Tab::Lighting,
                    RichText::new("Lighting").size(15.0),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("Editing profile {}", self.sel_profile + 1))
                        .color(theme::TEXT_DIM),
                );
            });
        });
        ui.separator();
        ui.add_space(6.0);
        match self.tab {
            Tab::Buttons => self.buttons_tab(ui),
            Tab::Sensor => {
                egui::ScrollArea::vertical()
                    .id_salt("sensor_scroll")
                    .show(ui, |ui| self.sensor_tab(ui));
            }
            Tab::Lighting => {
                egui::ScrollArea::vertical()
                    .id_salt("lighting_scroll")
                    .show(ui, |ui| self.lighting_tab(ui));
            }
        }
    }

    fn buttons_tab(&mut self, ui: &mut Ui) {
        let Some(e) = &self.edited else { return };
        let product = e.product_id;
        let has_gshift = e.info.has_g_shift();
        let sel_profile = self.sel_profile;
        let gshift = self.gshift_layer && has_gshift;
        let list: Vec<Binding> = {
            let p = &e.profiles[sel_profile].profile;
            if gshift {
                p.gshift_buttons.clone()
            } else {
                p.buttons.clone()
            }
        };
        let orig: Vec<Binding> = self
            .original
            .as_ref()
            .map(|o| {
                let p = &o.profiles[sel_profile].profile;
                if gshift {
                    p.gshift_buttons.clone()
                } else {
                    p.buttons.clone()
                }
            })
            .unwrap_or_default();

        ui.horizontal_top(|ui| {
            // Schematic
            ui.vertical(|ui| {
                ui.set_width(340.0);
                let describe = |s: usize| {
                    format!(
                        "{}: {}",
                        layout::slot_label(product, s),
                        list.get(s)
                            .map(keycapture::binding_label)
                            .unwrap_or_default()
                    )
                };
                let max_h = ui.available_height() - if has_gshift { 44.0 } else { 0.0 };
                if let Some(s) = schematic::show(ui, max_h, list.len(), self.sel_slot, &describe) {
                    self.sel_slot = Some(s);
                }
                if has_gshift {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.gshift_layer, false, "Normal");
                        ui.selectable_value(&mut self.gshift_layer, true, "G-shift layer");
                    })
                    .response
                    .on_hover_text(
                        "Bindings used while a button bound to \"G-shift (hold)\" is held",
                    );
                }
            });
            ui.add_space(12.0);

            // Binding list + editor
            egui::ScrollArea::vertical()
                .id_salt("bindings_scroll")
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width(560.0);
                        egui::Frame::new()
                            .fill(theme::SURFACE_1)
                            .corner_radius(8)
                            .inner_margin(8)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                egui::Grid::new("bindings")
                                    .num_columns(2)
                                    .spacing([16.0, 4.0])
                                    .striped(false)
                                    .show(ui, |ui| {
                                        for (i, b) in list.iter().enumerate() {
                                            let changed = orig.get(i) != Some(b);
                                            let label = layout::slot_label(product, i);
                                            let sel = self.sel_slot == Some(i);
                                            if ui
                                                .selectable_label(
                                                    sel,
                                                    RichText::new(label).color(if sel {
                                                        theme::TEXT
                                                    } else {
                                                        theme::TEXT_DIM
                                                    }),
                                                )
                                                .clicked()
                                            {
                                                self.sel_slot = Some(i);
                                            }
                                            let mut t = RichText::new(keycapture::binding_label(b));
                                            if changed {
                                                t = t.color(theme::WARN);
                                            }
                                            if ui
                                                .add(
                                                    egui::Label::new(t).sense(egui::Sense::click()),
                                                )
                                                .clicked()
                                            {
                                                self.sel_slot = Some(i);
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });
                        ui.add_space(10.0);
                        if let Some(slot) = self.sel_slot.filter(|s| *s < list.len()) {
                            self.binding_editor(ui, slot, gshift);
                        }
                    });
                });
        });
    }

    fn binding_editor(&mut self, ui: &mut Ui, slot: usize, gshift: bool) {
        let product = self.edited.as_ref().map(|e| e.product_id).unwrap_or(0);
        let sel_profile = self.sel_profile;
        let recording_here = self
            .recording
            .as_ref()
            .is_some_and(|r| r.profile == sel_profile && r.slot == slot && r.gshift == gshift);
        let mut start_recording = false;
        let mut cancel_recording = false;
        let Some(b) = self.binding_mut(sel_profile, slot, gshift) else {
            return;
        };

        egui::Frame::new()
            .fill(theme::SURFACE_1)
            .corner_radius(8)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let title = format!(
                    "{}{}",
                    layout::slot_label(product, slot),
                    if gshift { " (G-shift)" } else { "" }
                );
                ui.label(RichText::new(title).strong().size(16.0));
                if let Some(s) = layout::slots_for(product).iter().find(|s| s.slot == slot) {
                    ui.label(RichText::new(s.location).small().color(theme::TEXT_DIM));
                }
                ui.add_space(6.0);

                let kind = Kind::of(b);
                let mut new_kind = kind;
                egui::ComboBox::from_label("Action")
                    .selected_text(new_kind.label())
                    .show_ui(ui, |ui| {
                        for k in Kind::ALL {
                            ui.selectable_value(&mut new_kind, k, k.label());
                        }
                    });
                if new_kind != kind {
                    *b = new_kind.default_binding();
                }
                ui.add_space(6.0);

                match b {
                    Binding::MouseButtons(mask) => {
                        let current = if *mask == 0 {
                            1
                        } else {
                            mask.trailing_zeros() as u8 + 1
                        };
                        let mut n = current;
                        egui::ComboBox::from_label("Button")
                            .selected_text(hidpp::profile::mouse_button_name(1 << (n - 1)))
                            .show_ui(ui, |ui| {
                                for i in 1..=8u8 {
                                    ui.selectable_value(
                                        &mut n,
                                        i,
                                        hidpp::profile::mouse_button_name(1 << (i - 1)),
                                    );
                                }
                            });
                        if n != current {
                            *b = Binding::mouse_button(n);
                        }
                    }
                    Binding::Key(k) => {
                        key_editor(
                            ui,
                            k,
                            recording_here,
                            &mut start_recording,
                            &mut cancel_recording,
                        );
                    }
                    Binding::Consumer(u) => {
                        egui::ComboBox::from_label("Media key")
                            .selected_text(keys::consumer_label(*u))
                            .show_ui(ui, |ui| {
                                for (code, name) in keys::CONSUMER_KEYS {
                                    ui.selectable_value(u, *code, *name);
                                }
                            });
                    }
                    Binding::Function { function, .. } => {
                        let choices: &[Special] = match Kind::of_special(*function) {
                            Kind::Dpi => &[
                                Special::DpiUp,
                                Special::DpiDown,
                                Special::DpiCycle,
                                Special::DpiDefault,
                                Special::DpiShift,
                            ],
                            Kind::Profile => &[
                                Special::ProfileNext,
                                Special::ProfilePrevious,
                                Special::ProfileCycle,
                            ],
                            _ => &[
                                Special::GShift,
                                Special::TiltLeft,
                                Special::TiltRight,
                                Special::ScrollUp,
                                Special::ScrollDown,
                            ],
                        };
                        let mut f = *function;
                        egui::ComboBox::from_label("Function")
                            .selected_text(f.label())
                            .show_ui(ui, |ui| {
                                for c in choices {
                                    ui.selectable_value(&mut f, *c, c.label());
                                }
                            });
                        if f != *function {
                            *b = Binding::function(f);
                        }
                    }
                    Binding::Disabled => {
                        ui.label(RichText::new("The button does nothing.").color(theme::TEXT_DIM));
                    }
                    Binding::Unassigned => {
                        ui.label(
                            RichText::new("No binding stored for this slot.")
                                .color(theme::TEXT_DIM),
                        );
                    }
                    Binding::Macro(_) | Binding::Unknown(_) => {
                        ui.label(
                            RichText::new(format!(
                                "{b} is kept as is; pick another action to replace it."
                            ))
                            .color(theme::TEXT_DIM),
                        );
                    }
                }
            });

        if start_recording {
            self.stop_recording();
            if keycapture::start(ui.ctx()) {
                self.recording = Some(Recording {
                    capture: Capture::default(),
                    since: Instant::now(),
                    profile: sel_profile,
                    slot,
                    gshift,
                });
            } else {
                self.error("Could not start key recording");
            }
        }
        if cancel_recording {
            self.stop_recording();
        }
    }

    fn sensor_tab(&mut self, ui: &mut Ui) {
        let (dpi_values, rates) = match &self.dev {
            Some(d) => (d.dpi_values.clone(), d.rates_ms.clone()),
            None => return,
        };
        let has_shift = self.edited.as_ref().is_some_and(|e| e.info.has_dpi_shift());
        let Some(e) = self.edited.as_mut() else {
            return;
        };
        let p = &mut e.profiles[self.sel_profile].profile;
        let (lo, hi) = (
            dpi_values.first().copied().unwrap_or(100),
            dpi_values.last().copied().unwrap_or(25600),
        );
        let step = dpi_values
            .windows(2)
            .map(|w| w[1] - w[0])
            .min()
            .unwrap_or(50)
            .max(1);

        section(ui, "DPI levels", |ui| {
            let mut levels = p.dpi_levels();
            let mut remove = None;
            egui::Grid::new("dpi")
                .num_columns(5)
                .spacing([14.0, 8.0])
                .show(ui, |ui| {
                    ui.label("");
                    ui.label(RichText::new("DPI").color(theme::TEXT_DIM));
                    ui.label("");
                    if has_shift {
                        ui.label("")
                            .on_hover_text("Shift DPI is used while the DPI-shift button is held");
                    } else {
                        ui.label("");
                    }
                    ui.label("");
                    ui.end_row();
                    for (i, d) in levels.iter_mut().enumerate() {
                        ui.label(format!("Level {}", i + 1));
                        let mut v = u32::from(*d);
                        ui.add(
                            egui::DragValue::new(&mut v)
                                .range(lo..=hi)
                                .speed(f64::from(step))
                                .suffix(" DPI"),
                        );
                        *d = snap(v, &dpi_values, lo, hi);
                        ui.radio_value(&mut p.default_dpi_index, i as u8, "default");
                        if has_shift {
                            ui.radio_value(&mut p.shift_dpi_index, i as u8, "shift");
                        } else {
                            ui.label("");
                        }
                        if ui
                            .add_enabled(levels_len_gt1(p), egui::Button::new("Remove").small())
                            .on_hover_text("Remove level")
                            .clicked()
                        {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
            if let Some(i) = remove {
                levels.remove(i);
                let fix = |idx: &mut u8| {
                    if usize::from(*idx) > i || usize::from(*idx) >= levels.len() {
                        *idx = idx.saturating_sub(1);
                    }
                };
                fix(&mut p.default_dpi_index);
                fix(&mut p.shift_dpi_index);
            }
            if levels.len() < DPI_SLOTS && ui.button("+ Add level").clicked() {
                let next = levels
                    .last()
                    .map(|d| (*d).saturating_add(400).min(hi))
                    .unwrap_or(800);
                levels.push(next);
            }
            let mut arr = [0u16; DPI_SLOTS];
            for (i, d) in levels.iter().take(DPI_SLOTS).enumerate() {
                arr[i] = *d;
            }
            p.dpi = arr;
            ui.label(RichText::new(format!("Sensor range {lo}–{hi} DPI in steps of {step}. DPI up/down buttons move through these levels.")).small().color(theme::TEXT_DIM));
        });

        ui.add_space(10.0);
        section(ui, "Report rate", |ui| {
            let label = |ms: u8| format!("{} Hz", 1000 / u32::from(ms.max(1)));
            egui::ComboBox::new("rate", "polling rate")
                .selected_text(label(p.report_interval_ms))
                .show_ui(ui, |ui| {
                    for ms in &rates {
                        ui.selectable_value(&mut p.report_interval_ms, *ms, label(*ms));
                    }
                });
        });
    }

    fn lighting_tab(&mut self, ui: &mut Ui) {
        let zones = match &self.dev {
            Some(d) => d.led_zones.clone(),
            None => return,
        };
        let Some(e) = self.edited.as_mut() else {
            return;
        };
        let p: &mut Profile = &mut e.profiles[self.sel_profile].profile;
        for z in &zones {
            let Some(led) = p.leds.get_mut(usize::from(z.index)) else {
                continue;
            };
            let title = match z.location {
                1 => "Main zone",
                2 => "Logo",
                _ => "LED zone",
            };
            section(ui, title, |ui| led_editor(ui, led, &z.effects, z.index));
            ui.add_space(10.0);
        }
        ui.label(
            RichText::new("If Windows Dynamic Lighting is on (Settings › Personalisation › Dynamic Lighting), it can override these colours while Windows runs.")
                .small()
                .color(theme::TEXT_DIM),
        );
    }
}

fn levels_len_gt1(p: &Profile) -> bool {
    p.dpi_levels().len() > 1
}

fn snap(v: u32, values: &[u16], lo: u16, hi: u16) -> u16 {
    let v = v.clamp(u32::from(lo), u32::from(hi)) as u16;
    values
        .iter()
        .copied()
        .min_by_key(|x| x.abs_diff(v))
        .unwrap_or(v)
}

fn section(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(theme::SURFACE_1)
        .corner_radius(8)
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_width(ui.available_width().min(560.0));
            ui.label(RichText::new(title).strong().size(15.0));
            ui.add_space(6.0);
            body(ui);
        });
}

// ---------------------------------------------------------------------------
// Editors
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Mouse,
    Key,
    Media,
    Dpi,
    Profile,
    Other,
    Disabled,
    Keep,
}

impl Kind {
    const ALL: [Kind; 7] = [
        Kind::Mouse,
        Kind::Key,
        Kind::Media,
        Kind::Dpi,
        Kind::Profile,
        Kind::Other,
        Kind::Disabled,
    ];

    fn of(b: &Binding) -> Kind {
        match b {
            Binding::MouseButtons(_) => Kind::Mouse,
            Binding::Key(_) => Kind::Key,
            Binding::Consumer(_) => Kind::Media,
            Binding::Function { function, .. } => Kind::of_special(*function),
            Binding::Disabled => Kind::Disabled,
            Binding::Unassigned | Binding::Macro(_) | Binding::Unknown(_) => Kind::Keep,
        }
    }

    fn of_special(s: Special) -> Kind {
        match s {
            Special::DpiUp
            | Special::DpiDown
            | Special::DpiCycle
            | Special::DpiDefault
            | Special::DpiShift => Kind::Dpi,
            Special::ProfileNext | Special::ProfilePrevious | Special::ProfileCycle => {
                Kind::Profile
            }
            _ => Kind::Other,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Mouse => "Mouse button",
            Kind::Key => "Keyboard shortcut",
            Kind::Media => "Media key",
            Kind::Dpi => "DPI",
            Kind::Profile => "Profile switch",
            Kind::Other => "Other function",
            Kind::Disabled => "Disabled",
            Kind::Keep => "(current)",
        }
    }

    fn default_binding(self) -> Binding {
        match self {
            Kind::Mouse => Binding::mouse_button(1),
            Kind::Key => Binding::Key(KeyCombo {
                modifiers: keys::modifier::LCTRL,
                key: 0x06,
            }),
            Kind::Media => Binding::Consumer(0x00CD),
            Kind::Dpi => Binding::function(Special::DpiUp),
            Kind::Profile => Binding::function(Special::ProfileNext),
            Kind::Other => Binding::function(Special::GShift),
            Kind::Disabled | Kind::Keep => Binding::Disabled,
        }
    }
}

fn key_editor(ui: &mut Ui, k: &mut KeyCombo, recording: bool, start: &mut bool, cancel: &mut bool) {
    let shown = if recording {
        "Press the shortcut now…".to_string()
    } else {
        keycapture::combo_label(k)
    };
    let button = egui::Button::new(RichText::new(shown).size(18.0).strong())
        .min_size(egui::vec2(260.0, 40.0))
        .fill(if recording {
            theme::ACCENT.gamma_multiply(0.35)
        } else {
            theme::SURFACE_2
        })
        .stroke(egui::Stroke::new(
            1.0,
            if recording {
                theme::ACCENT
            } else {
                theme::OUTLINE
            },
        ));
    let r = ui.add(button).on_hover_text(if recording {
        "Click to cancel"
    } else {
        "Click, then press the keys to record"
    });
    if r.clicked() {
        if recording {
            *cancel = true;
        } else {
            *start = true;
        }
    }
    ui.label(
        RichText::new(if recording { "Recording: hold modifiers, then press a key. Click to cancel." } else { "Click to record. Keys are stored by physical position; labels follow your keyboard layout." })
            .small()
            .color(theme::TEXT_DIM),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        for (bit, name) in [
            (keys::modifier::LCTRL, "Ctrl"),
            (keys::modifier::LSHIFT, "Shift"),
            (keys::modifier::LALT, "Alt"),
            (keys::modifier::LGUI, "Win"),
        ] {
            let right = bit << 4;
            let mut on = k.modifiers & (bit | right) != 0;
            if ui.checkbox(&mut on, name).changed() {
                k.modifiers &= !(bit | right);
                if on {
                    k.modifiers |= bit;
                }
            }
        }
    });
    egui::ComboBox::from_label("Key")
        .selected_text(if k.key == 0 {
            "(none)".into()
        } else {
            keycapture::key_label(k.key)
        })
        .height(320.0)
        .show_ui(ui, |ui| {
            for (u, us_name) in
                keys::named_keys().filter(|(u, _)| keys::modifier_bit_for_usage(*u).is_none())
            {
                let local = keycapture::key_label(u);
                let text = if local != us_name {
                    format!("{local}   (US {us_name})")
                } else {
                    local
                };
                ui.selectable_value(&mut k.key, u, text);
            }
        });
}

fn led_editor(ui: &mut Ui, led: &mut Led, effects: &[u16], zone: u8) {
    const KNOWN: [(u16, &str); 4] = [
        (0x00, "Off"),
        (0x01, "Static colour"),
        (0x0a, "Breathing"),
        (0x03, "Colour cycle"),
    ];
    let current = match led {
        Led::Off => Some(0x00),
        Led::Static { .. } => Some(0x01),
        Led::Breathing { .. } => Some(0x0a),
        Led::Cycle { .. } => Some(0x03),
        Led::Raw { .. } => None,
    };
    let mut pick = current;
    let label = |id: Option<u16>| {
        KNOWN
            .iter()
            .find(|(k, _)| Some(*k) == id)
            .map(|(_, n)| *n)
            .unwrap_or("Other (kept as is)")
    };
    egui::ComboBox::new(("ledmode", zone), "effect")
        .selected_text(label(pick))
        .show_ui(ui, |ui| {
            for (id, name) in KNOWN.iter().filter(|(id, _)| effects.contains(id)) {
                ui.selectable_value(&mut pick, Some(*id), *name);
            }
        });
    let color_of = |l: &Led| match l {
        Led::Static { color, .. } | Led::Breathing { color, .. } => *color,
        _ => Rgb(0x4f, 0x9d, 0xff),
    };
    if pick != current {
        let c = color_of(led);
        *led = match pick {
            Some(0x00) => Led::Off,
            Some(0x01) => Led::Static { color: c, ramp: 0 },
            Some(0x0a) => Led::Breathing {
                color: c,
                period_ms: 4000,
                waveform: 0,
                brightness: 100,
            },
            Some(0x03) => Led::Cycle {
                period_ms: 8000,
                brightness: 100,
            },
            _ => led.clone(),
        };
    }
    let color_row = |ui: &mut Ui, color: &mut Rgb| {
        ui.horizontal(|ui| {
            let mut c = [color.0, color.1, color.2];
            ui.label("Colour");
            if ui.color_edit_button_srgb(&mut c).changed() {
                *color = Rgb(c[0], c[1], c[2]);
            }
            ui.label(
                RichText::new(color.to_string())
                    .monospace()
                    .color(theme::TEXT_DIM),
            );
        });
    };
    match led {
        Led::Static { color, .. } => color_row(ui, color),
        Led::Breathing {
            color,
            period_ms,
            brightness,
            ..
        } => {
            color_row(ui, color);
            ui.add(egui::Slider::new(period_ms, 1000..=20000).text("period (ms)"));
            ui.add(egui::Slider::new(brightness, 1..=100).text("brightness %"));
        }
        Led::Cycle {
            period_ms,
            brightness,
        } => {
            ui.add(egui::Slider::new(period_ms, 1000..=20000).text("period (ms)"));
            ui.add(egui::Slider::new(brightness, 1..=100).text("brightness %"));
        }
        Led::Off | Led::Raw { .. } => {}
    }
}
