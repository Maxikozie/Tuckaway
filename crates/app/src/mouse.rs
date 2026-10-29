//! The device behind the UI: the real mouse over HID++, or a mock that serves
//! a raw backup (the test fixture) from memory. The worker only sees [`Mouse`]
//! and [`Connector`], so every code path above this file is the same for both.

use std::path::{Path, PathBuf};

use hidpp::backup::{self, Backup};
use hidpp::device::{self, LedZone};
use hidpp::features::COLOR_LED_EFFECTS;
use hidpp::memory::{self, SectorWrite};
use hidpp::onboard::MODE_ONBOARD;
use hidpp::profile::Led;
use hidpp::{Device, Error, HidApi, Result};

/// 0x8070 functions used for the live lighting preview (Solaar "LED Control").
const FN_SET_ZONE_EFFECT: u8 = 3;
const FN_SET_SW_CONTROL: u8 = 8;

pub trait Mouse: Send {
    /// Stable identity used for "first connect this session" backups.
    fn key(&self) -> String;
    fn id_string(&self) -> String;
    fn is_mock(&self) -> bool;
    fn read_backup(&mut self) -> Result<Backup>;
    fn save_backup(&mut self, b: &Backup, label: &str) -> Result<Option<PathBuf>>;
    fn dpi_list(&mut self) -> Vec<u16>;
    fn report_rates_ms(&mut self) -> Vec<u8>;
    fn led_zones(&mut self) -> Vec<LedZone>;
    /// (current profile sector, onboard mode)
    fn active(&mut self) -> Result<(u16, u8)>;
    /// Back up `current`, write and verify every sector, then onboard mode +
    /// re-select `active`.
    fn write_plan(
        &mut self,
        current: &Backup,
        plan: &[SectorWrite],
        active: u16,
        label: &str,
        progress: &mut dyn FnMut(usize, u16),
    ) -> Result<()>;
    fn activate(&mut self, sector: u16) -> Result<()>;
    /// Take the LEDs from the firmware and show `led` on `zone` (RAM only).
    fn led_preview(&mut self, zone: u8, led: &Led) -> Result<()>;
    /// Hand the LEDs back to the firmware (it shows the active profile again).
    fn led_release(&mut self) -> Result<()>;
}

pub trait Connector: Send {
    fn find(&mut self) -> Option<Result<Box<dyn Mouse>>>;
    fn present(&mut self, key: &str) -> bool;
}

// ---------------------------------------------------------------- real

pub struct RealConnector {
    api: HidApi,
}

impl RealConnector {
    pub fn new() -> Result<Self> {
        Ok(RealConnector {
            api: HidApi::new()?,
        })
    }
}

impl Connector for RealConnector {
    fn find(&mut self) -> Option<Result<Box<dyn Mouse>>> {
        let _ = self.api.refresh_devices();
        let cand = device::find_mice(&self.api).into_iter().next()?;
        Some(Device::open(&self.api, &cand).map(|d| Box::new(RealMouse { d }) as Box<dyn Mouse>))
    }

    fn present(&mut self, key: &str) -> bool {
        let _ = self.api.refresh_devices();
        device::find_mice(&self.api).iter().any(|m| m.key() == key)
    }
}

pub struct RealMouse {
    d: Device,
}

impl Mouse for RealMouse {
    fn key(&self) -> String {
        self.d.candidate.key()
    }
    fn id_string(&self) -> String {
        self.d.candidate.id_string()
    }
    fn is_mock(&self) -> bool {
        false
    }
    fn read_backup(&mut self) -> Result<Backup> {
        Backup::read_from(&mut self.d)
    }
    fn save_backup(&mut self, b: &Backup, label: &str) -> Result<Option<PathBuf>> {
        let proof = b.save(&backup::default_backup_dir(), label)?;
        Ok(Some(proof.path().to_path_buf()))
    }
    fn dpi_list(&mut self) -> Vec<u16> {
        self.d.dpi_list().unwrap_or_default()
    }
    fn report_rates_ms(&mut self) -> Vec<u8> {
        self.d
            .report_rates_ms()
            .unwrap_or_else(|_| vec![1, 2, 4, 8])
    }
    fn led_zones(&mut self) -> Vec<LedZone> {
        self.d.led_zones().unwrap_or_default()
    }
    fn active(&mut self) -> Result<(u16, u8)> {
        Ok((self.d.current_profile_sector()?, self.d.onboard_mode()?))
    }
    fn write_plan(
        &mut self,
        current: &Backup,
        plan: &[SectorWrite],
        active: u16,
        label: &str,
        progress: &mut dyn FnMut(usize, u16),
    ) -> Result<()> {
        let proof = current.save(&backup::default_backup_dir(), label)?;
        for (i, w) in plan.iter().enumerate() {
            progress(i, w.sector);
            self.d
                .write_sector(&proof, &current.info, w.sector, &w.after)?;
        }
        memory::finish(&mut self.d, active)
    }
    fn activate(&mut self, sector: u16) -> Result<()> {
        memory::finish(&mut self.d, sector)
    }
    fn led_preview(&mut self, zone: u8, led: &Led) -> Result<()> {
        self.d.call(COLOR_LED_EFFECTS, FN_SET_SW_CONTROL, &[1])?;
        let mut p = vec![zone];
        p.extend_from_slice(&led.encode()?);
        p.push(0); // persist: RAM only
        self.d.call(COLOR_LED_EFFECTS, FN_SET_ZONE_EFFECT, &p)?;
        Ok(())
    }
    fn led_release(&mut self) -> Result<()> {
        self.d.call(COLOR_LED_EFFECTS, FN_SET_SW_CONTROL, &[0])?;
        Ok(())
    }
}

// ---------------------------------------------------------------- mock

/// Serves a raw backup as if it were the mouse. Writes change only memory.
pub struct MockConnector {
    backup: Option<Backup>,
}

impl MockConnector {
    pub fn load(path: &Path) -> Result<Self> {
        Ok(MockConnector {
            backup: Some(Backup::load(path)?),
        })
    }
}

impl Connector for MockConnector {
    fn find(&mut self) -> Option<Result<Box<dyn Mouse>>> {
        let b = self.backup.take()?;
        Some(Ok(Box::new(MockMouse::new(b))))
    }
    fn present(&mut self, _: &str) -> bool {
        true
    }
}

pub struct MockMouse {
    pub backup: Backup,
    pub sw_control: bool,
    pub preview: Vec<(u8, Led)>,
}

impl MockMouse {
    pub fn new(backup: Backup) -> Self {
        MockMouse {
            backup,
            sw_control: false,
            preview: Vec::new(),
        }
    }
}

impl Mouse for MockMouse {
    fn key(&self) -> String {
        "mock".into()
    }
    fn id_string(&self) -> String {
        format!(
            "{:04x}:{:04x} (mock)",
            self.backup.device.vendor_id, self.backup.device.product_id
        )
    }
    fn is_mock(&self) -> bool {
        true
    }
    fn read_backup(&mut self) -> Result<Backup> {
        Ok(self.backup.clone())
    }
    fn save_backup(&mut self, _: &Backup, _: &str) -> Result<Option<PathBuf>> {
        Ok(None)
    }
    fn dpi_list(&mut self) -> Vec<u16> {
        (100..=25600).step_by(50).collect()
    }
    fn report_rates_ms(&mut self) -> Vec<u8> {
        vec![1, 2, 4, 8]
    }
    fn led_zones(&mut self) -> Vec<LedZone> {
        let effects = vec![0x00, 0x01, 0x0a, 0x03];
        vec![
            LedZone {
                index: 0,
                location: 1,
                effects: effects.clone(),
            },
            LedZone {
                index: 1,
                location: 2,
                effects,
            },
        ]
    }
    fn active(&mut self) -> Result<(u16, u8)> {
        Ok((
            self.backup.current_profile_sector.unwrap_or(1),
            self.backup.onboard_mode.unwrap_or(MODE_ONBOARD),
        ))
    }
    fn write_plan(
        &mut self,
        _: &Backup,
        plan: &[SectorWrite],
        active: u16,
        _: &str,
        progress: &mut dyn FnMut(usize, u16),
    ) -> Result<()> {
        for (i, w) in plan.iter().enumerate() {
            progress(i, w.sector);
            let dump = self
                .backup
                .sectors
                .iter_mut()
                .find(|s| s.sector == w.sector)
                .ok_or_else(|| Error::Invalid(format!("mock has no sector {:#06x}", w.sector)))?;
            dump.data = Some(backup::to_hex(&w.after));
            dump.error = None;
            dump.crc_valid = true;
        }
        self.activate(active)
    }
    fn activate(&mut self, sector: u16) -> Result<()> {
        self.backup.onboard_mode = Some(MODE_ONBOARD);
        self.backup.current_profile_sector = Some(sector);
        Ok(())
    }
    fn led_preview(&mut self, zone: u8, led: &Led) -> Result<()> {
        led.encode()?;
        eprintln!("mock: LED preview zone {zone}: {}", led.label());
        self.sw_control = true;
        self.preview.retain(|(z, _)| *z != zone);
        self.preview.push((zone, led.clone()));
        Ok(())
    }
    fn led_release(&mut self) -> Result<()> {
        if self.sw_control {
            eprintln!("mock: LEDs handed back to the firmware");
        }
        self.sw_control = false;
        self.preview.clear();
        Ok(())
    }
}
