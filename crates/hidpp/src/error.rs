use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("HID error: {0}")]
    Hid(#[from] hidapi::HidError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("no supported mouse found")]
    NotFound,

    #[error("device disconnected")]
    Disconnected,

    #[error(
        "timed out waiting for a reply to feature {feature_index:#04x} function {function:#x}"
    )]
    Timeout { feature_index: u8, function: u8 },

    #[error("device returned HID++ error {code:#04x} ({}) for feature {feature_index:#04x} function {function:#x}", hidpp20_error_name(*.code))]
    Protocol {
        feature_index: u8,
        function: u8,
        code: u8,
    },

    #[error("device does not support feature {0:#06x}")]
    FeatureNotSupported(u16),

    #[error("unsupported onboard memory format: {0}")]
    UnsupportedFormat(String),

    #[error(
        "sector {sector:#06x} has an invalid CRC (stored {stored:#06x}, computed {computed:#06x})"
    )]
    BadCrc {
        sector: u16,
        stored: u16,
        computed: u16,
    },

    #[error("refusing to write: {0}")]
    WriteRefused(String),

    #[error("read-back of sector {0:#06x} does not match what was written")]
    VerifyFailed(u16),

    #[error("invalid data: {0}")]
    Invalid(String),
}

/// Error codes from the HID++ 2.0 error report (feature index 0xFF).
pub fn hidpp20_error_name(code: u8) -> &'static str {
    match code {
        0x00 => "no error",
        0x01 => "unknown",
        0x02 => "invalid argument",
        0x03 => "out of range",
        0x04 => "hardware error",
        0x05 => "logitech internal",
        0x06 => "invalid feature index",
        0x07 => "invalid function id",
        0x08 => "busy",
        0x09 => "unsupported",
        _ => "unrecognised",
    }
}
