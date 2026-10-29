//! Physical keys and their labels on the user's keyboard layout.
//!
//! Bindings store *physical* HID usages. The webview reports the physical key
//! of every keystroke as a W3C `KeyboardEvent.code` ("KeyQ", "Numpad1",
//! "ControlRight"), which maps 1:1 to a usage, so recording needs no hook.
//! Labels come from the active Windows layout: on Belgian AZERTY usage 0x14
//! (the key US calls Q) is labelled "A".

use hidpp::keys;
use hidpp::profile::KeyCombo;

/// W3C `code` → HID keyboard usage (page 0x07).
pub fn usage_for_code(code: &str) -> Option<u8> {
    if let Some(c) = code.strip_prefix("Key") {
        let b = c.as_bytes();
        if b.len() == 1 && b[0].is_ascii_uppercase() {
            return Some(0x04 + (b[0] - b'A'));
        }
    }
    if let Some(d) = code.strip_prefix("Digit") {
        let n: u8 = d.parse().ok()?;
        return Some(if n == 0 { 0x27 } else { 0x1D + n });
    }
    if let Some(d) = code.strip_prefix("Numpad") {
        if let Ok(n) = d.parse::<u8>() {
            return Some(if n == 0 { 0x62 } else { 0x58 + n });
        }
    }
    if let Some(f) = code.strip_prefix('F') {
        if let Ok(n) = f.parse::<u8>() {
            return match n {
                1..=12 => Some(0x39 + n),
                13..=24 => Some(0x68 + n - 13),
                _ => None,
            };
        }
    }
    Some(match code {
        "Enter" => 0x28,
        "Escape" => 0x29,
        "Backspace" => 0x2A,
        "Tab" => 0x2B,
        "Space" => 0x2C,
        "Minus" => 0x2D,
        "Equal" => 0x2E,
        "BracketLeft" => 0x2F,
        "BracketRight" => 0x30,
        "Backslash" => 0x31,
        "Semicolon" => 0x33,
        "Quote" => 0x34,
        "Backquote" => 0x35,
        "Comma" => 0x36,
        "Period" => 0x37,
        "Slash" => 0x38,
        "CapsLock" => 0x39,
        "PrintScreen" => 0x46,
        "ScrollLock" => 0x47,
        "Pause" => 0x48,
        "Insert" => 0x49,
        "Home" => 0x4A,
        "PageUp" => 0x4B,
        "Delete" => 0x4C,
        "End" => 0x4D,
        "PageDown" => 0x4E,
        "ArrowRight" => 0x4F,
        "ArrowLeft" => 0x50,
        "ArrowDown" => 0x51,
        "ArrowUp" => 0x52,
        "NumLock" => 0x53,
        "NumpadDivide" => 0x54,
        "NumpadMultiply" => 0x55,
        "NumpadSubtract" => 0x56,
        "NumpadAdd" => 0x57,
        "NumpadEnter" => 0x58,
        "NumpadDecimal" => 0x63,
        "IntlBackslash" => 0x64,
        "ContextMenu" => 0x65,
        "IntlRo" => 0x87,
        "IntlYen" => 0x89,
        "ControlLeft" => 0xE0,
        "ShiftLeft" => 0xE1,
        "AltLeft" => 0xE2,
        "MetaLeft" => 0xE3,
        "ControlRight" => 0xE4,
        "ShiftRight" => 0xE5,
        "AltRight" => 0xE6,
        "MetaRight" => 0xE7,
        _ => return None,
    })
}

/// Modifiers held plus the last non-modifier key, from the codes held down.
pub fn combo_from_codes(codes: &[String]) -> Option<KeyCombo> {
    let mut combo = KeyCombo {
        modifiers: 0,
        key: 0,
    };
    for c in codes {
        let u = usage_for_code(c)?;
        match keys::modifier_bit_for_usage(u) {
            Some(bit) => combo.modifiers |= bit,
            None => combo.key = u,
        }
    }
    (combo.modifiers != 0 || combo.key != 0).then_some(combo)
}

/// Usages whose label depends on the keyboard layout.
fn is_layout_dependent(usage: u8) -> bool {
    matches!(usage, 0x04..=0x27 | 0x2D..=0x38 | 0x64)
}

pub use platform::{current_layout, layout_char, Layout};

/// Label of a physical key on `layout`, falling back to the US name.
pub fn key_label(usage: u8, layout: Layout) -> String {
    layout_char(usage, layout)
        .filter(|_| is_layout_dependent(usage))
        .unwrap_or_else(|| keys::key_label(usage))
}

pub fn combo_label(k: &KeyCombo, layout: Layout) -> String {
    let mut parts: Vec<String> = keys::modifier_names(k.modifiers)
        .into_iter()
        .map(String::from)
        .collect();
    if k.key != 0 {
        parts.push(key_label(k.key, layout));
    }
    parts.join(" + ")
}

/// The physical key that types `ch` (an ASCII letter) on `layout`.
pub fn usage_for_letter(ch: char, layout: Layout) -> Option<u8> {
    let want = ch.to_ascii_uppercase().to_string();
    (0x04u8..=0x1D).find(|&u| layout_char(u, layout).as_deref() == Some(want.as_str()))
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, MapVirtualKeyExW, HKL, MAPVK_VK_TO_CHAR, MAPVK_VSC_TO_VK_EX,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    #[derive(Clone, Copy, Debug)]
    pub struct Layout(pub HKL);
    // An HKL is a plain identifier, not a pointer we dereference.
    unsafe impl Send for Layout {}

    /// The layout the user is typing with: that of the foreground window's
    /// thread (the worker thread's own layout can be the system default).
    pub fn current_layout() -> Layout {
        unsafe {
            let w = GetForegroundWindow();
            let tid = if w.is_null() {
                0
            } else {
                GetWindowThreadProcessId(w, std::ptr::null_mut())
            };
            Layout(GetKeyboardLayout(tid))
        }
    }

    /// Character on `layout` for the physical key of `usage`.
    pub fn layout_char(usage: u8, layout: Layout) -> Option<String> {
        let sc = hidpp::keys::usage_to_scancode(usage)?;
        let vsc = u32::from(sc & 0xFF) | if sc & 0x100 != 0 { 0xE000 } else { 0 };
        let vk = unsafe { MapVirtualKeyExW(vsc, MAPVK_VSC_TO_VK_EX, layout.0) };
        // Applications match shortcuts on virtual keys, so letters and digits
        // show their VK ("1" rather than AZERTY's unshifted "&").
        if (0x30..=0x39).contains(&vk) || (0x41..=0x5A).contains(&vk) {
            return char::from_u32(vk).map(String::from);
        }
        let ch = unsafe { MapVirtualKeyExW(vk, MAPVK_VK_TO_CHAR, layout.0) } & 0x7FFF_FFFF;
        char::from_u32(ch)
            .filter(|c| !c.is_control() && !c.is_whitespace())
            .map(|c| c.to_uppercase().collect())
    }
}

#[cfg(not(windows))]
mod platform {
    #[derive(Clone, Copy, Debug)]
    pub struct Layout;
    pub fn current_layout() -> Layout {
        Layout
    }
    pub fn layout_char(usage: u8, _: Layout) -> Option<String> {
        hidpp::keys::key_name(usage)
            .filter(|n| n.chars().count() == 1)
            .map(String::from)
    }
}

#[cfg(all(test, windows))]
pub mod tests {
    use super::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayoutList, LoadKeyboardLayoutW, UnloadKeyboardLayout, HKL, KLF_NOTELLSHELL,
    };

    /// A layout loaded for the test without activating it; unloaded on drop
    /// unless the user already had it.
    pub struct TestLayout {
        pub layout: Layout,
        unload: bool,
    }

    impl TestLayout {
        pub fn load(klid: &str) -> TestLayout {
            let mut before = [std::ptr::null_mut(); 64];
            let n = unsafe { GetKeyboardLayoutList(64, before.as_mut_ptr()) } as usize;
            let w: Vec<u16> = klid.encode_utf16().chain([0]).collect();
            let hkl: HKL = unsafe { LoadKeyboardLayoutW(w.as_ptr(), KLF_NOTELLSHELL) };
            assert!(!hkl.is_null(), "layout {klid} not available");
            TestLayout {
                layout: Layout(hkl),
                unload: !before[..n].contains(&hkl),
            }
        }
    }

    impl Drop for TestLayout {
        fn drop(&mut self) {
            if self.unload {
                unsafe { UnloadKeyboardLayout(self.layout.0) };
            }
        }
    }

    #[test]
    fn codes_map_to_physical_usages() {
        assert_eq!(usage_for_code("KeyA"), Some(0x04));
        assert_eq!(usage_for_code("KeyZ"), Some(0x1D));
        assert_eq!(usage_for_code("Digit1"), Some(0x1E));
        assert_eq!(usage_for_code("Digit0"), Some(0x27));
        assert_eq!(usage_for_code("Numpad1"), Some(0x59));
        assert_eq!(usage_for_code("Numpad0"), Some(0x62));
        assert_eq!(usage_for_code("F1"), Some(0x3A));
        assert_eq!(usage_for_code("F12"), Some(0x45));
        assert_eq!(usage_for_code("F13"), Some(0x68));
        assert_eq!(usage_for_code("F24"), Some(0x73));
        assert_eq!(usage_for_code("ControlRight"), Some(0xE4));
        assert_eq!(usage_for_code("IntlBackslash"), Some(0x64));
        assert_eq!(usage_for_code("Fn"), None);
    }

    #[test]
    fn combo_keeps_sides_and_numpad() {
        let codes = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let c = combo_from_codes(&codes(&["ControlRight", "ShiftLeft", "Numpad5"])).unwrap();
        assert_eq!(c.modifiers, keys::modifier::RCTRL | keys::modifier::LSHIFT);
        assert_eq!(c.key, 0x5D);
        assert!(combo_from_codes(&codes(&[])).is_none());
        assert!(combo_from_codes(&codes(&["Fn"])).is_none());
    }

    #[test]
    fn azerty_letters_sit_on_other_physical_keys() {
        let be = TestLayout::load("0000080C");
        let us = TestLayout::load("00000409");
        // Ctrl+A on AZERTY is the key US calls Q; C/V/X/S/Y do not move.
        assert_eq!(usage_for_letter('a', be.layout), Some(0x14));
        assert_eq!(usage_for_letter('a', us.layout), Some(0x04));
        assert_eq!(usage_for_letter('z', be.layout), Some(0x1A));
        for ch in ['c', 'v', 'x', 's', 'y'] {
            assert_eq!(
                usage_for_letter(ch, be.layout),
                usage_for_letter(ch, us.layout)
            );
        }
        assert_eq!(key_label(0x14, be.layout), "A");
        assert_eq!(key_label(0x1E, be.layout), "1");
    }
}
