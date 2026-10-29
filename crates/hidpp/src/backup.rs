//! Raw memory backups: every user sector plus the ROM directory and ROM
//! profiles, hex encoded in a JSON file.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::crc;
use crate::device::Device;
use crate::error::{Error, Result};
use crate::onboard::{BackupProof, OnboardInfo, ROM_BASE};

pub const FORMAT: &str = "mouseforge-raw-backup";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupDevice {
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
    pub firmware: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorDump {
    pub sector: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub crc_valid: bool,
}

impl SectorDump {
    pub fn bytes(&self) -> Option<Vec<u8>> {
        self.data.as_deref().and_then(|h| from_hex(h).ok())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub format: String,
    pub version: u32,
    pub created_utc: String,
    pub device: BackupDevice,
    pub info: OnboardInfo,
    pub onboard_mode: Option<u8>,
    pub current_profile_sector: Option<u16>,
    pub sectors: Vec<SectorDump>,
}

impl Backup {
    /// Read the whole onboard memory that matters.
    pub fn read_from(dev: &mut Device) -> Result<Backup> {
        let info = dev.onboard_info()?;
        info.validate()?;
        let name = dev.device_name().unwrap_or_default();
        let firmware = dev.firmware().unwrap_or_default();
        let onboard_mode = dev.onboard_mode().ok();
        let current_profile_sector = dev.current_profile_sector().ok();

        let mut list: Vec<u16> = (0..u16::from(info.sector_count)).collect();
        list.extend(ROM_BASE..=ROM_BASE + u16::from(info.rom_profile_count));

        let mut sectors = Vec::with_capacity(list.len());
        for s in list {
            match dev.read_sector(s, info.sector_size) {
                Ok(d) => sectors.push(SectorDump {
                    sector: s,
                    crc_valid: crc::is_valid(&d),
                    data: Some(to_hex(&d)),
                    error: None,
                }),
                Err(e @ (Error::Disconnected | Error::Hid(_))) => return Err(e),
                Err(e) => sectors.push(SectorDump {
                    sector: s,
                    data: None,
                    error: Some(e.to_string()),
                    crc_valid: false,
                }),
            }
        }

        Ok(Backup {
            format: FORMAT.into(),
            version: VERSION,
            created_utc: utc_timestamp(SystemTime::now()),
            device: BackupDevice {
                vendor_id: dev.candidate.vendor_id,
                product_id: dev.candidate.product_id,
                name,
                firmware,
            },
            info,
            onboard_mode,
            current_profile_sector,
            sectors,
        })
    }

    pub fn sector(&self, s: u16) -> Option<Vec<u8>> {
        self.sectors
            .iter()
            .find(|d| d.sector == s)
            .and_then(|d| d.bytes())
    }

    /// Write to `dir/backup-<timestamp>[-label].json` and return proof that it exists.
    pub fn save(&self, dir: &Path, label: &str) -> Result<BackupProof> {
        std::fs::create_dir_all(dir)?;
        let stamp = self
            .created_utc
            .replace([':', '-'], "")
            .replace('T', "-")
            .replace('Z', "");
        let mut name = format!("backup-{stamp}");
        if !label.is_empty() {
            name.push('-');
            name.push_str(label);
        }
        let mut path = dir.join(format!("{name}.json"));
        let mut n = 1;
        while path.exists() {
            path = dir.join(format!("{name}-{n}.json"));
            n += 1;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, &text)?;
        // Make sure what is on disk parses back to the same thing.
        let check: Backup = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        if check.sectors.len() != self.sectors.len() {
            return Err(Error::Invalid(
                "backup file did not verify after writing".into(),
            ));
        }
        Ok(BackupProof { path })
    }

    pub fn load(path: &Path) -> Result<Backup> {
        let b: Backup = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        if b.format != FORMAT {
            return Err(Error::Invalid(format!(
                "not a backup file (format {:?})",
                b.format
            )));
        }
        let size = usize::from(b.info.sector_size);
        for s in &b.sectors {
            if let Some(d) = &s.data {
                let bytes = from_hex(d)?;
                if bytes.len() != size {
                    return Err(Error::Invalid(format!(
                        "sector {:#06x} has {} bytes, expected {size}",
                        s.sector,
                        bytes.len()
                    )));
                }
            }
        }
        Ok(b)
    }

    /// User sectors a restore would write: present, CRC-valid, writable.
    pub fn restorable_sectors(&self) -> Vec<(u16, Vec<u8>)> {
        self.sectors
            .iter()
            .filter(|s| s.sector < ROM_BASE && s.sector < u16::from(self.info.sector_count))
            .filter_map(|s| s.bytes().map(|b| (s.sector, b)))
            .filter(|(_, b)| crc::is_valid(b))
            .collect()
    }
}

pub fn to_hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(s: &str) -> Result<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !s.len().is_multiple_of(2) {
        return Err(Error::Invalid("odd-length hex string".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| Error::Invalid(format!("bad hex at {i}")))
        })
        .collect()
}

/// `YYYY-MM-DDTHH:MM:SSZ` without pulling in a date crate.
pub fn utc_timestamp(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn default_backup_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Tuckaway")
        .join("backups")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn timestamp() {
        assert_eq!(utc_timestamp(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(
            utc_timestamp(UNIX_EPOCH + Duration::from_secs(1_793_278_800)),
            "2026-10-29T13:00:00Z"
        );
        assert_eq!(
            utc_timestamp(UNIX_EPOCH + Duration::from_secs(951_782_400)),
            "2000-02-29T00:00:00Z"
        );
    }

    #[test]
    fn hex_round_trip() {
        let d = [0u8, 1, 0xab, 0xff];
        assert_eq!(from_hex(&to_hex(&d)).unwrap(), d);
    }
}
