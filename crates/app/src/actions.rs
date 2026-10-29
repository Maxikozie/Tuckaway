//! What a button can do, in words people use: the grouped catalog the action
//! picker shows, and the friendly name of any binding ("Copy", not
//! "Ctrl + C"). Shortcut presets are resolved on the active keyboard layout,
//! so "Select all" on AZERTY sends Ctrl + the physical key labelled A.

use hidpp::keys::{self, modifier};
use hidpp::profile::{Binding, KeyCombo, Special};
use serde::Serialize;

use crate::keymap::{self, Layout};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Mouse,
    Shortcuts,
    Media,
    Dpi,
    Profiles,
    Disabled,
    /// Not offered in the picker: macros, unassigned, unknown bytes.
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct Action {
    pub label: String,
    pub group: Group,
    /// Shown for custom shortcuts only ("Ctrl + Shift + F5").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<String>,
    pub editable: bool,
    pub binding: Binding,
}

const PRESETS: &[(char, &str)] = &[
    ('c', "Copy"),
    ('v', "Paste"),
    ('x', "Cut"),
    ('z', "Undo"),
    ('y', "Redo"),
    ('a', "Select all"),
    ('s', "Save"),
];

const MEDIA: &[(u16, &str)] = &[
    (0x00CD, "Play / pause"),
    (0x00B5, "Next track"),
    (0x00B6, "Previous track"),
    (0x00E9, "Volume up"),
    (0x00EA, "Volume down"),
    (0x00E2, "Mute"),
];

const MOUSE: &[(u16, &str)] = &[
    (0x01, "Left click"),
    (0x02, "Right click"),
    (0x04, "Middle click"),
    (0x08, "Back"),
    (0x10, "Forward"),
];

fn function_name(f: Special) -> (&'static str, Group) {
    match f {
        Special::TiltLeft => ("Scroll left", Group::Mouse),
        Special::TiltRight => ("Scroll right", Group::Mouse),
        Special::ScrollUp => ("Scroll up", Group::Mouse),
        Special::ScrollDown => ("Scroll down", Group::Mouse),
        Special::DpiUp => ("DPI up", Group::Dpi),
        Special::DpiDown => ("DPI down", Group::Dpi),
        Special::DpiCycle => ("Cycle DPI", Group::Dpi),
        Special::DpiDefault => ("Default DPI", Group::Dpi),
        Special::DpiShift => ("DPI shift", Group::Dpi),
        Special::ProfileNext => ("Next profile", Group::Profiles),
        Special::ProfilePrevious => ("Previous profile", Group::Profiles),
        Special::ProfileCycle => ("Cycle profiles", Group::Profiles),
        Special::GShift => ("G-shift", Group::Profiles),
        _ => ("", Group::Other),
    }
}

/// The picker's actions, in display order.
pub fn catalog(layout: Layout) -> Vec<Action> {
    let mut out = Vec::new();
    for &(mask, _) in MOUSE {
        out.push(describe(&Binding::MouseButtons(mask), layout));
    }
    for f in [Special::TiltLeft, Special::TiltRight] {
        out.push(describe(&Binding::function(f), layout));
    }
    for &(ch, _) in PRESETS {
        if let Some(key) = keymap::usage_for_letter(ch, layout) {
            out.push(describe(
                &Binding::Key(KeyCombo {
                    modifiers: modifier::LCTRL,
                    key,
                }),
                layout,
            ));
        }
    }
    for &(u, _) in MEDIA {
        out.push(describe(&Binding::Consumer(u), layout));
    }
    for f in [
        Special::DpiUp,
        Special::DpiDown,
        Special::DpiCycle,
        Special::DpiDefault,
        Special::DpiShift,
        Special::ProfileNext,
        Special::ProfilePrevious,
        Special::ProfileCycle,
        Special::GShift,
    ] {
        out.push(describe(&Binding::function(f), layout));
    }
    out.push(describe(&Binding::Disabled, layout));
    out
}

/// A shortcut that matches a preset gets its name ("Copy"); either control
/// key counts, nothing else may be held.
fn preset_name(k: &KeyCombo, layout: Layout) -> Option<&'static str> {
    if k.modifiers != modifier::LCTRL && k.modifiers != modifier::RCTRL {
        return None;
    }
    let ch = keymap::layout_char(k.key, layout)?;
    PRESETS
        .iter()
        .find(|(c, _)| ch.eq_ignore_ascii_case(&c.to_string()))
        .map(|(_, n)| *n)
}

pub fn describe(b: &Binding, layout: Layout) -> Action {
    let (label, group, keys_text, editable) = match b {
        Binding::MouseButtons(mask) => {
            let name = MOUSE
                .iter()
                .find(|(m, _)| m == mask)
                .map(|(_, n)| n.to_string())
                .unwrap_or_else(|| hidpp::profile::mouse_button_name(*mask));
            (name, Group::Mouse, None, true)
        }
        Binding::Key(k) => match preset_name(k, layout) {
            Some(n) => (n.to_string(), Group::Shortcuts, None, true),
            None => {
                let text = keymap::combo_label(k, layout);
                (text.clone(), Group::Shortcuts, Some(text), true)
            }
        },
        Binding::Consumer(u) => {
            let name = MEDIA
                .iter()
                .find(|(c, _)| c == u)
                .map(|(_, n)| n.to_string())
                .unwrap_or_else(|| keys::consumer_label(*u));
            (name, Group::Media, None, true)
        }
        Binding::Disabled => ("Disabled".into(), Group::Disabled, None, true),
        Binding::Function { function, .. } => match function_name(*function) {
            ("", _) => (function.label(), Group::Other, None, true),
            (n, g) => (n.to_string(), g, None, true),
        },
        Binding::Macro(_) => ("Macro".into(), Group::Other, None, false),
        Binding::Unassigned => ("Not assigned".into(), Group::Other, None, true),
        Binding::Unknown(_) => ("Unknown action".into(), Group::Other, None, false),
    };
    Action {
        label,
        group,
        keys: keys_text,
        editable,
        binding: b.clone(),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::keymap::tests::TestLayout;

    #[test]
    fn presets_follow_the_layout() {
        let be = TestLayout::load("0000080C");
        let us = TestLayout::load("00000409");
        let find = |l: Layout, name: &str| {
            catalog(l)
                .into_iter()
                .find(|a| a.label == name)
                .unwrap()
                .binding
        };
        // AZERTY "Select all" is Ctrl + the physical Q key.
        assert_eq!(
            find(be.layout, "Select all"),
            Binding::Key(KeyCombo {
                modifiers: modifier::LCTRL,
                key: 0x14
            })
        );
        assert_eq!(
            find(us.layout, "Select all"),
            Binding::Key(KeyCombo {
                modifiers: modifier::LCTRL,
                key: 0x04
            })
        );
        // Ctrl + physical A is "Select all" on US but "Ctrl + Q" on AZERTY.
        let b = Binding::Key(KeyCombo {
            modifiers: modifier::LCTRL,
            key: 0x04,
        });
        assert_eq!(describe(&b, us.layout).label, "Select all");
        let a = describe(&b, be.layout);
        assert_eq!(a.label, "Ctrl + Q");
        assert_eq!(a.keys.as_deref(), Some("Ctrl + Q"));
    }

    #[test]
    fn friendly_names_for_factory_bindings() {
        let us = TestLayout::load("00000409");
        let l = us.layout;
        assert_eq!(describe(&Binding::MouseButtons(1), l).label, "Left click");
        assert_eq!(
            describe(&Binding::function(Special::DpiDown), l).label,
            "DPI down"
        );
        assert_eq!(
            describe(&Binding::function(Special::ProfileCycle), l).label,
            "Cycle profiles"
        );
        assert_eq!(
            describe(&Binding::function(Special::TiltLeft), l).label,
            "Scroll left"
        );
        assert_eq!(describe(&Binding::Consumer(0xCD), l).label, "Play / pause");
        assert!(!describe(&Binding::Macro([0, 1, 0, 0]), l).editable);
        let c = describe(
            &Binding::Key(KeyCombo {
                modifiers: modifier::LCTRL,
                key: 0x06,
            }),
            l,
        );
        assert_eq!(
            (c.label.as_str(), c.group, c.keys.is_none()),
            ("Copy", Group::Shortcuts, true)
        );
    }

    #[test]
    fn catalog_covers_every_group_once_per_label() {
        let us = TestLayout::load("00000409");
        let cat = catalog(us.layout);
        let mut labels: Vec<_> = cat.iter().map(|a| a.label.clone()).collect();
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), cat.len());
        for g in [
            Group::Mouse,
            Group::Shortcuts,
            Group::Media,
            Group::Dpi,
            Group::Profiles,
            Group::Disabled,
        ] {
            assert!(cat.iter().any(|a| a.group == g), "{g:?}");
        }
    }
}
