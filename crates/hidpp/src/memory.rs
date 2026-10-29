//! The decoded onboard memory as a whole: directory + profiles, ROM fallback,
//! factory reset, write planning, diffing and applying.

use serde::{Deserialize, Serialize};

use crate::backup::Backup;
use crate::crc;
use crate::device::Device;
use crate::error::{Error, Result};
use crate::layout;
use crate::onboard::{BackupProof, OnboardInfo, MODE_ONBOARD, ROM_BASE, USER_DIRECTORY_SECTOR};
use crate::profile::{DirEntry, Directory, Profile, DPI_SLOTS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// Decoded from a CRC-valid user sector.
    User,
    /// User sector missing or corrupt; decoded from this ROM sector instead
    /// (libratbag `hidpp20_onboard_profiles_initialize` fallback).
    Rom(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileSlot {
    /// 0-based profile number.
    pub index: usize,
    /// User sector the profile lives in (and is written to).
    pub sector: u16,
    pub enabled: bool,
    pub source: Source,
    pub profile: Profile,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OnboardMemory {
    pub info: OnboardInfo,
    pub product_id: u16,
    pub directory_valid: bool,
    pub profiles: Vec<ProfileSlot>,
    /// Factory profiles, decoded from ROM sectors 0x0101...
    pub rom_profiles: Vec<Profile>,
    /// Enabled flags from the ROM directory (sector 0x0100).
    pub rom_enabled: Vec<bool>,
    /// The raw sectors these were decoded from (for diffing writes).
    pub source_sectors: Vec<(u16, Vec<u8>)>,
}

/// One planned sector write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectorWrite {
    pub sector: u16,
    pub before: Option<Vec<u8>>,
    pub after: Vec<u8>,
}

impl OnboardMemory {
    /// Decode from a raw backup (live reads go through [`Backup::read_from`] too).
    pub fn from_backup(b: &Backup) -> Result<Self> {
        let info = b.info;
        info.validate()?;
        let count = usize::from(info.profile_count);
        let rom_count = usize::from(info.rom_profile_count);

        // ROM: profiles have no CRC (libratbag reads them with check_crc = false).
        let mut rom_profiles = Vec::with_capacity(rom_count);
        for i in 0..rom_count {
            let s = ROM_BASE + 1 + i as u16;
            let raw = b
                .sector(s)
                .ok_or_else(|| Error::Invalid(format!("ROM sector {s:#06x} missing from dump")))?;
            rom_profiles.push(Profile::decode(&raw, &info)?);
        }
        let rom_dir = b
            .sector(ROM_BASE)
            .map(|r| Directory::decode(&r, rom_count))
            .unwrap_or(Directory { entries: vec![] });
        let rom_enabled: Vec<bool> = (0..rom_count)
            .map(|i| rom_dir.entries.get(i).map(|e| e.enabled).unwrap_or(i == 0))
            .collect();

        let dir_raw = b.sector(USER_DIRECTORY_SECTOR);
        let directory_valid = dir_raw.as_deref().is_some_and(crc::is_valid);
        let directory = dir_raw
            .as_deref()
            .filter(|_| directory_valid)
            .map(|r| Directory::decode(r, count));

        let mut profiles = Vec::with_capacity(count);
        for i in 0..count {
            let default_sector = i as u16 + 1;
            let (sector, enabled) = match &directory {
                Some(d) => match d.entries.get(i) {
                    Some(e) if e.sector > 0 && e.sector < u16::from(info.sector_count) => {
                        (e.sector, e.enabled)
                    }
                    Some(e) => (default_sector, e.enabled),
                    None => (default_sector, false),
                },
                None => (
                    default_sector,
                    rom_enabled.get(i).copied().unwrap_or(i == 0),
                ),
            };
            let user = b
                .sector(sector)
                .filter(|r| directory_valid && crc::is_valid(r));
            let (source, profile) = match user {
                Some(raw) => (Source::User, Profile::decode(&raw, &info)?),
                None => {
                    let ri = if i < rom_count { i } else { 0 };
                    (
                        Source::Rom(ROM_BASE + 1 + ri as u16),
                        rom_profiles[ri].clone(),
                    )
                }
            };
            profiles.push(ProfileSlot {
                index: i,
                sector,
                enabled,
                source,
                profile,
            });
        }

        let source_sectors = b
            .sectors
            .iter()
            .filter(|s| s.sector < ROM_BASE)
            .filter_map(|s| s.bytes().map(|d| (s.sector, d)))
            .collect();

        Ok(OnboardMemory {
            info,
            product_id: b.device.product_id,
            directory_valid,
            profiles,
            rom_profiles,
            rom_enabled,
            source_sectors,
        })
    }

    pub fn directory(&self) -> Directory {
        Directory {
            entries: self
                .profiles
                .iter()
                .map(|p| DirEntry {
                    sector: p.sector,
                    enabled: p.enabled,
                })
                .collect(),
        }
    }

    pub fn current_sector_bytes(&self, sector: u16) -> Option<&[u8]> {
        self.source_sectors
            .iter()
            .find(|(s, _)| *s == sector)
            .map(|(_, d)| d.as_slice())
    }

    /// The same memory with every profile replaced by its factory ROM
    /// profile and the directory reset to the ROM directory's enabled flags.
    pub fn factory_reset(&self) -> OnboardMemory {
        let mut m = self.clone();
        for p in &mut m.profiles {
            let ri = if p.index < m.rom_profiles.len() {
                p.index
            } else {
                0
            };
            p.profile = m.rom_profiles[ri].clone();
            p.sector = p.index as u16 + 1;
            p.enabled = m.rom_enabled.get(p.index).copied().unwrap_or(p.index == 0);
        }
        m
    }

    /// Sector writes needed to turn the current memory (`self`) into `edited`.
    /// Profiles first, directory last (libratbag `hidpp20_onboard_profiles_commit`).
    pub fn plan(&self, edited: &OnboardMemory) -> Result<Vec<SectorWrite>> {
        if !edited.profiles.iter().any(|p| p.enabled) {
            return Err(Error::Invalid(
                "at least one profile must stay enabled".into(),
            ));
        }
        let size = usize::from(self.info.sector_size);
        let mut writes = Vec::new();
        for (new, old) in edited.profiles.iter().zip(&self.profiles) {
            let after = new.profile.encode()?;
            let before = self.current_sector_bytes(new.sector).map(<[u8]>::to_vec);
            if before.as_deref() == Some(after.as_slice()) {
                continue;
            }
            // Untouched, disabled ROM-fallback profiles stay unwritten.
            let changed = new.profile != old.profile || new.sector != old.sector;
            if !new.enabled && !changed && old.source != Source::User {
                continue;
            }
            writes.push(SectorWrite {
                sector: new.sector,
                before,
                after,
            });
        }
        let dir_after = edited.directory().encode(size);
        let dir_before = self
            .current_sector_bytes(USER_DIRECTORY_SECTOR)
            .map(<[u8]>::to_vec);
        if dir_before.as_deref() != Some(dir_after.as_slice()) {
            writes.push(SectorWrite {
                sector: USER_DIRECTORY_SECTOR,
                before: dir_before,
                after: dir_after,
            });
        }
        for w in &writes {
            if !crc::is_valid(&w.after) || w.after.len() != size {
                return Err(Error::WriteRefused(format!(
                    "planned sector {:#06x} failed validation",
                    w.sector
                )));
            }
        }
        Ok(writes)
    }

    /// Human-readable changes from `self` to `edited`.
    pub fn diff(&self, edited: &OnboardMemory) -> Vec<String> {
        let mut out = Vec::new();
        for (new, old) in edited.profiles.iter().zip(&self.profiles) {
            let n = new.index + 1;
            if new.enabled != old.enabled {
                out.push(format!(
                    "Profile {n}: {}",
                    if new.enabled { "enabled" } else { "disabled" }
                ));
            }
            for line in diff_profile(self.product_id, &old.profile, &new.profile) {
                out.push(format!("Profile {n}: {line}"));
            }
        }
        out
    }
}

pub fn diff_profile(product_id: u16, a: &Profile, b: &Profile) -> Vec<String> {
    let mut out = Vec::new();
    if a.report_interval_ms != b.report_interval_ms {
        out.push(format!(
            "report rate {} Hz → {} Hz",
            a.report_rate_hz(),
            b.report_rate_hz()
        ));
    }
    if a.dpi != b.dpi {
        out.push(format!(
            "DPI levels {:?} → {:?}",
            a.dpi_levels(),
            b.dpi_levels()
        ));
    }
    if a.default_dpi_index != b.default_dpi_index {
        out.push(format!(
            "default DPI level {} → {}",
            a.default_dpi_index + 1,
            b.default_dpi_index + 1
        ));
    }
    if a.shift_dpi_index != b.shift_dpi_index {
        out.push(format!(
            "shift DPI level {} → {}",
            a.shift_dpi_index + 1,
            b.shift_dpi_index + 1
        ));
    }
    if a.angle_snapping != b.angle_snapping {
        out.push(format!(
            "angle snapping {} → {}",
            a.angle_snapping, b.angle_snapping
        ));
    }
    for (i, (x, y)) in a.buttons.iter().zip(&b.buttons).enumerate() {
        if x != y {
            out.push(format!("{}: {x} → {y}", layout::slot_label(product_id, i)));
        }
    }
    for (i, (x, y)) in a.gshift_buttons.iter().zip(&b.gshift_buttons).enumerate() {
        if x != y {
            out.push(format!(
                "{} (G-shift): {x} → {y}",
                layout::slot_label(product_id, i)
            ));
        }
    }
    for (z, (x, y)) in a.leds.iter().zip(&b.leds).enumerate() {
        if x != y {
            out.push(format!("LED zone {z}: {} → {}", x.label(), y.label()));
        }
    }
    if a.name != b.name {
        out.push(format!("name {:?} → {:?}", a.name, b.name));
    }
    debug_assert_eq!(a.dpi.len(), DPI_SLOTS);
    out
}

/// Write the planned sectors (each is verified by read-back inside
/// [`Device::write_sector`]), then make sure the mouse is in onboard mode and
/// re-select `active_sector` so the new settings are loaded.
pub fn apply(
    dev: &mut Device,
    proof: &BackupProof,
    info: &OnboardInfo,
    writes: &[SectorWrite],
    active_sector: u16,
) -> Result<()> {
    for w in writes {
        dev.write_sector(proof, info, w.sector, &w.after)?;
    }
    finish(dev, active_sector)
}

/// Put the mouse in onboard mode with `active_sector` selected, and check it stuck.
pub fn finish(dev: &mut Device, active_sector: u16) -> Result<()> {
    dev.set_onboard_mode(MODE_ONBOARD)?;
    dev.set_current_profile_sector(active_sector)?;
    let mode = dev.onboard_mode()?;
    if mode != MODE_ONBOARD {
        return Err(Error::Invalid(format!(
            "mouse reports mode {mode} after write, expected onboard (1)"
        )));
    }
    Ok(())
}

/// Sector writes that bring `current` back to `wanted` (a raw backup).
/// Only CRC-valid user sectors that differ are written; ROM is never touched.
pub fn restore_plan(wanted: &Backup, current: &Backup) -> Result<Vec<SectorWrite>> {
    if (wanted.device.vendor_id, wanted.device.product_id)
        != (current.device.vendor_id, current.device.product_id)
    {
        return Err(Error::Invalid(format!(
            "backup is for {:04x}:{:04x}, connected mouse is {:04x}:{:04x}",
            wanted.device.vendor_id,
            wanted.device.product_id,
            current.device.vendor_id,
            current.device.product_id
        )));
    }
    if wanted.info != current.info {
        return Err(Error::Invalid(
            "backup memory layout differs from the connected mouse".into(),
        ));
    }
    Ok(wanted
        .restorable_sectors()
        .into_iter()
        .filter(|(s, d)| current.sector(*s).as_deref() != Some(d.as_slice()))
        .map(|(s, d)| SectorWrite {
            sector: s,
            before: current.sector(s),
            after: d,
        })
        .collect())
}
