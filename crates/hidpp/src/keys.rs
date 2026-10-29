//! HID usage names and conversions.
//!
//! Keyboard bindings store *physical* HID keyboard usages (page 0x07). The
//! names here are the US-layout labels of those physical keys; a GUI should
//! translate them to the user's layout for display (usage → scancode → OS name).

/// Modifier bits as stored in the keyboard binding (byte 2). Same layout as the
/// HID boot keyboard modifier byte; Solaar `special_keys.KEYMOD`.
pub mod modifier {
    pub const LCTRL: u8 = 0x01;
    pub const LSHIFT: u8 = 0x02;
    pub const LALT: u8 = 0x04;
    pub const LGUI: u8 = 0x08;
    pub const RCTRL: u8 = 0x10;
    pub const RSHIFT: u8 = 0x20;
    pub const RALT: u8 = 0x40;
    pub const RGUI: u8 = 0x80;
}

const MODIFIER_NAMES: [(u8, &str); 8] = [
    (modifier::LCTRL, "Ctrl"),
    (modifier::LSHIFT, "Shift"),
    (modifier::LALT, "Alt"),
    (modifier::LGUI, "Win"),
    (modifier::RCTRL, "Right Ctrl"),
    (modifier::RSHIFT, "Right Shift"),
    (modifier::RALT, "AltGr"),
    (modifier::RGUI, "Right Win"),
];

pub fn modifier_names(bits: u8) -> Vec<&'static str> {
    MODIFIER_NAMES
        .iter()
        .filter(|(b, _)| bits & b != 0)
        .map(|(_, n)| *n)
        .collect()
}

pub fn modifier_from_name(name: &str) -> Option<u8> {
    let n = name.trim().to_ascii_lowercase();
    let bit = match n.as_str() {
        "ctrl" | "control" | "lctrl" | "left ctrl" => modifier::LCTRL,
        "shift" | "lshift" | "left shift" => modifier::LSHIFT,
        "alt" | "lalt" | "left alt" => modifier::LALT,
        "win" | "gui" | "meta" | "super" | "lwin" | "left win" => modifier::LGUI,
        "right ctrl" | "rctrl" => modifier::RCTRL,
        "right shift" | "rshift" => modifier::RSHIFT,
        "altgr" | "right alt" | "ralt" => modifier::RALT,
        "right win" | "rwin" => modifier::RGUI,
        _ => return None,
    };
    Some(bit)
}

/// Modifier key usages (0xE0..=0xE7) map onto the modifier bits in order.
pub fn modifier_bit_for_usage(usage: u8) -> Option<u8> {
    (0xE0..=0xE7).contains(&usage).then(|| 1 << (usage - 0xE0))
}

/// US-label name of a keyboard usage (page 0x07).
pub fn key_name(usage: u8) -> Option<&'static str> {
    Some(match usage {
        0x04..=0x1D => LETTERS[usize::from(usage - 0x04)],
        0x1E..=0x27 => DIGITS[usize::from(usage - 0x1E)],
        0x28 => "Enter",
        0x29 => "Esc",
        0x2A => "Backspace",
        0x2B => "Tab",
        0x2C => "Space",
        0x2D => "-",
        0x2E => "=",
        0x2F => "[",
        0x30 => "]",
        0x31 => "\\",
        0x32 => "Non-US #",
        0x33 => ";",
        0x34 => "'",
        0x35 => "`",
        0x36 => ",",
        0x37 => ".",
        0x38 => "/",
        0x39 => "Caps Lock",
        0x3A..=0x45 => FKEYS[usize::from(usage - 0x3A)],
        0x46 => "Print Screen",
        0x47 => "Scroll Lock",
        0x48 => "Pause",
        0x49 => "Insert",
        0x4A => "Home",
        0x4B => "Page Up",
        0x4C => "Delete",
        0x4D => "End",
        0x4E => "Page Down",
        0x4F => "Right",
        0x50 => "Left",
        0x51 => "Down",
        0x52 => "Up",
        0x53 => "Num Lock",
        0x54 => "Num /",
        0x55 => "Num *",
        0x56 => "Num -",
        0x57 => "Num +",
        0x58 => "Num Enter",
        0x59..=0x62 => NUMPAD[usize::from(usage - 0x59)],
        0x63 => "Num .",
        0x64 => "Non-US \\",
        0x65 => "Menu",
        0x66 => "Power",
        0x67 => "Num =",
        0x68..=0x73 => FKEYS_HIGH[usize::from(usage - 0x68)],
        0x74 => "Execute",
        0x75 => "Help",
        0x76 => "Menu Key",
        0x77 => "Select",
        0x78 => "Stop",
        0x79 => "Again",
        0x7A => "Undo",
        0x7B => "Cut",
        0x7C => "Copy",
        0x7D => "Paste",
        0x7E => "Find",
        0x7F => "Mute",
        0x80 => "Volume Up",
        0x81 => "Volume Down",
        0x85 => "Num ,",
        0x87 => "Intl 1",
        0x88 => "Intl 2",
        0x89 => "Intl 3",
        0x8A => "Intl 4",
        0x8B => "Intl 5",
        0x90 => "Lang 1",
        0x91 => "Lang 2",
        0xE0 => "Left Ctrl",
        0xE1 => "Left Shift",
        0xE2 => "Left Alt",
        0xE3 => "Left Win",
        0xE4 => "Right Ctrl",
        0xE5 => "Right Shift",
        0xE6 => "Right Alt",
        0xE7 => "Right Win",
        _ => return None,
    })
}

const LETTERS: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];
const DIGITS: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
const FKEYS: [&str; 12] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
];
const FKEYS_HIGH: [&str; 12] = [
    "F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
];
const NUMPAD: [&str; 10] = [
    "Num 1", "Num 2", "Num 3", "Num 4", "Num 5", "Num 6", "Num 7", "Num 8", "Num 9", "Num 0",
];

pub fn key_label(usage: u8) -> String {
    key_name(usage)
        .map(str::to_string)
        .unwrap_or_else(|| format!("Key {usage:#04x}"))
}

pub fn key_from_name(name: &str) -> Option<u8> {
    let n = name.trim();
    if let Some(hex) = n.strip_prefix("Key 0x").or_else(|| n.strip_prefix("0x")) {
        return u8::from_str_radix(hex, 16).ok();
    }
    (0u8..=0xFF).find(|&u| key_name(u).is_some_and(|k| k.eq_ignore_ascii_case(n)))
}

/// All usages that have a name (for pickers).
pub fn named_keys() -> impl Iterator<Item = (u8, &'static str)> {
    (0u8..=0xFF).filter_map(|u| key_name(u).map(|n| (u, n)))
}

/// Windows/PS2 set-1 scancode of a keyboard usage, with bit 8 (0x100) set for
/// extended (E0-prefixed) keys. Lets a GUI ask the OS for the label of the
/// physical key on the active layout.
pub fn usage_to_scancode(usage: u8) -> Option<u16> {
    const E: u16 = 0x100;
    let s: u16 = match usage {
        0x04 => 0x1E, // A
        0x05 => 0x30,
        0x06 => 0x2E,
        0x07 => 0x20,
        0x08 => 0x12,
        0x09 => 0x21,
        0x0A => 0x22,
        0x0B => 0x23,
        0x0C => 0x17,
        0x0D => 0x24,
        0x0E => 0x25,
        0x0F => 0x26,
        0x10 => 0x32,
        0x11 => 0x31,
        0x12 => 0x18,
        0x13 => 0x19,
        0x14 => 0x10,
        0x15 => 0x13,
        0x16 => 0x1F,
        0x17 => 0x14,
        0x18 => 0x16,
        0x19 => 0x2F,
        0x1A => 0x11,
        0x1B => 0x2D,
        0x1C => 0x15,
        0x1D => 0x2C, // Z
        0x1E..=0x27 => 0x02 + u16::from(usage - 0x1E),
        0x28 => 0x1C,
        0x29 => 0x01,
        0x2A => 0x0E,
        0x2B => 0x0F,
        0x2C => 0x39,
        0x2D => 0x0C,
        0x2E => 0x0D,
        0x2F => 0x1A,
        0x30 => 0x1B,
        0x31 => 0x2B,
        0x32 => 0x2B,
        0x33 => 0x27,
        0x34 => 0x28,
        0x35 => 0x29,
        0x36 => 0x33,
        0x37 => 0x34,
        0x38 => 0x35,
        0x39 => 0x3A,
        0x3A..=0x43 => 0x3B + u16::from(usage - 0x3A), // F1..F10
        0x44 => 0x57,
        0x45 => 0x58,
        0x46 => E | 0x37,
        0x47 => 0x46,
        0x48 => 0x45,
        0x49 => E | 0x52,
        0x4A => E | 0x47,
        0x4B => E | 0x49,
        0x4C => E | 0x53,
        0x4D => E | 0x4F,
        0x4E => E | 0x51,
        0x4F => E | 0x4D,
        0x50 => E | 0x4B,
        0x51 => E | 0x50,
        0x52 => E | 0x48,
        0x53 => E | 0x45,
        0x54 => E | 0x35,
        0x55 => 0x37,
        0x56 => 0x4A,
        0x57 => 0x4E,
        0x58 => E | 0x1C,
        0x59 => 0x4F,
        0x5A => 0x50,
        0x5B => 0x51,
        0x5C => 0x4B,
        0x5D => 0x4C,
        0x5E => 0x4D,
        0x5F => 0x47,
        0x60 => 0x48,
        0x61 => 0x49,
        0x62 => 0x52,
        0x63 => 0x53,
        0x64 => 0x56,
        0x65 => E | 0x5D,
        0x67 => 0x59,
        0x68..=0x72 => 0x64 + u16::from(usage - 0x68), // F13..F23
        0x73 => 0x76,                                  // F24
        0xE0 => 0x1D,
        0xE1 => 0x2A,
        0xE2 => 0x38,
        0xE3 => E | 0x5B,
        0xE4 => E | 0x1D,
        0xE5 => 0x36,
        0xE6 => E | 0x38,
        0xE7 => E | 0x5C,
        _ => return None,
    };
    Some(s)
}

/// Inverse of [`usage_to_scancode`].
pub fn scancode_to_usage(scancode: u16) -> Option<u8> {
    // 0x2B is shared by '\' and Non-US '#'; prefer '\'.
    (0u8..=0xFF).find(|&u| u != 0x32 && usage_to_scancode(u) == Some(scancode))
}

/// Consumer page (0x0C) usages offered for media bindings.
pub const CONSUMER_KEYS: &[(u16, &str)] = &[
    (0x00CD, "Play/Pause"),
    (0x00B5, "Next Track"),
    (0x00B6, "Previous Track"),
    (0x00B7, "Stop"),
    (0x00E2, "Mute"),
    (0x00E9, "Volume Up"),
    (0x00EA, "Volume Down"),
    (0x006F, "Brightness Up"),
    (0x0070, "Brightness Down"),
    (0x0183, "Media Player"),
    (0x018A, "Mail"),
    (0x0192, "Calculator"),
    (0x0194, "File Explorer"),
    (0x0221, "Browser Search"),
    (0x0223, "Browser Home"),
    (0x0224, "Browser Back"),
    (0x0225, "Browser Forward"),
    (0x0226, "Browser Stop"),
    (0x0227, "Browser Refresh"),
    (0x022A, "Browser Favorites"),
    (0x0032, "Sleep"),
];

pub fn consumer_name(usage: u16) -> Option<&'static str> {
    CONSUMER_KEYS
        .iter()
        .find(|(u, _)| *u == usage)
        .map(|(_, n)| *n)
}

pub fn consumer_label(usage: u16) -> String {
    consumer_name(usage)
        .map(str::to_string)
        .unwrap_or_else(|| format!("Consumer {usage:#06x}"))
}

pub fn consumer_from_name(name: &str) -> Option<u16> {
    let n = name.trim();
    if let Some(hex) = n
        .strip_prefix("Consumer 0x")
        .or_else(|| n.strip_prefix("0x"))
    {
        return u16::from_str_radix(hex, 16).ok();
    }
    CONSUMER_KEYS
        .iter()
        .find(|(_, k)| k.eq_ignore_ascii_case(n))
        .map(|(u, _)| *u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for (u, n) in named_keys() {
            assert_eq!(key_from_name(n), Some(u), "{n}");
        }
        assert_eq!(key_from_name("c"), Some(0x06));
        assert_eq!(key_from_name("0x99"), Some(0x99));
        for (u, n) in CONSUMER_KEYS {
            assert_eq!(consumer_from_name(n), Some(*u));
        }
    }

    #[test]
    fn scancodes_unique_and_invertible() {
        for (u, _) in named_keys() {
            if let Some(s) = usage_to_scancode(u) {
                if u != 0x32 {
                    assert_eq!(scancode_to_usage(s), Some(u), "usage {u:#04x}");
                }
            }
        }
    }

    #[test]
    fn letters_are_physical_us_positions() {
        // AZERTY puts 'A' on the physical key of US 'Q': both are usage 0x14
        // and scancode 0x10. Usage 0x04 is the key left of 'S' (AZERTY 'Q').
        assert_eq!(usage_to_scancode(0x14), Some(0x10));
        assert_eq!(usage_to_scancode(0x04), Some(0x1E));
        assert_eq!(usage_to_scancode(0x1A), Some(0x11)); // US W / AZERTY Z
        assert_eq!(usage_to_scancode(0x1D), Some(0x2C)); // US Z / AZERTY W
        assert_eq!(usage_to_scancode(0x33), Some(0x27)); // US ; / AZERTY M
        assert_eq!(usage_to_scancode(0x10), Some(0x32)); // US M / AZERTY ,
        assert_eq!(usage_to_scancode(0x68), Some(0x64)); // F13
        assert_eq!(usage_to_scancode(0x73), Some(0x76)); // F24
    }
}
