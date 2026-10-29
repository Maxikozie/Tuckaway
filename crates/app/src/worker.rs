//! Device thread. Owns the mouse; the UI talks to it through [`Worker::call`]
//! (one reply per request) and receives [`Ev`] pushes for hot-plug, active
//! profile changes and write progress.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use hidpp::backup::Backup;
use hidpp::device::LedZone;
use hidpp::export::ProfileExport;
use hidpp::memory::{self, OnboardMemory, Source};
use hidpp::onboard::ROM_BASE;
use hidpp::profile::{Led, Profile};
use hidpp::{layout, Error};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::actions::{self, Action};
use crate::keymap;
use crate::mouse::{Connector, Mouse};

// ---------------------------------------------------------------- wire types

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Ev {
    Searching,
    Connected(Box<Snapshot>),
    Disconnected(String),
    Active { sector: u16, mode: u8 },
    Busy(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub name: String,
    pub id: String,
    pub firmware: Vec<String>,
    pub mock: bool,
    pub product_id: u16,
    pub profiles: Vec<ProfileView>,
    pub active_sector: u16,
    pub mode: u8,
    pub dpi_values: Vec<u16>,
    pub rates_ms: Vec<u8>,
    pub led_zones: Vec<LedZone>,
    pub slots: Vec<SlotView>,
    pub has_gshift: bool,
    pub has_dpi_shift: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileView {
    pub index: usize,
    pub sector: u16,
    pub enabled: bool,
    pub from_rom: bool,
    pub profile: Profile,
    pub actions: ActionsView,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionsView {
    pub buttons: Vec<Action>,
    pub gshift: Vec<Action>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlotView {
    pub slot: usize,
    pub label: String,
    pub location: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileEdit {
    pub index: usize,
    pub enabled: bool,
    pub profile: Profile,
}

pub enum Op {
    Reload,
    Apply {
        edits: Vec<ProfileEdit>,
        active: u16,
    },
    Activate(u16),
    LedPreview {
        zone: u8,
        led: Led,
    },
    LedRelease,
    BackupNow,
    Restore(PathBuf),
    FactoryReset,
    Export {
        profile: Box<Profile>,
        path: PathBuf,
    },
    Import {
        index: usize,
        path: PathBuf,
    },
}

pub type Reply = Result<Value, String>;

pub fn actions_for(p: &Profile, l: keymap::Layout) -> ActionsView {
    ActionsView {
        buttons: p.buttons.iter().map(|b| actions::describe(b, l)).collect(),
        gshift: p
            .gshift_buttons
            .iter()
            .map(|b| actions::describe(b, l))
            .collect(),
    }
}

// ---------------------------------------------------------------- worker

#[derive(Clone)]
pub struct Worker {
    tx: Sender<(Op, Sender<Reply>)>,
}

impl Worker {
    pub fn spawn(connector: Box<dyn Connector>, emit: impl Fn(Ev) + Send + 'static) -> Worker {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("device".into())
            .spawn(move || run(connector, rx, Box::new(emit)))
            .expect("spawn device thread");
        Worker { tx }
    }

    /// Blocks until the device thread answers.
    pub fn call(&self, op: Op) -> Reply {
        let (rtx, rrx) = mpsc::channel();
        self.tx
            .send((op, rtx))
            .map_err(|_| "The device thread has stopped.".to_string())?;
        rrx.recv()
            .map_err(|_| "The device thread has stopped.".to_string())?
    }

    /// Like [`call`](Self::call) but gives up after `timeout` (used on exit).
    pub fn call_timeout(&self, op: Op, timeout: Duration) -> Option<Reply> {
        let (rtx, rrx) = mpsc::channel();
        self.tx.send((op, rtx)).ok()?;
        rrx.recv_timeout(timeout).ok()
    }
}

const POLL: Duration = Duration::from_millis(800);

struct Session {
    m: Box<dyn Mouse>,
    loaded: OnboardMemory,
    last: (u16, u8),
    previewing: bool,
}

struct Ctx {
    emit: Box<dyn Fn(Ev) + Send>,
}

fn run(
    mut conn: Box<dyn Connector>,
    rx: Receiver<(Op, Sender<Reply>)>,
    emit: Box<dyn Fn(Ev) + Send>,
) {
    let c = Ctx { emit };
    let mut backed_up: HashSet<String> = HashSet::new();
    let mut s: Option<Session> = None;
    (c.emit)(Ev::Searching);

    loop {
        let Some(sess) = s.as_mut() else {
            // Not connected: refuse queued requests, look for the mouse.
            while let Ok((_, r)) = rx.try_recv() {
                let _ = r.send(Err("The mouse is not connected.".into()));
            }
            if let Some(found) = conn.find() {
                match found.and_then(|mut m| {
                    let first = backed_up.insert(m.key());
                    let (snap, loaded) = load(&mut *m, first)?;
                    Ok((m, snap, loaded))
                }) {
                    Ok((m, snap, loaded)) => {
                        let last = (snap.active_sector, snap.mode);
                        s = Some(Session {
                            m,
                            loaded,
                            last,
                            previewing: false,
                        });
                        (c.emit)(Ev::Connected(snap));
                        continue;
                    }
                    Err(e) => (c.emit)(Ev::Disconnected(format!(
                        "Found the mouse but could not read it: {e}"
                    ))),
                }
            }
            match rx.recv_timeout(Duration::from_millis(1000)) {
                Ok((_, r)) => {
                    let _ = r.send(Err("The mouse is not connected.".into()));
                }
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }
            continue;
        };

        let result = match rx.recv_timeout(POLL) {
            Ok((op, reply)) => {
                let r = handle(&c, sess, op);
                let fatal = matches!(
                    r,
                    Err(Error::Disconnected | Error::Hid(_) | Error::NotFound)
                );
                let _ = reply.send(r.as_ref().map(Clone::clone).map_err(friendly));
                if fatal {
                    r.map(|_| ())
                } else {
                    Ok(())
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => poll(&c, &mut *conn, sess),
        };
        if let Err(Error::Disconnected | Error::Hid(_) | Error::NotFound) = result {
            s = None;
            (c.emit)(Ev::Disconnected("The mouse was disconnected.".into()));
            (c.emit)(Ev::Searching);
        }
    }
}

fn friendly(e: &Error) -> String {
    let s = e.to_string();
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().chain(ch).collect(),
        None => s,
    }
}

fn poll(c: &Ctx, conn: &mut dyn Connector, s: &mut Session) -> hidpp::Result<()> {
    if !conn.present(&s.m.key()) {
        return Err(Error::NotFound);
    }
    let now = s.m.active()?;
    if now != s.last {
        // A profile switch (G9 or another app) hands the LEDs back.
        if now.0 != s.last.0 && s.previewing {
            let _ = s.m.led_release();
            s.previewing = false;
        }
        s.last = now;
        (c.emit)(Ev::Active {
            sector: now.0,
            mode: now.1,
        });
    }
    Ok(())
}

fn load(m: &mut dyn Mouse, backup_first: bool) -> hidpp::Result<(Box<Snapshot>, OnboardMemory)> {
    let b = m.read_backup()?;
    if backup_first {
        m.save_backup(&b, "connect")?;
    }
    let mem = OnboardMemory::from_backup(&b)?;
    let l = keymap::current_layout();
    let pid = b.device.product_id;
    let (active_sector, mode) = m.active()?;
    let snap = Snapshot {
        name: b.device.name.clone(),
        id: m.id_string(),
        firmware: b.device.firmware.clone(),
        mock: m.is_mock(),
        product_id: pid,
        profiles: mem
            .profiles
            .iter()
            .map(|p| ProfileView {
                index: p.index,
                sector: p.sector,
                enabled: p.enabled,
                from_rom: matches!(p.source, Source::Rom(_)),
                profile: p.profile.clone(),
                actions: actions_for(&p.profile, l),
            })
            .collect(),
        active_sector,
        mode,
        dpi_values: m.dpi_list(),
        rates_ms: m.report_rates_ms(),
        led_zones: m.led_zones(),
        slots: layout::slots_for(pid)
            .iter()
            .map(|s| SlotView {
                slot: s.slot,
                label: s.label.into(),
                location: s.location.into(),
            })
            .collect(),
        has_gshift: mem.info.has_g_shift(),
        has_dpi_shift: mem.info.has_dpi_shift(),
    };
    Ok((Box::new(snap), mem))
}

fn reload(c: &Ctx, s: &mut Session) -> hidpp::Result<()> {
    let (snap, mem) = load(&mut *s.m, false)?;
    s.loaded = mem;
    s.last = (snap.active_sector, snap.mode);
    (c.emit)(Ev::Connected(snap));
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

fn write(
    c: &Ctx,
    s: &mut Session,
    current: &Backup,
    plan: &[memory::SectorWrite],
    active: u16,
    label: &str,
) -> hidpp::Result<()> {
    let n = plan.len();
    s.m.write_plan(current, plan, active, label, &mut |i, _| {
        (c.emit)(Ev::Busy(format!("Writing {} of {n}", i + 1)));
    })
}

fn release(s: &mut Session) {
    let _ = s.m.led_release();
    s.previewing = false;
}

fn handle(c: &Ctx, s: &mut Session, op: Op) -> hidpp::Result<Value> {
    match op {
        Op::Reload => {
            reload(c, s)?;
            Ok(Value::Null)
        }
        Op::BackupNow => {
            let b = s.m.read_backup()?;
            let path = s.m.save_backup(&b, "manual")?;
            Ok(json!({ "path": path }))
        }
        Op::Activate(sector) => {
            s.m.activate(sector)?;
            release(s);
            s.last = s.m.active()?;
            (c.emit)(Ev::Active {
                sector: s.last.0,
                mode: s.last.1,
            });
            Ok(Value::Null)
        }
        Op::LedPreview { zone, led } => {
            s.m.led_preview(zone, &led)?;
            s.previewing = true;
            Ok(Value::Null)
        }
        Op::LedRelease => {
            s.m.led_release()?;
            s.previewing = false;
            Ok(Value::Null)
        }
        Op::Apply { edits, active } => {
            (c.emit)(Ev::Busy("Checking the mouse".into()));
            let current = s.m.read_backup()?;
            let fresh = OnboardMemory::from_backup(&current)?;
            if user_sectors(&fresh) != user_sectors(&s.loaded) {
                reload(c, s)?;
                return Err(Error::Invalid(
                    "the mouse's memory changed since it was loaded. It has been reloaded; nothing was written".into(),
                ));
            }
            let mut edited = fresh.clone();
            for e in edits {
                let slot = edited
                    .profiles
                    .get_mut(e.index)
                    .ok_or_else(|| Error::Invalid(format!("no profile {}", e.index + 1)))?;
                slot.profile.apply_from(&e.profile);
                slot.enabled = e.enabled;
            }
            let active = edited
                .profiles
                .iter()
                .find(|p| p.sector == active && p.enabled)
                .or_else(|| edited.profiles.iter().find(|p| p.enabled))
                .map(|p| p.sector)
                .unwrap_or(active);
            let plan = fresh.plan(&edited)?;
            write(c, s, &current, &plan, active, "pre-write")?;
            let after = OnboardMemory::from_backup(&s.m.read_backup()?)?;
            let same = after
                .profiles
                .iter()
                .zip(&edited.profiles)
                .all(|(a, b)| a.profile == b.profile && a.enabled == b.enabled);
            release(s);
            reload(c, s)?;
            if !same {
                return Err(Error::Invalid(
                    "the read-back after writing does not match the edit".into(),
                ));
            }
            Ok(json!({ "sectors": plan.len() }))
        }
        Op::Restore(path) => {
            let wanted = Backup::load(&path)?;
            let current = s.m.read_backup()?;
            let plan = memory::restore_plan(&wanted, &current)?;
            let active = current
                .current_profile_sector
                .filter(|s| *s < ROM_BASE)
                .unwrap_or(1);
            write(c, s, &current, &plan, active, "pre-restore")?;
            release(s);
            reload(c, s)?;
            Ok(json!({ "sectors": plan.len() }))
        }
        Op::FactoryReset => {
            let current = s.m.read_backup()?;
            let mem = OnboardMemory::from_backup(&current)?;
            let reset = mem.factory_reset();
            let plan = mem.plan(&reset)?;
            let active = reset
                .profiles
                .iter()
                .find(|p| p.enabled)
                .map(|p| p.sector)
                .unwrap_or(1);
            write(c, s, &current, &plan, active, "pre-factory-reset")?;
            release(s);
            reload(c, s)?;
            Ok(json!({ "sectors": plan.len() }))
        }
        Op::Export { profile, path } => {
            let b = &s.loaded;
            let vid = 0x046d;
            ProfileExport::new(vid, b.product_id, &profile).save(&path)?;
            Ok(json!({ "path": path }))
        }
        Op::Import { index, path } => {
            let ex = ProfileExport::load(&path)?;
            let mut target = s
                .loaded
                .profiles
                .get(index)
                .ok_or_else(|| Error::Invalid(format!("no profile {}", index + 1)))?
                .profile
                .clone();
            ex.apply_to(&mut target)?;
            let l = keymap::current_layout();
            Ok(json!({ "profile": target, "actions": actions_for(&target, l) }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mouse::MockConnector;
    use hidpp::profile::{Binding, KeyCombo};
    use std::sync::{Arc, Mutex};

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../hidpp/tests/data/c08b-2026-10-29.json")
    }

    fn start() -> (Worker, Arc<Mutex<Vec<Ev>>>, Box<Snapshot>) {
        let evs = Arc::new(Mutex::new(Vec::new()));
        let sink = evs.clone();
        let w = Worker::spawn(
            Box::new(MockConnector::load(&fixture()).unwrap()),
            move |e| sink.lock().unwrap().push(e),
        );
        let snap = wait_connected(&evs, 0);
        (w, evs, snap)
    }

    fn wait_connected(evs: &Arc<Mutex<Vec<Ev>>>, skip: usize) -> Box<Snapshot> {
        for _ in 0..200 {
            let found = evs
                .lock()
                .unwrap()
                .iter()
                .skip(skip)
                .rev()
                .find_map(|e| match e {
                    Ev::Connected(s) => Some(s.clone()),
                    _ => None,
                });
            if let Some(s) = found {
                return s;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("never connected");
    }

    #[test]
    fn mock_serves_the_fixture() {
        let (_w, _e, snap) = start();
        assert!(snap.mock);
        assert_eq!(snap.profiles.len(), 5);
        assert_eq!(snap.slots.len(), 11);
        assert!(snap.has_gshift);
        assert_eq!(snap.dpi_values.first(), Some(&100));
        assert_eq!(snap.dpi_values.last(), Some(&25600));
        assert_eq!(snap.led_zones.len(), 2);
    }

    #[test]
    fn apply_round_trips_through_the_mock() {
        let (w, evs, snap) = start();
        let mut p = snap.profiles[1].profile.clone();
        p.buttons[6] = Binding::Key(KeyCombo {
            modifiers: hidpp::keys::modifier::LCTRL,
            key: 0x06,
        });
        p.report_interval_ms = 2;
        let n = evs.lock().unwrap().len();
        let r = w
            .call(Op::Apply {
                edits: vec![ProfileEdit {
                    index: 1,
                    enabled: true,
                    profile: p,
                }],
                active: snap.active_sector,
            })
            .unwrap();
        assert_eq!(r["sectors"], 1);
        let after = wait_connected(&evs, n);
        let prof = &after.profiles[1];
        assert_eq!(prof.profile.report_interval_ms, 2);
        assert_eq!(prof.profile.buttons[6], snap.profiles[2].profile.buttons[6]);
        // Everything else is untouched.
        assert_eq!(after.profiles[0].profile, snap.profiles[0].profile);
        assert_eq!(
            after.profiles[1].profile.buttons[7],
            snap.profiles[1].profile.buttons[7]
        );
    }

    #[test]
    fn disabling_every_profile_is_refused() {
        let (w, _e, snap) = start();
        let edits = snap
            .profiles
            .iter()
            .map(|p| ProfileEdit {
                index: p.index,
                enabled: false,
                profile: p.profile.clone(),
            })
            .collect();
        let err = w
            .call(Op::Apply {
                edits,
                active: snap.active_sector,
            })
            .unwrap_err();
        assert!(err.contains("enabled"), "{err}");
    }

    #[test]
    fn preview_then_release() {
        let (w, _e, _snap) = start();
        w.call(Op::LedPreview {
            zone: 0,
            led: Led::Static {
                color: hidpp::profile::Rgb(255, 0, 0),
                ramp: 0,
            },
        })
        .unwrap();
        w.call(Op::LedRelease).unwrap();
    }
}
