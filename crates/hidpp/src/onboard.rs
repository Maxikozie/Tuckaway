//! Onboard Profiles feature (0x8100): description, mode, active profile and
//! raw sector memory access.
//!
//! Function numbers and framing are taken from libratbag `src/hidpp20.c`
//! (CMD_ONBOARD_PROFILES_*) and Solaar `hidpp20.py` (`OnboardProfiles`):
//!
//! | fn | name                  | params                                   |
//! |----|-----------------------|------------------------------------------|
//! | 0  | getProfilesDescr      | -                                        |
//! | 1  | setOnboardMode        | mode (1 = onboard, 2 = host)             |
//! | 2  | getOnboardMode        | -                                        |
//! | 3  | setCurrentProfile     | sector (BE16)                            |
//! | 4  | getCurrentProfile     | -> sector (BE16)                         |
//! | 5  | memoryRead            | sector (BE16), offset (BE16) -> 16 bytes |
//! | 6  | memoryAddrWrite       | sector (BE16), offset (BE16), len (BE16) |
//! | 7  | memoryWrite           | 16 data bytes                            |
//! | 8  | memoryWriteEnd        | -                                        |
//! | 11 | getCurrentDpiIndex    | -> index                                 |
//! | 12 | setCurrentDpiIndex    | index                                    |

use serde::{Deserialize, Serialize};

use crate::crc;
use crate::device::Device;
use crate::error::{Error, Result};
use crate::features::ONBOARD_PROFILES;

pub const MODE_ONBOARD: u8 = 0x01;
pub const MODE_HOST: u8 = 0x02;

/// User (writable) memory: sector 0 is the profile directory.
pub const USER_DIRECTORY_SECTOR: u16 = 0x0000;
/// ROM (factory, read-only) memory: 0x0100 is the ROM directory and
/// 0x0101.. are the ROM profiles.
pub const ROM_BASE: u16 = 0x0100;

pub const MEMORY_MODEL_G402: u8 = 0x01;
pub const MACRO_FORMAT_G402: u8 = 0x01;

/// Reply of getProfilesDescr (`struct hidpp20_onboard_profiles_info`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnboardInfo {
    pub memory_model: u8,
    pub profile_format: u8,
    pub macro_format: u8,
    pub profile_count: u8,
    pub rom_profile_count: u8,
    pub button_count: u8,
    pub sector_count: u8,
    pub sector_size: u16,
    pub mechanical_layout: u8,
    pub various_info: u8,
}

impl OnboardInfo {
    pub fn parse(r: &[u8]) -> Self {
        OnboardInfo {
            memory_model: r[0],
            profile_format: r[1],
            macro_format: r[2],
            profile_count: r[3],
            rom_profile_count: r[4],
            button_count: r[5],
            sector_count: r[6],
            sector_size: u16::from_be_bytes([r[7], r[8]]),
            mechanical_layout: r[9],
            various_info: r[10],
        }
    }

    /// libratbag: `(mechanical_layout & 0x03) == 0x02`. Solaar uses the same
    /// test to decide whether the second ("G-shift") button table is present.
    pub fn has_g_shift(&self) -> bool {
        self.mechanical_layout & 0x03 == 0x02
    }

    /// libratbag: `((mechanical_layout & 0x0c) >> 2) == 0x02`.
    pub fn has_dpi_shift(&self) -> bool {
        (self.mechanical_layout & 0x0c) >> 2 == 0x02
    }

    pub fn corded(&self) -> bool {
        matches!(self.various_info & 0x07, 1 | 4)
    }

    /// Number of button slots stored per profile (libratbag caps at 16).
    pub fn buttons(&self) -> usize {
        usize::from(self.button_count.min(16))
    }

    /// Checks libratbag's `hidpp20_onboard_profiles_validate()` conditions,
    /// plus a sector size the profile layout fits in.
    pub fn validate(&self) -> Result<()> {
        if self.memory_model != MEMORY_MODEL_G402 {
            return Err(Error::UnsupportedFormat(format!(
                "memory model {:#04x}",
                self.memory_model
            )));
        }
        if !(1..=5).contains(&self.profile_format) {
            return Err(Error::UnsupportedFormat(format!(
                "profile format {:#04x}",
                self.profile_format
            )));
        }
        if self.macro_format != MACRO_FORMAT_G402 {
            return Err(Error::UnsupportedFormat(format!(
                "macro format {:#04x}",
                self.macro_format
            )));
        }
        if self.sector_size < 0xFF || self.sector_size > 0x1000 {
            return Err(Error::UnsupportedFormat(format!(
                "sector size {}",
                self.sector_size
            )));
        }
        Ok(())
    }
}

/// Proof that a backup of the current memory exists on disk. Only
/// [`crate::backup::Backup::save`] creates one, and every write API demands it.
#[derive(Debug)]
pub struct BackupProof {
    pub(crate) path: std::path::PathBuf,
}

impl BackupProof {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Device {
    pub fn onboard_info(&mut self) -> Result<OnboardInfo> {
        let r = self.call(ONBOARD_PROFILES, 0, &[])?;
        Ok(OnboardInfo::parse(&r))
    }

    pub fn onboard_mode(&mut self) -> Result<u8> {
        Ok(self.call(ONBOARD_PROFILES, 2, &[])?[0])
    }

    pub fn set_onboard_mode(&mut self, mode: u8) -> Result<()> {
        self.call(ONBOARD_PROFILES, 1, &[mode])?;
        Ok(())
    }

    /// Sector of the active profile (0x0001.. for user, 0x0101.. for ROM).
    pub fn current_profile_sector(&mut self) -> Result<u16> {
        let r = self.call(ONBOARD_PROFILES, 4, &[])?;
        Ok(u16::from_be_bytes([r[0], r[1]]))
    }

    pub fn set_current_profile_sector(&mut self, sector: u16) -> Result<()> {
        self.call(ONBOARD_PROFILES, 3, &sector.to_be_bytes())?;
        Ok(())
    }

    pub fn current_dpi_index(&mut self) -> Result<u8> {
        Ok(self.call(ONBOARD_PROFILES, 11, &[])?[0])
    }

    /// Read one sector. Like libratbag, reads in 16-byte chunks; the firmware
    /// rejects reads past `size - 16`, so the last chunk is re-aligned to end
    /// exactly at the end of the sector.
    pub fn read_sector(&mut self, sector: u16, size: u16) -> Result<Vec<u8>> {
        let size = usize::from(size);
        let mut data = vec![0u8; size];
        let mut offset = 0usize;
        while offset < size {
            let at = if size - offset < 16 {
                size - 16
            } else {
                offset
            };
            let mut p = [0u8; 4];
            p[..2].copy_from_slice(&sector.to_be_bytes());
            p[2..].copy_from_slice(&(at as u16).to_be_bytes());
            let r = self.call(ONBOARD_PROFILES, 5, &p)?;
            data[at..at + 16].copy_from_slice(&r);
            offset = at + 16;
        }
        Ok(data)
    }

    /// Write one user sector, then read it back and compare.
    ///
    /// Refuses ROM sectors, sectors beyond `sector_count`, data of the wrong
    /// length, and data whose stored CRC is wrong. Requires a [`BackupProof`].
    pub fn write_sector(
        &mut self,
        _backup: &BackupProof,
        info: &OnboardInfo,
        sector: u16,
        data: &[u8],
    ) -> Result<()> {
        if sector >= ROM_BASE || sector >= u16::from(info.sector_count) {
            return Err(Error::WriteRefused(format!(
                "sector {sector:#06x} is not a writable user sector"
            )));
        }
        if data.len() != usize::from(info.sector_size) {
            return Err(Error::WriteRefused(format!(
                "sector data is {} bytes, device sector size is {}",
                data.len(),
                info.sector_size
            )));
        }
        if !crc::is_valid(data) {
            return Err(Error::WriteRefused(format!(
                "sector {sector:#06x} data has an invalid CRC"
            )));
        }

        // A failed or mismatching attempt is retried once from the start of
        // the sector; streamed chunks themselves are never resent.
        let mut last_err = Error::VerifyFailed(sector);
        for _ in 0..2 {
            match self.write_sector_once(sector, data) {
                Ok(()) => match self.read_sector(sector, info.sector_size) {
                    Ok(back) if back == data => return Ok(()),
                    Ok(_) => last_err = Error::VerifyFailed(sector),
                    Err(e) => last_err = e,
                },
                Err(e @ (Error::Disconnected | Error::Hid(_))) => return Err(e),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    /// memoryAddrWrite(sector, offset 0, length), 16-byte memoryWrite chunks,
    /// memoryWriteEnd: the sequence of libratbag's
    /// `hidpp20_onboard_profiles_write_sector()` and Solaar's `write_sector()`.
    fn write_sector_once(&mut self, sector: u16, data: &[u8]) -> Result<()> {
        let mut p = [0u8; 6];
        p[..2].copy_from_slice(&sector.to_be_bytes());
        p[4..].copy_from_slice(&(data.len() as u16).to_be_bytes());
        self.call_once(ONBOARD_PROFILES, 6, &p)?;
        for chunk in data.chunks(16) {
            let mut c = [0u8; 16];
            c[..chunk.len()].copy_from_slice(chunk);
            self.call_once(ONBOARD_PROFILES, 7, &c)?;
        }
        self.call_once(ONBOARD_PROFILES, 8, &[])?;
        Ok(())
    }
}
