//! Button-slot-to-physical-button mapping.
//!
//! libratbag and Solaar both index buttons purely by slot; neither names the
//! physical buttons. The mapping below comes from two sources:
//! * the factory (ROM) bindings read from the mouse: slot 0-4 send mouse
//!   buttons 1-5, slot 5 = DPI shift, 6 = DPI down, 7 = DPI up, 8 = profile
//!   cycle, 9 = tilt right, 10 = tilt left;
//! * piper's `data/svgs/logitech-g502.svg` (used for `usb:046d:c08b`), which
//!   draws hotspots `button0`..`button10` at the matching physical buttons.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonSlot {
    pub slot: usize,
    pub label: &'static str,
    /// Where the button is, in words.
    pub location: &'static str,
}

pub const SLOTS_046D_C08B: &[ButtonSlot] = &[
    ButtonSlot {
        slot: 0,
        label: "Left click",
        location: "main left button",
    },
    ButtonSlot {
        slot: 1,
        label: "Right click",
        location: "main right button",
    },
    ButtonSlot {
        slot: 2,
        label: "Wheel click",
        location: "pressing the scroll wheel",
    },
    ButtonSlot {
        slot: 3,
        label: "Back (G4)",
        location: "thumb, rear button",
    },
    ButtonSlot {
        slot: 4,
        label: "Forward (G5)",
        location: "thumb, front button",
    },
    ButtonSlot {
        slot: 5,
        label: "Sniper (G6)",
        location: "thumb, large front button",
    },
    ButtonSlot {
        slot: 6,
        label: "G7",
        location: "left of left click, rear (default DPI down)",
    },
    ButtonSlot {
        slot: 7,
        label: "G8",
        location: "left of left click, front (default DPI up)",
    },
    ButtonSlot {
        slot: 8,
        label: "G9",
        location: "behind the scroll wheel (default profile cycle)",
    },
    ButtonSlot {
        slot: 9,
        label: "Wheel tilt right",
        location: "push the wheel right",
    },
    ButtonSlot {
        slot: 10,
        label: "Wheel tilt left",
        location: "push the wheel left",
    },
];

pub fn slots_for(product_id: u16) -> &'static [ButtonSlot] {
    match product_id {
        0xc08b => SLOTS_046D_C08B,
        _ => &[],
    }
}

pub fn slot_label(product_id: u16, slot: usize) -> String {
    slots_for(product_id)
        .iter()
        .find(|s| s.slot == slot)
        .map(|s| s.label.to_string())
        .unwrap_or_else(|| format!("Button slot {slot}"))
}
