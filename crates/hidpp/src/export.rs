//! Readable JSON import/export of a single profile.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::layout;
use crate::profile::Profile;

pub const FORMAT: &str = "mouseforge-profile";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileExport {
    pub format: String,
    pub version: u32,
    /// "vvvv:pppp" of the mouse the profile came from.
    pub device: String,
    /// Informational: what each button slot is (ignored on import).
    #[serde(default)]
    pub button_labels: Vec<String>,
    pub profile: Profile,
}

impl ProfileExport {
    pub fn new(vendor_id: u16, product_id: u16, profile: &Profile) -> Self {
        ProfileExport {
            format: FORMAT.into(),
            version: VERSION,
            device: format!("{vendor_id:04x}:{product_id:04x}"),
            button_labels: (0..profile.buttons.len())
                .map(|i| layout::slot_label(product_id, i))
                .collect(),
            profile: profile.clone(),
        }
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(s: &str) -> Result<Self> {
        let e: ProfileExport = serde_json::from_str(s)?;
        if e.format != FORMAT {
            return Err(Error::Invalid(format!(
                "not a profile export (format {:?})",
                e.format
            )));
        }
        if e.version > VERSION {
            return Err(Error::Invalid(format!(
                "profile export version {} is newer than supported",
                e.version
            )));
        }
        e.profile.validate()?;
        Ok(e)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    /// Apply onto an existing profile (keeps its raw base bytes). Lists are
    /// fitted to the target's slot counts; slots the export lacks keep the
    /// target's current value.
    pub fn apply_to(&self, target: &mut Profile) -> Result<()> {
        fn fit<T: Clone>(new: &[T], old: &[T]) -> Vec<T> {
            old.iter()
                .enumerate()
                .map(|(i, o)| new.get(i).unwrap_or(o).clone())
                .collect()
        }
        let mut p = self.profile.clone();
        p.buttons = fit(&self.profile.buttons, &target.buttons);
        p.gshift_buttons = fit(&self.profile.gshift_buttons, &target.gshift_buttons);
        p.leds = fit(&self.profile.leds, &target.leds);
        p.validate()?;
        target.apply_from(&p);
        Ok(())
    }
}
