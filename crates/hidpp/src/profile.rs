//! Profile sector and directory sector: decode, edit, encode.
//!
//! Layout of a profile sector (libratbag `union hidpp20_internal_profile`,
//! Solaar `OnboardProfile.from_bytes`), offsets in bytes:
//!
//! | off | len | field                                              |
//! |-----|-----|----------------------------------------------------|
//! | 0   | 1   | report interval in ms (1 = 1000 Hz)                |
//! | 1   | 1   | default DPI index                                  |
//! | 2   | 1   | shift ("sniper") DPI index                         |
//! | 3   | 10  | 5 × DPI, little endian, 0 = unused                 |
//! | 13  | 3   | profile colour (RGB)                               |
//! | 16  | 1   | power mode                                         |
//! | 17  | 1   | angle snapping                                     |
//! | 18  | 14  | write counter / reserved / power timeouts          |
//! | 32  | 64  | 16 × 4-byte button bindings                        |
//! | 96  | 64  | 16 × 4-byte G-shift button bindings                |
//! | 160 | 48  | name (UTF-16LE per Solaar, all 0xFF = no name)     |
//! | 208 | 44  | 4 × 11-byte LED effects: zone 0, zone 1, alt 0, 1  |
//! | n-2 | 2   | CRC-CCITT, big endian                              |
//!
//! Encoding is a patch over the original sector bytes: fields whose decoded
//! value did not change keep their exact original bytes, so unknown and
//! reserved bytes are always preserved and an unmodified profile encodes to
//! exactly what was read.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::crc;
use crate::error::{Error, Result};
use crate::keys;
use crate::onboard::OnboardInfo;

pub const OFF_REPORT_RATE: usize = 0;
pub const OFF_DEFAULT_DPI: usize = 1;
pub const OFF_SHIFT_DPI: usize = 2;
pub const OFF_DPI: usize = 3;
pub const DPI_SLOTS: usize = 5;
pub const OFF_COLOR: usize = 13;
pub const OFF_POWER_MODE: usize = 16;
pub const OFF_ANGLE_SNAPPING: usize = 17;
pub const OFF_BUTTONS: usize = 32;
pub const OFF_GSHIFT_BUTTONS: usize = 96;
pub const MAX_BUTTONS: usize = 16;
pub const OFF_NAME: usize = 160;
pub const NAME_BYTES: usize = 48;
pub const OFF_LEDS: usize = 208;
pub const LED_BYTES: usize = 11;
pub const LED_ZONES: usize = 2;
/// Bytes a profile needs before the CRC.
const MIN_PROFILE_SIZE: usize = OFF_LEDS + 4 * LED_BYTES + 2;

// ---------------------------------------------------------------------------
// Button bindings
// ---------------------------------------------------------------------------

/// Function codes of a 0x90 ("special") binding. Codes 0x01-0x0b follow
/// libratbag's `hidpp20_profiles_specials[]`, 0x0c-0x11 Solaar's `ButtonFunctions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Special {
    TiltLeft,
    TiltRight,
    DpiUp,
    DpiDown,
    DpiCycle,
    DpiDefault,
    DpiShift,
    ProfileNext,
    ProfilePrevious,
    ProfileCycle,
    GShift,
    BatteryStatus,
    ProfileSelect,
    ModeSwitch,
    HostButton,
    ScrollDown,
    ScrollUp,
    Other(u8),
}

impl Special {
    pub const EDITABLE: &'static [Special] = &[
        Special::DpiUp,
        Special::DpiDown,
        Special::DpiCycle,
        Special::DpiDefault,
        Special::DpiShift,
        Special::ProfileNext,
        Special::ProfilePrevious,
        Special::ProfileCycle,
        Special::GShift,
        Special::TiltLeft,
        Special::TiltRight,
        Special::ScrollUp,
        Special::ScrollDown,
    ];

    pub fn code(self) -> u8 {
        match self {
            Special::TiltLeft => 0x01,
            Special::TiltRight => 0x02,
            Special::DpiUp => 0x03,
            Special::DpiDown => 0x04,
            Special::DpiCycle => 0x05,
            Special::DpiDefault => 0x06,
            Special::DpiShift => 0x07,
            Special::ProfileNext => 0x08,
            Special::ProfilePrevious => 0x09,
            Special::ProfileCycle => 0x0a,
            Special::GShift => 0x0b,
            Special::BatteryStatus => 0x0c,
            Special::ProfileSelect => 0x0d,
            Special::ModeSwitch => 0x0e,
            Special::HostButton => 0x0f,
            Special::ScrollDown => 0x10,
            Special::ScrollUp => 0x11,
            Special::Other(c) => c,
        }
    }

    pub fn from_code(c: u8) -> Self {
        match c {
            0x01 => Special::TiltLeft,
            0x02 => Special::TiltRight,
            0x03 => Special::DpiUp,
            0x04 => Special::DpiDown,
            0x05 => Special::DpiCycle,
            0x06 => Special::DpiDefault,
            0x07 => Special::DpiShift,
            0x08 => Special::ProfileNext,
            0x09 => Special::ProfilePrevious,
            0x0a => Special::ProfileCycle,
            0x0b => Special::GShift,
            0x0c => Special::BatteryStatus,
            0x0d => Special::ProfileSelect,
            0x0e => Special::ModeSwitch,
            0x0f => Special::HostButton,
            0x10 => Special::ScrollDown,
            0x11 => Special::ScrollUp,
            c => Special::Other(c),
        }
    }

    pub fn id(self) -> String {
        match self {
            Special::TiltLeft => "tilt_left".into(),
            Special::TiltRight => "tilt_right".into(),
            Special::DpiUp => "dpi_up".into(),
            Special::DpiDown => "dpi_down".into(),
            Special::DpiCycle => "dpi_cycle".into(),
            Special::DpiDefault => "dpi_default".into(),
            Special::DpiShift => "dpi_shift".into(),
            Special::ProfileNext => "profile_next".into(),
            Special::ProfilePrevious => "profile_previous".into(),
            Special::ProfileCycle => "profile_cycle".into(),
            Special::GShift => "g_shift".into(),
            Special::BatteryStatus => "battery_status".into(),
            Special::ProfileSelect => "profile_select".into(),
            Special::ModeSwitch => "mode_switch".into(),
            Special::HostButton => "host_button".into(),
            Special::ScrollDown => "scroll_down".into(),
            Special::ScrollUp => "scroll_up".into(),
            Special::Other(c) => format!("0x{c:02x}"),
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        if let Some(h) = s.strip_prefix("0x") {
            return u8::from_str_radix(h, 16).ok().map(Special::from_code);
        }
        (1u8..=0x11).map(Special::from_code).find(|sp| sp.id() == s)
    }

    pub fn label(self) -> String {
        match self {
            Special::TiltLeft => "Wheel tilt left".into(),
            Special::TiltRight => "Wheel tilt right".into(),
            Special::DpiUp => "DPI up".into(),
            Special::DpiDown => "DPI down".into(),
            Special::DpiCycle => "DPI cycle".into(),
            Special::DpiDefault => "DPI default".into(),
            Special::DpiShift => "DPI shift (hold)".into(),
            Special::ProfileNext => "Next profile".into(),
            Special::ProfilePrevious => "Previous profile".into(),
            Special::ProfileCycle => "Cycle profiles".into(),
            Special::GShift => "G-shift (hold)".into(),
            Special::BatteryStatus => "Battery status".into(),
            Special::ProfileSelect => "Profile select".into(),
            Special::ModeSwitch => "Mode switch".into(),
            Special::HostButton => "Host button".into(),
            Special::ScrollDown => "Scroll down".into(),
            Special::ScrollUp => "Scroll up".into(),
            Special::Other(c) => format!("Function {c:#04x}"),
        }
    }
}

/// A keyboard shortcut: modifier bits plus one physical HID usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub modifiers: u8,
    pub key: u8,
}

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = keys::modifier_names(self.modifiers)
            .into_iter()
            .map(String::from)
            .collect();
        if self.key != 0 {
            parts.push(keys::key_label(self.key));
        }
        if parts.is_empty() {
            return f.write_str("(none)");
        }
        f.write_str(&parts.join(" + "))
    }
}

impl std::str::FromStr for KeyCombo {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let mut modifiers = 0u8;
        let mut key = 0u8;
        // Canonical form is "Ctrl + C"; also accept the compact "Ctrl+C".
        let tokens: Vec<&str> = if s.contains(" + ") {
            s.split(" + ").collect()
        } else {
            s.split('+').collect()
        };
        for t in tokens {
            let t = t.trim();
            if t.is_empty() {
                continue;
            }
            if let Some(m) = keys::modifier_from_name(t) {
                modifiers |= m;
            } else if let Some(k) = keys::key_from_name(t) {
                if key != 0 {
                    return Err(Error::Invalid(format!(
                        "more than one non-modifier key in {s:?}"
                    )));
                }
                key = k;
            } else {
                return Err(Error::Invalid(format!("unknown key {t:?} in {s:?}")));
            }
        }
        Ok(KeyCombo { modifiers, key })
    }
}

impl Serialize for KeyCombo {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for KeyCombo {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// One button slot's binding, decoded from its 4 bytes.
///
/// Encoding (Solaar `Button.from_bytes/to_bytes`, libratbag `hidpp20_buttons_to_cpu`):
/// * `80 01 HH LL` – send mouse button bitmask (BE16, bit 0 = left)
/// * `80 02 MM KK` – send keyboard usage KK with modifier bits MM
/// * `80 03 HH LL` – send consumer usage (BE16)
/// * `80 00 xx xx` – no action
/// * `90 FF xx DD` – function FF (DPI/profile/tilt/…), DD data
/// * `0S SS AA AA` / `1S SS AA AA` – run/stop macro at sector S, address A
/// * `FF FF FF FF` – slot not assigned
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    /// HID mouse button mask (1 = left, 2 = right, 4 = middle, 8 = back, 16 = forward …).
    MouseButtons(u16),
    Key(KeyCombo),
    Consumer(u16),
    Disabled,
    /// `raw2`/`raw3` keep the trailing bytes (factory data uses `ff ff`).
    Function {
        function: Special,
        raw2: u8,
        data: u8,
    },
    Macro([u8; 4]),
    Unassigned,
    Unknown([u8; 4]),
}

impl Binding {
    pub fn function(f: Special) -> Self {
        Binding::Function {
            function: f,
            raw2: 0xff,
            data: 0xff,
        }
    }

    pub fn mouse_button(n: u8) -> Self {
        Binding::MouseButtons(1u16 << (n.clamp(1, 16) - 1))
    }

    pub fn decode(b: [u8; 4]) -> Self {
        match b[0] {
            0x80 => match b[1] {
                0x00 => Binding::Disabled,
                0x01 => Binding::MouseButtons(u16::from_be_bytes([b[2], b[3]])),
                0x02 => Binding::Key(KeyCombo {
                    modifiers: b[2],
                    key: b[3],
                }),
                0x03 => Binding::Consumer(u16::from_be_bytes([b[2], b[3]])),
                _ => Binding::Unknown(b),
            },
            0x90 => Binding::Function {
                function: Special::from_code(b[1]),
                raw2: b[2],
                data: b[3],
            },
            0xFF if b == [0xFF; 4] => Binding::Unassigned,
            x if x >> 4 <= 1 => Binding::Macro(b),
            _ => Binding::Unknown(b),
        }
    }

    pub fn encode(&self) -> [u8; 4] {
        match *self {
            Binding::MouseButtons(m) => {
                let m = m.to_be_bytes();
                [0x80, 0x01, m[0], m[1]]
            }
            Binding::Key(k) => [0x80, 0x02, k.modifiers, k.key],
            Binding::Consumer(u) => {
                let u = u.to_be_bytes();
                [0x80, 0x03, u[0], u[1]]
            }
            // Solaar writes NO_ACTION as 80 00 ff ff.
            Binding::Disabled => [0x80, 0x00, 0xFF, 0xFF],
            Binding::Function {
                function,
                raw2,
                data,
            } => [0x90, function.code(), raw2, data],
            Binding::Macro(b) | Binding::Unknown(b) => b,
            Binding::Unassigned => [0xFF; 4],
        }
    }

    /// Whether the GUI can edit this binding type (macros/unknowns are preserved only).
    pub fn is_editable(&self) -> bool {
        !matches!(self, Binding::Macro(_) | Binding::Unknown(_))
    }
}

pub fn mouse_button_name(mask: u16) -> String {
    let names = [
        "Left click",
        "Right click",
        "Middle click",
        "Back",
        "Forward",
    ];
    let bits: Vec<String> = (0..16)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| {
            names
                .get(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Mouse button {}", i + 1))
        })
        .collect();
    if bits.is_empty() {
        "Mouse button (none)".into()
    } else {
        bits.join(" + ")
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Binding::MouseButtons(m) => f.write_str(&mouse_button_name(*m)),
            Binding::Key(k) => write!(f, "{k}"),
            Binding::Consumer(u) => f.write_str(&keys::consumer_label(*u)),
            Binding::Disabled => f.write_str("Disabled"),
            Binding::Function { function, .. } => f.write_str(&function.label()),
            Binding::Macro(b) => {
                let sector = (u16::from(b[0] & 0x0F) << 8) | u16::from(b[1]);
                let addr = u16::from_be_bytes([b[2], b[3]]);
                let what = if b[0] >> 4 == 1 {
                    "Stop macro"
                } else {
                    "Macro"
                };
                write!(f, "{what} (sector {sector}, offset {addr})")
            }
            Binding::Unassigned => f.write_str("Unassigned"),
            Binding::Unknown(b) => write!(
                f,
                "Unknown {:02x} {:02x} {:02x} {:02x}",
                b[0], b[1], b[2], b[3]
            ),
        }
    }
}

/// JSON form: `{"type":"key","keys":"Ctrl + C"}` etc.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BindingJson {
    Mouse {
        button: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mask: Option<u16>,
    },
    Key {
        keys: KeyCombo,
    },
    Consumer {
        key: String,
    },
    Disabled,
    Function {
        function: String,
        #[serde(default = "ff", skip_serializing_if = "is_ff")]
        data: u8,
        #[serde(default = "ff", skip_serializing_if = "is_ff")]
        reserved: u8,
    },
    Macro {
        raw: String,
    },
    Unassigned,
    Unknown {
        raw: String,
    },
}

fn ff() -> u8 {
    0xff
}
fn is_ff(v: &u8) -> bool {
    *v == 0xff
}
fn raw_hex(b: &[u8; 4]) -> String {
    format!("{:02x} {:02x} {:02x} {:02x}", b[0], b[1], b[2], b[3])
}
fn parse_raw(s: &str) -> std::result::Result<[u8; 4], String> {
    let v = crate::backup::from_hex(s).map_err(|e| e.to_string())?;
    v.try_into()
        .map_err(|_| "raw binding must be 4 bytes".to_string())
}

impl Serialize for Binding {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let j = match *self {
            Binding::MouseButtons(m) => {
                let single = m.count_ones() == 1;
                BindingJson::Mouse {
                    button: if m == 0 {
                        0
                    } else {
                        m.trailing_zeros() as u8 + 1
                    },
                    mask: (!single).then_some(m),
                }
            }
            Binding::Key(k) => BindingJson::Key { keys: k },
            Binding::Consumer(u) => BindingJson::Consumer {
                key: keys::consumer_label(u),
            },
            Binding::Disabled => BindingJson::Disabled,
            Binding::Function {
                function,
                raw2,
                data,
            } => BindingJson::Function {
                function: function.id(),
                data,
                reserved: raw2,
            },
            Binding::Macro(b) => BindingJson::Macro { raw: raw_hex(&b) },
            Binding::Unassigned => BindingJson::Unassigned,
            Binding::Unknown(b) => BindingJson::Unknown { raw: raw_hex(&b) },
        };
        j.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        use serde::de::Error as _;
        Ok(match BindingJson::deserialize(d)? {
            BindingJson::Mouse { button, mask } => match mask {
                Some(m) => Binding::MouseButtons(m),
                None if (1..=16).contains(&button) => Binding::mouse_button(button),
                None => {
                    return Err(D::Error::custom(format!(
                        "mouse button {button} out of range 1..=16"
                    )))
                }
            },
            BindingJson::Key { keys } => Binding::Key(keys),
            BindingJson::Consumer { key } => Binding::Consumer(
                keys::consumer_from_name(&key)
                    .ok_or_else(|| D::Error::custom(format!("unknown consumer key {key:?}")))?,
            ),
            BindingJson::Disabled => Binding::Disabled,
            BindingJson::Function {
                function,
                data,
                reserved,
            } => Binding::Function {
                function: Special::from_id(&function)
                    .ok_or_else(|| D::Error::custom(format!("unknown function {function:?}")))?,
                raw2: reserved,
                data,
            },
            BindingJson::Macro { raw } => {
                Binding::Macro(parse_raw(&raw).map_err(D::Error::custom)?)
            }
            BindingJson::Unassigned => Binding::Unassigned,
            BindingJson::Unknown { raw } => {
                Binding::Unknown(parse_raw(&raw).map_err(D::Error::custom)?)
            }
        })
    }
}

// ---------------------------------------------------------------------------
// LEDs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let h = s.trim_start_matches('#');
        let v = u32::from_str_radix(h, 16).map_err(serde::de::Error::custom)?;
        if h.len() != 6 {
            return Err(serde::de::Error::custom("colour must be #rrggbb"));
        }
        Ok(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}

/// One 11-byte LED effect (libratbag `struct hidpp20_internal_led`; mode ids
/// from `enum hidpp20_led_mode`, parameter offsets agree with Solaar `LEDEffects`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Led {
    /// mode 0x00
    Off,
    /// mode 0x01: colour at 1..4, "effect"/ramp byte at 4.
    Static {
        color: Rgb,
        #[serde(default)]
        ramp: u8,
    },
    /// mode 0x03: period (BE16) at 6..8, intensity at 8 (0 = 100 %).
    Cycle { period_ms: u16, brightness: u8 },
    /// mode 0x0a: colour 1..4, period (BE16) 4..6, waveform 6, intensity 7.
    Breathing {
        color: Rgb,
        period_ms: u16,
        #[serde(default)]
        waveform: u8,
        brightness: u8,
    },
    /// Anything else, kept byte for byte.
    Raw { raw: String },
}

impl Led {
    pub fn decode(b: &[u8]) -> Self {
        let rgb = |o: usize| Rgb(b[o], b[o + 1], b[o + 2]);
        let pct = |v: u8| if v == 0 { 100 } else { v };
        match b[0] {
            0x00 => Led::Off,
            0x01 => Led::Static {
                color: rgb(1),
                ramp: b[4],
            },
            0x03 => Led::Cycle {
                period_ms: u16::from_be_bytes([b[6], b[7]]),
                brightness: pct(b[8]),
            },
            0x0a => Led::Breathing {
                color: rgb(1),
                period_ms: u16::from_be_bytes([b[4], b[5]]),
                waveform: b[6],
                brightness: pct(b[7]),
            },
            _ => Led::Raw {
                raw: crate::backup::to_hex(&b[..LED_BYTES]),
            },
        }
    }

    /// Encode to 11 bytes. Like libratbag's `hidpp20_onboard_profiles_write_led`,
    /// the effect bytes start zeroed and brightness 100 is written as 0.
    pub fn encode(&self) -> Result<[u8; LED_BYTES]> {
        let mut o = [0u8; LED_BYTES];
        let pct = |v: u8| if v >= 100 { 0 } else { v };
        match self {
            Led::Off => {}
            Led::Static { color, ramp } => {
                o[0] = 0x01;
                o[1..4].copy_from_slice(&[color.0, color.1, color.2]);
                o[4] = *ramp;
            }
            Led::Cycle {
                period_ms,
                brightness,
            } => {
                o[0] = 0x03;
                o[6..8].copy_from_slice(&period_ms.to_be_bytes());
                o[8] = pct(*brightness);
            }
            Led::Breathing {
                color,
                period_ms,
                waveform,
                brightness,
            } => {
                o[0] = 0x0a;
                o[1..4].copy_from_slice(&[color.0, color.1, color.2]);
                o[4..6].copy_from_slice(&period_ms.to_be_bytes());
                o[6] = *waveform;
                o[7] = pct(*brightness);
            }
            Led::Raw { raw } => {
                let v = crate::backup::from_hex(raw)?;
                if v.len() != LED_BYTES {
                    return Err(Error::Invalid("raw LED effect must be 11 bytes".into()));
                }
                o.copy_from_slice(&v);
            }
        }
        Ok(o)
    }

    pub fn label(&self) -> String {
        match self {
            Led::Off => "Off".into(),
            Led::Static { color, .. } => format!("Static {color}"),
            Led::Cycle {
                period_ms,
                brightness,
            } => format!("Colour cycle, {period_ms} ms, {brightness}%"),
            Led::Breathing {
                color,
                period_ms,
                brightness,
                ..
            } => format!("Breathing {color}, {period_ms} ms, {brightness}%"),
            Led::Raw { raw } => format!("Other effect ({raw})"),
        }
    }
}

// ---------------------------------------------------------------------------
// Profile
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Report interval in milliseconds (1 = 1000 Hz, 2 = 500 Hz, …).
    pub report_interval_ms: u8,
    pub default_dpi_index: u8,
    pub shift_dpi_index: u8,
    /// Five DPI slots; 0 means the slot is unused.
    pub dpi: [u16; DPI_SLOTS],
    pub color: Rgb,
    pub power_mode: u8,
    pub angle_snapping: u8,
    pub buttons: Vec<Binding>,
    #[serde(default)]
    pub gshift_buttons: Vec<Binding>,
    #[serde(default)]
    pub name: String,
    pub leds: Vec<Led>,
    /// The sector bytes this profile was decoded from (base for encoding).
    #[serde(skip)]
    pub raw: Vec<u8>,
}

/// Settings equality: the raw base bytes are not compared.
impl PartialEq for Profile {
    fn eq(&self, o: &Self) -> bool {
        self.report_interval_ms == o.report_interval_ms
            && self.default_dpi_index == o.default_dpi_index
            && self.shift_dpi_index == o.shift_dpi_index
            && self.dpi == o.dpi
            && self.color == o.color
            && self.power_mode == o.power_mode
            && self.angle_snapping == o.angle_snapping
            && self.buttons == o.buttons
            && self.gshift_buttons == o.gshift_buttons
            && self.name == o.name
            && self.leds == o.leds
    }
}

impl Profile {
    pub fn decode(raw: &[u8], info: &OnboardInfo) -> Result<Self> {
        if raw.len() != usize::from(info.sector_size) || raw.len() < MIN_PROFILE_SIZE {
            return Err(Error::Invalid(format!(
                "profile sector is {} bytes",
                raw.len()
            )));
        }
        let nb = info.buttons();
        let slot = |base: usize, i: usize| {
            let o = base + i * 4;
            Binding::decode([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]])
        };
        let mut dpi = [0u16; DPI_SLOTS];
        for (i, d) in dpi.iter_mut().enumerate() {
            *d = u16::from_le_bytes([raw[OFF_DPI + 2 * i], raw[OFF_DPI + 2 * i + 1]]);
        }
        Ok(Profile {
            report_interval_ms: raw[OFF_REPORT_RATE],
            default_dpi_index: raw[OFF_DEFAULT_DPI],
            shift_dpi_index: raw[OFF_SHIFT_DPI],
            dpi,
            color: Rgb(raw[OFF_COLOR], raw[OFF_COLOR + 1], raw[OFF_COLOR + 2]),
            power_mode: raw[OFF_POWER_MODE],
            angle_snapping: raw[OFF_ANGLE_SNAPPING],
            buttons: (0..nb).map(|i| slot(OFF_BUTTONS, i)).collect(),
            gshift_buttons: if info.has_g_shift() {
                (0..nb).map(|i| slot(OFF_GSHIFT_BUTTONS, i)).collect()
            } else {
                Vec::new()
            },
            name: decode_name(&raw[OFF_NAME..OFF_NAME + NAME_BYTES]),
            leds: (0..LED_ZONES)
                .map(|z| Led::decode(&raw[OFF_LEDS + z * LED_BYTES..]))
                .collect(),
            raw: raw.to_vec(),
        })
    }

    /// Encode onto `self.raw`, touching only fields whose value changed, and seal the CRC.
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.raw.len() < MIN_PROFILE_SIZE {
            return Err(Error::Invalid(
                "profile has no base sector to encode onto".into(),
            ));
        }
        self.validate()?;
        let mut out = self.raw.clone();
        out[OFF_REPORT_RATE] = self.report_interval_ms;
        out[OFF_DEFAULT_DPI] = self.default_dpi_index;
        out[OFF_SHIFT_DPI] = self.shift_dpi_index;
        for (i, d) in self.dpi.iter().enumerate() {
            out[OFF_DPI + 2 * i..OFF_DPI + 2 * i + 2].copy_from_slice(&d.to_le_bytes());
        }
        out[OFF_COLOR..OFF_COLOR + 3].copy_from_slice(&[self.color.0, self.color.1, self.color.2]);
        out[OFF_POWER_MODE] = self.power_mode;
        out[OFF_ANGLE_SNAPPING] = self.angle_snapping;

        let patch_slots = |out: &mut [u8], base: usize, list: &[Binding]| {
            for (i, b) in list.iter().enumerate().take(MAX_BUTTONS) {
                let o = base + i * 4;
                let old = Binding::decode([out[o], out[o + 1], out[o + 2], out[o + 3]]);
                if old != *b {
                    out[o..o + 4].copy_from_slice(&b.encode());
                }
            }
        };
        patch_slots(&mut out, OFF_BUTTONS, &self.buttons);
        patch_slots(&mut out, OFF_GSHIFT_BUTTONS, &self.gshift_buttons);

        let name_region = &mut out[OFF_NAME..OFF_NAME + NAME_BYTES];
        if decode_name(name_region) != self.name {
            name_region.copy_from_slice(&encode_name(&self.name)?);
        }

        for (z, led) in self.leds.iter().enumerate().take(LED_ZONES) {
            let o = OFF_LEDS + z * LED_BYTES;
            if Led::decode(&out[o..o + LED_BYTES]) != *led {
                out[o..o + LED_BYTES].copy_from_slice(&led.encode()?);
            }
        }

        crc::seal(&mut out);
        Ok(out)
    }

    pub fn validate(&self) -> Result<()> {
        if self.report_interval_ms == 0 || self.report_interval_ms > 8 {
            return Err(Error::Invalid(format!(
                "report interval {} ms out of range 1..=8",
                self.report_interval_ms
            )));
        }
        let used = self.dpi.iter().take_while(|&&d| d != 0).count();
        if used == 0 {
            return Err(Error::Invalid("at least one DPI level is required".into()));
        }
        if self.dpi[used..].iter().any(|&d| d != 0) {
            return Err(Error::Invalid(
                "unused DPI levels must come after the used ones".into(),
            ));
        }
        if usize::from(self.default_dpi_index) >= used {
            return Err(Error::Invalid(format!(
                "default DPI index {} but only {used} levels",
                self.default_dpi_index
            )));
        }
        if usize::from(self.shift_dpi_index) >= used {
            return Err(Error::Invalid(format!(
                "shift DPI index {} but only {used} levels",
                self.shift_dpi_index
            )));
        }
        if self.buttons.len() > MAX_BUTTONS || self.gshift_buttons.len() > MAX_BUTTONS {
            return Err(Error::Invalid("too many button slots".into()));
        }
        if self
            .buttons
            .iter()
            .chain(&self.gshift_buttons)
            .any(|b| matches!(b, Binding::Key(k) if k.modifiers == 0 && k.key == 0))
        {
            return Err(Error::Invalid("a keyboard binding has no key".into()));
        }
        if self.name.encode_utf16().count() > NAME_BYTES / 2 {
            return Err(Error::Invalid(
                "profile name longer than 24 characters".into(),
            ));
        }
        Ok(())
    }

    pub fn report_rate_hz(&self) -> u32 {
        1000 / u32::from(self.report_interval_ms.max(1))
    }

    pub fn dpi_levels(&self) -> Vec<u16> {
        self.dpi.iter().copied().take_while(|&d| d != 0).collect()
    }

    /// Replace the decoded fields with those of `other`, keeping our raw base.
    pub fn apply_from(&mut self, other: &Profile) {
        let raw = std::mem::take(&mut self.raw);
        *self = Profile {
            raw,
            ..other.clone()
        };
    }
}

fn decode_name(b: &[u8]) -> String {
    if b.iter().all(|&x| x == 0xFF) {
        return String::new();
    }
    let units: Vec<u16> = b
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0 && u != 0xFFFF)
        .collect();
    String::from_utf16_lossy(&units)
}

fn encode_name(name: &str) -> Result<[u8; NAME_BYTES]> {
    let mut out = [0xFFu8; NAME_BYTES];
    if name.is_empty() {
        return Ok(out);
    }
    let units: Vec<u16> = name.encode_utf16().collect();
    if units.len() > NAME_BYTES / 2 {
        return Err(Error::Invalid(
            "profile name longer than 24 characters".into(),
        ));
    }
    out.fill(0);
    for (i, u) in units.iter().enumerate() {
        out[2 * i..2 * i + 2].copy_from_slice(&u.to_le_bytes());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Directory (sector 0x0000)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    pub sector: u16,
    pub enabled: bool,
}

/// Profile directory: 4-byte entries `sector(BE16) enabled 00`, terminated by
/// `FF FF`, as written by libratbag's `hidpp20_onboard_profiles_write_dict()`
/// and Solaar's `OnboardProfiles.to_bytes()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directory {
    pub entries: Vec<DirEntry>,
}

impl Directory {
    pub fn decode(raw: &[u8], max_entries: usize) -> Self {
        let mut entries = Vec::new();
        for c in raw.as_chunks::<4>().0.iter().take(max_entries) {
            let sector = u16::from_be_bytes([c[0], c[1]]);
            if sector == 0xFFFF {
                break;
            }
            entries.push(DirEntry {
                sector,
                enabled: c[2] != 0,
            });
        }
        Directory { entries }
    }

    pub fn encode(&self, size: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(size);
        for e in &self.entries {
            out.extend_from_slice(&e.sector.to_be_bytes());
            out.push(u8::from(e.enabled));
            out.push(0x00);
        }
        out.extend_from_slice(&[0xFF, 0xFF, 0x00, 0x00]);
        out.resize(size, 0xFF);
        crc::seal(&mut out);
        out
    }

    pub fn is_enabled(&self, sector: u16) -> bool {
        self.entries.iter().any(|e| e.sector == sector && e.enabled)
    }
}
