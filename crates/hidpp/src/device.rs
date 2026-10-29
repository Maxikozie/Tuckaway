//! HID++ 2.0 transport over hidapi.
//!
//! On Windows every top-level HID collection is its own device path, so the
//! vendor collections (usage page 0xFF00) have to be picked out by usage:
//! usage 0x0001 carries short reports (ID 0x10, 7 bytes) and usage 0x0002
//! carries long reports (ID 0x11, 20 bytes). Replies can come back on either
//! collection, so both are polled while waiting.

use std::collections::HashMap;
use std::ffi::CString;
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::features;

pub const VENDOR_ID: u16 = 0x046d;
/// Product IDs this tool has been written and tested against.
pub const SUPPORTED_PRODUCT_IDS: &[u16] = &[0xc08b];

pub const USAGE_PAGE_HIDPP: u16 = 0xFF00;
pub const USAGE_SHORT: u16 = 0x0001;
pub const USAGE_LONG: u16 = 0x0002;

pub const REPORT_SHORT: u8 = 0x10;
pub const REPORT_LONG: u8 = 0x11;
const SHORT_LEN: usize = 7;
const LONG_LEN: usize = 20;

/// Device index used for a directly connected (corded) device.
const DEVICE_INDEX: u8 = 0xFF;
/// Our software id (low nibble of byte 3). Solaar notes 0x07, 0x0A, 0x0B, 0x0D
/// and 0x0F are taken by other tools/firmware.
const SW_ID: u8 = 0x0C;

const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);
const POLL_SLICE_MS: i32 = 2;
const RETRIES: u64 = 3;

/// One HID collection as seen by the OS.
#[derive(Debug, Clone, Serialize)]
pub struct CollectionInfo {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub product: String,
    pub serial: String,
}

/// A mouse we can talk HID++ to: the short and long vendor collections of one
/// interface.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub vendor_id: u16,
    pub product_id: u16,
    pub product: String,
    pub serial: String,
    pub interface: i32,
    #[serde(skip)]
    pub short_path: Option<CString>,
    #[serde(skip)]
    pub long_path: Option<CString>,
}

impl Candidate {
    pub fn id_string(&self) -> String {
        format!("{:04x}:{:04x}", self.vendor_id, self.product_id)
    }

    /// Stable key for hot-plug tracking.
    pub fn key(&self) -> String {
        let p = |c: &Option<CString>| {
            c.as_ref()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        format!("{}|{}", p(&self.short_path), p(&self.long_path))
    }
}

/// Every HID collection from the vendor we know about (for `list`).
pub fn enumerate_collections(api: &HidApi) -> Vec<CollectionInfo> {
    let mut out: Vec<CollectionInfo> = api
        .device_list()
        .filter(|d| d.vendor_id() == VENDOR_ID)
        .map(|d| CollectionInfo {
            path: d.path().to_string_lossy().into_owned(),
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            interface: d.interface_number(),
            usage_page: d.usage_page(),
            usage: d.usage(),
            product: d.product_string().unwrap_or_default().to_string(),
            serial: d.serial_number().unwrap_or_default().to_string(),
        })
        .collect();
    out.sort_by_key(|c| (c.product_id, c.interface, c.usage_page, c.usage));
    out
}

/// Supported mice, grouped per interface.
pub fn find_mice(api: &HidApi) -> Vec<Candidate> {
    let mut by_iface: HashMap<(u16, u16, i32, String), Candidate> = HashMap::new();
    for d in api.device_list() {
        if d.vendor_id() != VENDOR_ID
            || !SUPPORTED_PRODUCT_IDS.contains(&d.product_id())
            || d.usage_page() != USAGE_PAGE_HIDPP
        {
            continue;
        }
        let serial = d.serial_number().unwrap_or_default().to_string();
        let key = (
            d.vendor_id(),
            d.product_id(),
            d.interface_number(),
            serial.clone(),
        );
        let entry = by_iface.entry(key).or_insert_with(|| Candidate {
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            product: d.product_string().unwrap_or_default().to_string(),
            serial,
            interface: d.interface_number(),
            short_path: None,
            long_path: None,
        });
        match d.usage() {
            USAGE_SHORT => entry.short_path = Some(d.path().to_owned()),
            USAGE_LONG => entry.long_path = Some(d.path().to_owned()),
            _ => {}
        }
    }
    let mut v: Vec<Candidate> = by_iface
        .into_values()
        .filter(|c| c.long_path.is_some())
        .collect();
    v.sort_by_key(|c| c.key());
    v
}

/// One entry of the device's feature table.
#[derive(Debug, Clone, Serialize)]
pub struct FeatureEntry {
    pub index: u8,
    pub id: u16,
    pub name: &'static str,
    pub flags: u8,
    pub version: u8,
}

pub struct Device {
    short: Option<HidDevice>,
    long: HidDevice,
    pub candidate: Candidate,
    feature_cache: HashMap<u16, u8>,
    timeout: Duration,
}

impl Device {
    pub fn open(api: &HidApi, candidate: &Candidate) -> Result<Self> {
        let long_path = candidate.long_path.as_ref().ok_or(Error::NotFound)?;
        let long = api.open_path(long_path)?;
        let short = match &candidate.short_path {
            Some(p) => api.open_path(p).ok(),
            None => None,
        };
        let mut dev = Device {
            short,
            long,
            candidate: candidate.clone(),
            feature_cache: HashMap::new(),
            timeout: DEFAULT_TIMEOUT,
        };
        dev.feature_cache.insert(features::ROOT, 0);
        Ok(dev)
    }

    /// Open the first supported mouse.
    pub fn open_first(api: &HidApi) -> Result<Self> {
        let c = find_mice(api).into_iter().next().ok_or(Error::NotFound)?;
        Self::open(api, &c)
    }

    fn drain(&self) {
        let mut buf = [0u8; 64];
        while matches!(self.long.read_timeout(&mut buf, 0), Ok(n) if n > 0) {}
        if let Some(s) = &self.short {
            while matches!(s.read_timeout(&mut buf, 0), Ok(n) if n > 0) {}
        }
    }

    fn send(&self, feature_index: u8, function: u8, params: &[u8]) -> Result<()> {
        let fn_sw = (function << 4) | SW_ID;
        let use_short = params.len() <= SHORT_LEN - 4 && self.short.is_some();
        if use_short {
            let mut buf = [0u8; SHORT_LEN];
            buf[0] = REPORT_SHORT;
            buf[1] = DEVICE_INDEX;
            buf[2] = feature_index;
            buf[3] = fn_sw;
            buf[4..4 + params.len()].copy_from_slice(params);
            self.short.as_ref().unwrap().write(&buf)?;
        } else {
            if params.len() > LONG_LEN - 4 {
                return Err(Error::Invalid(format!(
                    "{} parameter bytes do not fit a long report",
                    params.len()
                )));
            }
            let mut buf = [0u8; LONG_LEN];
            buf[0] = REPORT_LONG;
            buf[1] = DEVICE_INDEX;
            buf[2] = feature_index;
            buf[3] = fn_sw;
            buf[4..4 + params.len()].copy_from_slice(params);
            self.long.write(&buf)?;
        }
        Ok(())
    }

    fn read_one(&self, buf: &mut [u8; 64]) -> Result<usize> {
        let n = self.long.read_timeout(buf, POLL_SLICE_MS)?;
        if n > 0 {
            return Ok(n);
        }
        if let Some(s) = &self.short {
            let n = s.read_timeout(buf, POLL_SLICE_MS)?;
            if n > 0 {
                return Ok(n);
            }
        }
        Ok(0)
    }

    /// Send one HID++ 2.0 request and wait for the matching reply, retrying a
    /// few times on timeouts and "busy" errors.
    /// Returns the 16 parameter bytes of the reply (zero padded for short replies).
    pub fn request(&mut self, feature_index: u8, function: u8, params: &[u8]) -> Result<[u8; 16]> {
        let mut attempt = 0;
        loop {
            match self.request_once(feature_index, function, params) {
                Err(Error::Timeout { .. } | Error::Protocol { code: 0x08, .. })
                    if attempt < RETRIES =>
                {
                    attempt += 1;
                    std::thread::sleep(Duration::from_millis(20 * attempt));
                }
                r => return r,
            }
        }
    }

    fn request_once(&mut self, feature_index: u8, function: u8, params: &[u8]) -> Result<[u8; 16]> {
        self.drain();
        self.send(feature_index, function, params)?;
        let fn_sw = (function << 4) | SW_ID;
        let deadline = Instant::now() + self.timeout;
        let mut buf = [0u8; 64];
        while Instant::now() < deadline {
            let n = match self.read_one(&mut buf) {
                Ok(n) => n,
                Err(Error::Hid(_)) => return Err(Error::Disconnected),
                Err(e) => return Err(e),
            };
            if n < 4 || (buf[0] != REPORT_SHORT && buf[0] != REPORT_LONG) {
                continue;
            }
            let rep = &buf[..n];
            // HID++ 2.0 error: [id, dev, 0xFF, feature_index, fn|sw, code]
            // HID++ 1.0 error: [id, dev, 0x8F, feature_index, fn|sw, code]
            if (rep[2] == 0xFF || rep[2] == 0x8F)
                && n >= 6
                && rep[3] == feature_index
                && rep[4] == fn_sw
            {
                return Err(Error::Protocol {
                    feature_index,
                    function,
                    code: rep[5],
                });
            }
            if rep[2] == feature_index && rep[3] == fn_sw {
                let mut out = [0u8; 16];
                let len = (n - 4).min(16);
                out[..len].copy_from_slice(&rep[4..4 + len]);
                return Ok(out);
            }
        }
        Err(Error::Timeout {
            feature_index,
            function,
        })
    }

    /// Feature index for a feature id, via Root.getFeature (0x0000 fn 0).
    pub fn feature_index(&mut self, feature_id: u16) -> Result<u8> {
        if let Some(&i) = self.feature_cache.get(&feature_id) {
            return Ok(i);
        }
        let r = self.request(0x00, 0, &feature_id.to_be_bytes())?;
        if r[0] == 0 {
            return Err(Error::FeatureNotSupported(feature_id));
        }
        self.feature_cache.insert(feature_id, r[0]);
        Ok(r[0])
    }

    pub fn has_feature(&mut self, feature_id: u16) -> bool {
        self.feature_index(feature_id).is_ok()
    }

    /// Call a function on a feature by id.
    pub fn call(&mut self, feature_id: u16, function: u8, params: &[u8]) -> Result<[u8; 16]> {
        let idx = self.feature_index(feature_id)?;
        self.request(idx, function, params)
    }

    /// Like [`Device::call`] but never resends: for non-idempotent requests
    /// such as streamed memory writes.
    pub fn call_once(&mut self, feature_id: u16, function: u8, params: &[u8]) -> Result<[u8; 16]> {
        let idx = self.feature_index(feature_id)?;
        self.request_once(idx, function, params)
    }

    /// Root.getProtocolVersion (fn 1). Returns (major, minor).
    pub fn protocol_version(&mut self) -> Result<(u8, u8)> {
        let r = self.request(0x00, 1, &[0, 0, 0x5A])?;
        Ok((r[0], r[1]))
    }

    /// Whole feature table via IFeatureSet (0x0001).
    pub fn list_features(&mut self) -> Result<Vec<FeatureEntry>> {
        let count = self.call(features::FEATURE_SET, 0, &[])?[0];
        let mut out = vec![FeatureEntry {
            index: 0,
            id: features::ROOT,
            name: features::name(features::ROOT),
            flags: 0,
            version: 0,
        }];
        for i in 1..=count {
            let r = self.call(features::FEATURE_SET, 1, &[i])?;
            let id = u16::from_be_bytes([r[0], r[1]]);
            self.feature_cache.insert(id, i);
            out.push(FeatureEntry {
                index: i,
                id,
                name: features::name(id),
                flags: r[2],
                version: r[3],
            });
        }
        Ok(out)
    }

    /// Device name via 0x0005.
    pub fn device_name(&mut self) -> Result<String> {
        let len = self.call(features::DEVICE_NAME, 0, &[])?[0] as usize;
        let mut name = Vec::with_capacity(len);
        while name.len() < len {
            let r = self.call(features::DEVICE_NAME, 1, &[name.len() as u8])?;
            let take = (len - name.len()).min(16);
            name.extend_from_slice(&r[..take]);
        }
        Ok(String::from_utf8_lossy(&name)
            .trim_end_matches('\0')
            .to_string())
    }

    /// Firmware entities via 0x0003, formatted like "MPM 12.01.B0010".
    pub fn firmware(&mut self) -> Result<Vec<String>> {
        let count = self.call(features::FIRMWARE_INFO, 0, &[])?[0];
        let mut out = Vec::new();
        for e in 0..count {
            let r = self.call(features::FIRMWARE_INFO, 1, &[e])?;
            let kind = match r[0] & 0x0F {
                0 => "firmware",
                1 => "bootloader",
                2 => "hardware",
                _ => "other",
            };
            let prefix = String::from_utf8_lossy(&r[1..4])
                .trim_end_matches('\0')
                .to_string();
            out.push(format!(
                "{kind}: {prefix} {:02x}.{:02x}.B{:04x}",
                r[4],
                r[5],
                u16::from_be_bytes([r[6], r[7]])
            ));
        }
        Ok(out)
    }

    /// Sensor DPI list via Adjustable DPI (0x2201 fn 1), parsed like Solaar's
    /// `produce_dpi_list()`: a value with the top three bits set is a step,
    /// followed by the upper bound of that range.
    pub fn dpi_list(&mut self) -> Result<Vec<u16>> {
        let mut bytes = Vec::new();
        for i in 0..=255u8 {
            let r = self.call(features::ADJUSTABLE_DPI, 1, &[0, 0, i])?;
            bytes.extend_from_slice(&r[1..]);
            if bytes.ends_with(&[0, 0]) || bytes.len() > 512 {
                break;
            }
        }
        Ok(parse_dpi_list(&bytes))
    }

    /// Report rate list via 0x8060 fn 0: bit n set means (n+1) ms is supported.
    pub fn report_rates_ms(&mut self) -> Result<Vec<u8>> {
        let flags = self.call(features::REPORT_RATE, 0, &[])?[0];
        Ok((0..8)
            .filter(|i| flags & (1 << i) != 0)
            .map(|i| i + 1)
            .collect())
    }

    /// LED zones via Color LED Effects (0x8070), as Solaar's `LEDEffectsInfo`:
    /// fn 1 getZoneInfo(zone) -> location, effect count; fn 2
    /// getZoneEffectInfo(zone, i) -> effect id.
    pub fn led_zones(&mut self) -> Result<Vec<LedZone>> {
        let count = self.call(features::COLOR_LED_EFFECTS, 0, &[])?[0];
        let mut out = Vec::new();
        for z in 0..count {
            let r = self.call(features::COLOR_LED_EFFECTS, 1, &[z, 0xFF, 0])?;
            let location = u16::from_be_bytes([r[1], r[2]]);
            let mut effects = Vec::new();
            for e in 0..r[3] {
                let er = self.call(features::COLOR_LED_EFFECTS, 2, &[z, e, 0])?;
                effects.push(u16::from_be_bytes([er[2], er[3]]));
            }
            out.push(LedZone {
                index: z,
                location,
                effects,
            });
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LedZone {
    pub index: u8,
    /// Solaar `LEDZoneLocations`: 1 = primary, 2 = logo, ...
    pub location: u16,
    /// Effect ids (0 off, 1 static, 3 cycle, 0x0a breathing, ...).
    pub effects: Vec<u16>,
}

impl LedZone {
    pub fn location_name(&self) -> &'static str {
        match self.location {
            1 => "primary",
            2 => "logo",
            3 => "left side",
            4 => "right side",
            5 => "combined",
            _ => "unknown",
        }
    }
}

pub fn parse_dpi_list(bytes: &[u8]) -> Vec<u16> {
    let mut list: Vec<u16> = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let val = u16::from_be_bytes([bytes[i], bytes[i + 1]]);
        if val == 0 {
            break;
        }
        if val >> 13 == 0b111 {
            let step = val & 0x1FFF;
            if i + 3 >= bytes.len() || step == 0 {
                break;
            }
            let last = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]);
            let mut v = list.last().copied().unwrap_or(0);
            while v.saturating_add(step) <= last && v.checked_add(step).is_some() {
                v += step;
                list.push(v);
            }
            i += 4;
        } else {
            list.push(val);
            i += 2;
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_list_ranges() {
        // 100..=500 step 100, then 800
        let b = [0x00, 0x64, 0xE0, 0x64, 0x01, 0xF4, 0x03, 0x20, 0, 0];
        assert_eq!(parse_dpi_list(&b), vec![100, 200, 300, 400, 500, 800]);
    }
}
