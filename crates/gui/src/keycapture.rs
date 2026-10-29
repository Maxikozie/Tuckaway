//! Physical key capture and layout-aware key labels.
//!
//! Bindings store physical HID usages. egui's `physical_key` cannot tell the
//! numpad from the digit row, has no right-hand modifiers, and Ctrl+C/X/V are
//! turned into clipboard events using the *logical* key. So while the user is
//! recording a shortcut we read scancodes from a low-level keyboard hook,
//! installed only for that moment and only acting while our window has focus.
//! Scancodes map to usages with [`hidpp::keys::scancode_to_usage`].

use hidpp::keys;
use hidpp::profile::{Binding, KeyCombo};

/// Turns a stream of (scancode, pressed) into a combo: modifiers held plus the
/// first non-modifier key pressed. Scancodes carry 0x100 for E0-extended keys.
#[derive(Debug, Default)]
pub struct Capture {
    held: u8,
}

impl Capture {
    pub fn feed(&mut self, scancode: u16, pressed: bool) -> Option<KeyCombo> {
        let usage = keys::scancode_to_usage(scancode)?;
        if let Some(bit) = keys::modifier_bit_for_usage(usage) {
            if pressed {
                self.held |= bit;
            } else {
                self.held &= !bit;
            }
            return None;
        }
        pressed.then_some(KeyCombo {
            modifiers: self.held,
            key: usage,
        })
    }
}

/// Usages whose label depends on the keyboard layout.
fn is_layout_dependent(usage: u8) -> bool {
    matches!(usage, 0x04..=0x27 | 0x2D..=0x38 | 0x64)
}

/// Label of a physical key on the active keyboard layout ("A" for usage 0x14
/// on AZERTY), falling back to the US name for non-character keys.
pub fn key_label(usage: u8) -> String {
    platform::layout_char(usage, platform::current_layout())
        .filter(|_| is_layout_dependent(usage))
        .unwrap_or_else(|| keys::key_label(usage))
}

pub fn combo_label(k: &KeyCombo) -> String {
    let mut parts: Vec<String> = keys::modifier_names(k.modifiers)
        .into_iter()
        .map(String::from)
        .collect();
    if k.key != 0 {
        parts.push(key_label(k.key));
    }
    if parts.is_empty() {
        "(none)".into()
    } else {
        parts.join(" + ")
    }
}

pub fn binding_label(b: &Binding) -> String {
    match b {
        Binding::Key(k) => combo_label(k),
        other => other.to_string(),
    }
}

pub use platform::{poll, start, stop};

#[cfg(windows)]
mod platform {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, MapVirtualKeyExW, HKL, MAPVK_VK_TO_CHAR, MAPVK_VSC_TO_VK_EX,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetForegroundWindow, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION,
        HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
    };

    struct HookState {
        hook: HHOOK,
        window: HWND,
        ctx: egui::Context,
        queue: VecDeque<(u16, bool)>,
        lost_focus: bool,
    }

    // Raw handles are only touched on the UI thread (the hook runs there too).
    unsafe impl Send for HookState {}

    static STATE: Mutex<Option<HookState>> = Mutex::new(None);

    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            if let Ok(mut guard) = STATE.try_lock() {
                if let Some(st) = guard.as_mut() {
                    if unsafe { GetForegroundWindow() } != st.window {
                        st.lost_focus = true;
                    } else {
                        let k = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
                        // Bit 0x200 marks the fake Left Ctrl Windows sends with AltGr.
                        if k.scanCode & 0x200 == 0 {
                            let ext = if k.flags & LLKHF_EXTENDED != 0 {
                                0x100
                            } else {
                                0
                            };
                            let pressed =
                                wparam == WM_KEYDOWN as usize || wparam == WM_SYSKEYDOWN as usize;
                            st.queue
                                .push_back(((k.scanCode & 0xFF) as u16 | ext, pressed));
                        }
                        st.ctx.request_repaint();
                        // Swallow it: no Start menu, Alt+F4 or copy while recording.
                        return 1;
                    }
                }
            }
        }
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }

    /// Start recording. Call from the UI thread.
    pub fn start(ctx: &egui::Context) -> bool {
        let mut guard = STATE.lock().unwrap();
        if guard.is_some() {
            return true;
        }
        let hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(hook_proc),
                GetModuleHandleW(std::ptr::null()),
                0,
            )
        };
        if hook.is_null() {
            return false;
        }
        *guard = Some(HookState {
            hook,
            window: unsafe { GetForegroundWindow() },
            ctx: ctx.clone(),
            queue: VecDeque::new(),
            lost_focus: false,
        });
        true
    }

    pub fn stop() {
        if let Some(st) = STATE.lock().unwrap().take() {
            unsafe { UnhookWindowsHookEx(st.hook) };
        }
    }

    /// Drain recorded key events. `None` means recording ended because the
    /// window lost focus.
    pub fn poll() -> Option<Vec<(u16, bool)>> {
        let mut guard = STATE.lock().unwrap();
        let st = guard.as_mut()?;
        if st.lost_focus {
            drop(guard);
            stop();
            return None;
        }
        Some(st.queue.drain(..).collect())
    }

    pub fn current_layout() -> HKL {
        unsafe { GetKeyboardLayout(0) }
    }

    /// Character on `layout` for the physical key of `usage`.
    pub fn layout_char(usage: u8, layout: HKL) -> Option<String> {
        let sc = hidpp::keys::usage_to_scancode(usage)?;
        let vsc = u32::from(sc & 0xFF) | if sc & 0x100 != 0 { 0xE000 } else { 0 };
        let vk = unsafe { MapVirtualKeyExW(vsc, MAPVK_VSC_TO_VK_EX, layout) };
        // Applications match shortcuts on virtual keys, so letters and digits
        // show their VK ("1" rather than AZERTY's unshifted "&").
        if (0x30..=0x39).contains(&vk) || (0x41..=0x5A).contains(&vk) {
            return char::from_u32(vk).map(String::from);
        }
        let ch = unsafe { MapVirtualKeyExW(vk, MAPVK_VK_TO_CHAR, layout) } & 0x7FFF_FFFF;
        char::from_u32(ch)
            .filter(|c| !c.is_control() && !c.is_whitespace())
            .map(|c| c.to_uppercase().collect())
    }
}

#[cfg(not(windows))]
mod platform {
    pub type Hkl = ();
    pub fn start(_: &egui::Context) -> bool {
        false
    }
    pub fn stop() {}
    pub fn poll() -> Option<Vec<(u16, bool)>> {
        None
    }
    pub fn current_layout() -> Hkl {}
    pub fn layout_char(_: u8, _: Hkl) -> Option<String> {
        None
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayoutList, LoadKeyboardLayoutW, UnloadKeyboardLayout, HKL,
    };

    const KLF_NOTELLSHELL: u32 = 0x80;

    /// Load a layout for the test without activating it; unload it afterwards
    /// if it was not already loaded.
    struct Layout(HKL, bool);

    impl Layout {
        fn load(klid: &str) -> Layout {
            let mut before = vec![std::ptr::null_mut(); 64];
            let n = unsafe { GetKeyboardLayoutList(64, before.as_mut_ptr()) } as usize;
            before.truncate(n);
            let w: Vec<u16> = klid.encode_utf16().chain([0]).collect();
            let hkl = unsafe { LoadKeyboardLayoutW(w.as_ptr(), KLF_NOTELLSHELL) };
            assert!(!hkl.is_null(), "layout {klid} not available");
            Layout(hkl, !before.contains(&hkl))
        }
        fn label(&self, usage: u8) -> String {
            platform::layout_char(usage, self.0).unwrap()
        }
    }

    impl Drop for Layout {
        fn drop(&mut self) {
            if self.1 {
                unsafe { UnloadKeyboardLayout(self.0) };
            }
        }
    }

    /// Scancodes of the AZERTY keys labelled A, Z, Q, W, M.
    const AZ_A: u16 = 0x10;
    const AZ_Z: u16 = 0x11;
    const AZ_Q: u16 = 0x1E;
    const AZ_W: u16 = 0x2C;
    const AZ_M: u16 = 0x27;
    const LCTRL: u16 = 0x1D;
    const LSHIFT: u16 = 0x2A;
    const RCTRL: u16 = 0x11D;

    fn press(c: &mut Capture, keys: &[u16]) -> Option<KeyCombo> {
        let mut out = None;
        for &k in keys {
            out = out.or(c.feed(k, true));
        }
        out
    }

    #[test]
    fn belgian_azerty_keys_capture_physical_usages() {
        // Pressing the keys labelled A, Z, Q, W, M on Belgian AZERTY must store
        // the usages of the same *positions* on US QWERTY: Q, W, A, Z, ';'.
        let cases = [
            (AZ_A, 0x14, "A"),
            (AZ_Z, 0x1A, "Z"),
            (AZ_Q, 0x04, "Q"),
            (AZ_W, 0x1D, "W"),
            (AZ_M, 0x33, "M"),
        ];
        let be = Layout::load("0000080C");
        for (scan, usage, az_label) in cases {
            let combo = press(&mut Capture::default(), &[LCTRL, scan]).unwrap();
            assert_eq!(
                combo,
                KeyCombo {
                    modifiers: keys::modifier::LCTRL,
                    key: usage
                },
                "scan {scan:#x}"
            );
            assert_eq!(
                be.label(usage),
                az_label,
                "AZERTY label of usage {usage:#04x}"
            );
        }
    }

    #[test]
    fn us_layout_labels_differ_for_the_same_usages() {
        let us = Layout::load("00000409");
        assert_eq!(us.label(0x14), "Q");
        assert_eq!(us.label(0x1A), "W");
        assert_eq!(us.label(0x04), "A");
        assert_eq!(us.label(0x1D), "Z");
        assert_eq!(us.label(0x33), ";");
    }

    #[test]
    fn copy_paste_are_same_position_on_azerty() {
        let be = Layout::load("0000080C");
        // C and V sit on the same physical keys on AZERTY and QWERTY.
        let c = press(&mut Capture::default(), &[LCTRL, 0x2E]).unwrap();
        let v = press(&mut Capture::default(), &[LCTRL, 0x2F]).unwrap();
        assert_eq!((c.key, be.label(c.key)), (0x06, "C".to_string()));
        assert_eq!((v.key, be.label(v.key)), (0x19, "V".to_string()));
    }

    #[test]
    fn digit_row_shows_digits_on_azerty() {
        let be = Layout::load("0000080C");
        assert_eq!(be.label(0x1E), "1");
    }

    #[test]
    fn capture_tracks_modifier_release_and_sides() {
        let mut c = Capture::default();
        assert_eq!(c.feed(LSHIFT, true), None);
        assert_eq!(c.feed(LSHIFT, false), None);
        assert_eq!(
            c.feed(0x2E, true),
            Some(KeyCombo {
                modifiers: 0,
                key: 0x06
            })
        );
        let mut c = Capture::default();
        let combo = press(&mut c, &[RCTRL, LSHIFT, 0x14]).unwrap();
        assert_eq!(
            combo.modifiers,
            keys::modifier::RCTRL | keys::modifier::LSHIFT
        );
        // Numpad 7 (0x47) and the digit row 7 (0x08) are different usages.
        assert_eq!(c.feed(0x47, true).unwrap().key, 0x5F);
        assert_eq!(c.feed(0x08, true).unwrap().key, 0x24);
        // Extended 0x47 is Home, not numpad 7.
        assert_eq!(c.feed(0x147, true).unwrap().key, 0x4A);
    }

    /// End-to-end through the real low-level hook: injects Right Ctrl + F24
    /// (harmless if it ever leaked) as scancodes and checks what the hook
    /// recorded. Injects real keystrokes, so it only runs on request:
    /// `cargo test -p gui -- --ignored hook`.
    #[test]
    #[ignore]
    fn hook_records_injected_scancodes() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
            KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
        };

        let events = std::thread::spawn(|| {
            let ctx = egui::Context::default();
            assert!(start(&ctx), "hook installs");
            let key = |scan: u16, ext: bool, up: bool| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: 0,
                        wScan: scan,
                        dwFlags: KEYEVENTF_SCANCODE
                            | if ext { KEYEVENTF_EXTENDEDKEY } else { 0 }
                            | if up { KEYEVENTF_KEYUP } else { 0 },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            let inputs = [
                key(0x1D, true, false),
                key(0x76, false, false),
                key(0x76, false, true),
                key(0x1D, true, true),
            ];
            let sent = unsafe {
                SendInput(
                    inputs.len() as u32,
                    inputs.as_ptr(),
                    std::mem::size_of::<INPUT>() as i32,
                )
            };
            assert_eq!(sent, 4);
            let until = std::time::Instant::now() + std::time::Duration::from_millis(400);
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            while std::time::Instant::now() < until {
                while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0
                {
                    unsafe {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let ev = poll();
            stop();
            ev
        })
        .join()
        .unwrap()
        .expect("still recording (focus unchanged)");
        assert_eq!(
            ev_to_combo(&events),
            Some(KeyCombo {
                modifiers: keys::modifier::RCTRL,
                key: 0x73
            }),
            "{events:x?}"
        );
        assert_eq!(
            events,
            vec![(0x11D, true), (0x76, true), (0x76, false), (0x11D, false)]
        );
    }

    fn ev_to_combo(events: &[(u16, bool)]) -> Option<KeyCombo> {
        let mut c = Capture::default();
        events.iter().find_map(|&(s, p)| c.feed(s, p))
    }
}
