//! HID++ 2.0 access to the onboard profile memory of wired gaming mice.
//!
//! The protocol details (memory read/write sequence, sector layout, button
//! encoding, CRC, ROM fallback) follow libratbag (`src/hidpp20.c`,
//! `src/driver-hidpp20.c`) and Solaar (`lib/logitech_receiver/hidpp20.py`).

pub mod backup;
pub mod crc;
pub mod device;
pub mod error;
pub mod export;
pub mod features;
pub mod keys;
pub mod layout;
pub mod memory;
pub mod onboard;
pub mod profile;

pub use device::{Candidate, Device};
pub use error::{Error, Result};
pub use hidapi::HidApi;
