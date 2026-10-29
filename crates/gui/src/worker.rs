//! Device I/O thread. Owns the HID handle; the UI talks to it over channels so
//! it never blocks. Polls enumeration for hot-plug and the active profile so
//! changes made with the mouse's own buttons show up.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use hidpp::backup::{self, Backup};
use hidpp::device::{self, LedZone};
use hidpp::memory::{self, OnboardMemory};
use hidpp::onboard::{MODE_ONBOARD, ROM_BASE};
use hidpp::{Device, Error, HidApi};

pub enum Cmd {
    Reload,
    Apply {
        base: Box<OnboardMemory>,
        edited: Box<OnboardMemory>,
        active: u16,
    },
    Activate(u16),
    SetOnboardMode,
    Restore(PathBuf),
    FactoryReset,
    BackupNow,
}

pub struct Loaded {
    pub name: String,
    pub id: String,
    pub firmware: Vec<String>,
    pub memory: OnboardMemory,
    pub dpi_values: Vec<u16>,
    pub rates_ms: Vec<u8>,
    pub led_zones: Vec<LedZone>,
    pub active_sector: u16,
    pub mode: u8,
}

pub enum Ev {
    Searching,
    Connected(Box<Loaded>),
    Disconnected(String),
    Active { sector: u16, mode: u8 },
    Busy(String),
    Done(String),
    Error(String),
}

pub struct Worker {
    pub tx: Sender<Cmd>,
    pub rx: Receiver<Ev>,
}

impl Worker {
    pub fn spawn(ctx: egui::Context) -> Worker {
        let (tx, cmd_rx) = mpsc::channel();
        let (ev_tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("device".into())
            .spawn(move || run(ctx, cmd_rx, ev_tx))
            .expect("spawn device thread");
        Worker { tx, rx }
    }
}

struct Ctx {
    ui: egui::Context,
    tx: Sender<Ev>,
}

impl Ctx {
    fn send(&self, ev: Ev) {
        let _ = self.tx.send(ev);
        self.ui.request_repaint();
    }
}

const POLL: Duration = Duration::from_millis(800);

fn run(ui: egui::Context, rx: Receiver<Cmd>, tx: Sender<Ev>) {
    let c = Ctx { ui, tx };
    let mut api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            c.send(Ev::Error(format!("HID subsystem unavailable: {e}")));
            return;
        }
    };
    let mut backed_up: HashSet<String> = HashSet::new();
    let mut dev: Option<Device> = None;
    let mut last = (0u16, 0u8);
    c.send(Ev::Searching);

    loop {
        let Some(d) = dev.as_mut() else {
            // Not connected: drop queued commands, look for the mouse.
            while rx.try_recv().is_ok() {}
            let _ = api.refresh_devices();
            if let Some(cand) = device::find_mice(&api).into_iter().next() {
                match Device::open(&api, &cand).and_then(|mut d| {
                    let first = backed_up.insert(cand.key());
                    let loaded = load(&mut d, first)?;
                    Ok((d, loaded))
                }) {
                    Ok((d, loaded)) => {
                        last = (loaded.active_sector, loaded.mode);
                        dev = Some(d);
                        c.send(Ev::Connected(Box::new(loaded)));
                    }
                    Err(e) => c.send(Ev::Disconnected(format!(
                        "found the mouse but could not read it: {e}"
                    ))),
                }
            }
            match rx.recv_timeout(Duration::from_millis(1000)) {
                Err(RecvTimeoutError::Disconnected) => return,
                _ => continue,
            }
        };

        let result = match rx.recv_timeout(POLL) {
            Ok(cmd) => handle(&c, d, cmd),
            Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => poll(&c, &mut api, d, &mut last),
        };
        match result {
            Ok(()) => {}
            Err(Error::Disconnected | Error::Hid(_) | Error::NotFound) => {
                dev = None;
                c.send(Ev::Disconnected("mouse disconnected".into()));
                c.send(Ev::Searching);
            }
            Err(e) => c.send(Ev::Error(e.to_string())),
        }
    }
}

fn poll(c: &Ctx, api: &mut HidApi, d: &mut Device, last: &mut (u16, u8)) -> hidpp::Result<()> {
    let _ = api.refresh_devices();
    let key = d.candidate.key();
    if !device::find_mice(api).iter().any(|m| m.key() == key) {
        return Err(Error::NotFound);
    }
    let now = (d.current_profile_sector()?, d.onboard_mode()?);
    if now != *last {
        *last = now;
        c.send(Ev::Active {
            sector: now.0,
            mode: now.1,
        });
    }
    Ok(())
}

fn load(d: &mut Device, backup_first: bool) -> hidpp::Result<Loaded> {
    let b = Backup::read_from(d)?;
    if backup_first {
        b.save(&backup::default_backup_dir(), "connect")?;
    }
    let memory = OnboardMemory::from_backup(&b)?;
    Ok(Loaded {
        name: b.device.name.clone(),
        id: d.candidate.id_string(),
        firmware: b.device.firmware.clone(),
        dpi_values: d.dpi_list().unwrap_or_default(),
        rates_ms: d.report_rates_ms().unwrap_or_else(|_| vec![1, 2, 4, 8]),
        led_zones: d.led_zones().unwrap_or_default(),
        active_sector: b.current_profile_sector.unwrap_or(1),
        mode: b.onboard_mode.unwrap_or(0),
        memory,
    })
}

fn reload(c: &Ctx, d: &mut Device) -> hidpp::Result<()> {
    let loaded = load(d, false)?;
    c.send(Ev::Connected(Box::new(loaded)));
    Ok(())
}

fn handle(c: &Ctx, d: &mut Device, cmd: Cmd) -> hidpp::Result<()> {
    match cmd {
        Cmd::Reload => {
            c.send(Ev::Busy("Reading the mouse…".into()));
            reload(c, d)?;
            c.send(Ev::Done("Reloaded from the mouse".into()));
        }
        Cmd::BackupNow => {
            c.send(Ev::Busy("Backing up…".into()));
            let p = Backup::read_from(d)?.save(&backup::default_backup_dir(), "manual")?;
            c.send(Ev::Done(format!("Backup saved: {}", p.path().display())));
        }
        Cmd::Activate(sector) => {
            memory::finish(d, sector)?;
            c.send(Ev::Active {
                sector: d.current_profile_sector()?,
                mode: d.onboard_mode()?,
            });
        }
        Cmd::SetOnboardMode => {
            d.set_onboard_mode(MODE_ONBOARD)?;
            c.send(Ev::Active {
                sector: d.current_profile_sector()?,
                mode: d.onboard_mode()?,
            });
        }
        Cmd::Apply {
            base,
            edited,
            active,
        } => {
            c.send(Ev::Busy("Checking the mouse before writing…".into()));
            let current = Backup::read_from(d)?;
            let fresh = OnboardMemory::from_backup(&current)?;
            if user_sectors(&fresh) != user_sectors(&base) {
                reload(c, d)?;
                return Err(Error::Invalid(
                    "the mouse memory changed since it was loaded; reloaded it, nothing was written".into(),
                ));
            }
            let plan = fresh.plan(&edited)?;
            write_plan(c, d, &current, &plan, active, "pre-write")?;
            let after = OnboardMemory::from_backup(&Backup::read_from(d)?)?;
            let same = after
                .profiles
                .iter()
                .zip(&edited.profiles)
                .all(|(a, b)| a.profile == b.profile && a.enabled == b.enabled);
            reload(c, d)?;
            if !same {
                return Err(Error::Invalid(
                    "read-back after writing does not match the edit".into(),
                ));
            }
            c.send(Ev::Done(format!(
                "Wrote {} sector(s), read back and verified",
                plan.len()
            )));
        }
        Cmd::Restore(path) => {
            let wanted = Backup::load(&path)?;
            c.send(Ev::Busy("Restoring…".into()));
            let current = Backup::read_from(d)?;
            let plan = memory::restore_plan(&wanted, &current)?;
            let active = current
                .current_profile_sector
                .filter(|s| *s < ROM_BASE)
                .unwrap_or(1);
            write_plan(c, d, &current, &plan, active, "pre-restore")?;
            reload(c, d)?;
            c.send(Ev::Done(format!(
                "Restored {} sector(s) from {}",
                plan.len(),
                path.display()
            )));
        }
        Cmd::FactoryReset => {
            c.send(Ev::Busy("Factory reset…".into()));
            let current = Backup::read_from(d)?;
            let mem = OnboardMemory::from_backup(&current)?;
            let reset = mem.factory_reset();
            let plan = mem.plan(&reset)?;
            let active = reset
                .profiles
                .iter()
                .find(|p| p.enabled)
                .map(|p| p.sector)
                .unwrap_or(1);
            write_plan(c, d, &current, &plan, active, "pre-factory-reset")?;
            reload(c, d)?;
            c.send(Ev::Done(format!(
                "Factory profiles restored ({} sector(s) written)",
                plan.len()
            )));
        }
    }
    Ok(())
}

/// Back up `current` to disk, then write and verify every planned sector.
fn write_plan(
    c: &Ctx,
    d: &mut Device,
    current: &Backup,
    plan: &[memory::SectorWrite],
    active: u16,
    label: &str,
) -> hidpp::Result<()> {
    let proof = current.save(&backup::default_backup_dir(), label)?;
    for (i, w) in plan.iter().enumerate() {
        c.send(Ev::Busy(format!(
            "Writing sector {:#06x} ({}/{})…",
            w.sector,
            i + 1,
            plan.len()
        )));
        d.write_sector(&proof, &current.info, w.sector, &w.after)?;
    }
    memory::finish(d, active)?;
    Ok(())
}

fn user_sectors(m: &OnboardMemory) -> Vec<(u16, Vec<u8>)> {
    let n = u16::from(m.info.profile_count);
    m.source_sectors
        .iter()
        .filter(|(s, _)| *s <= n)
        .cloned()
        .collect()
}
