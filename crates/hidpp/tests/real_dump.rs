//! Encode/decode tests against a raw dump of a real 046d:c08b.

use hidpp::backup::Backup;
use hidpp::crc;
use hidpp::export::ProfileExport;
use hidpp::memory::{OnboardMemory, Source};
use hidpp::onboard::ROM_BASE;
use hidpp::profile::{Binding, Directory, KeyCombo, Led, Profile, Rgb, Special, OFF_BUTTONS};

fn dump() -> Backup {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/c08b-2026-10-29.json"
    );
    Backup::load(std::path::Path::new(path)).expect("fixture loads")
}

fn user_profile_sectors(b: &Backup) -> Vec<(u16, Vec<u8>)> {
    (1..=u16::from(b.info.profile_count))
        .map(|s| (s, b.sector(s).unwrap()))
        .collect()
}

#[test]
fn fixture_crcs() {
    let b = dump();
    assert!(crc::is_valid(&b.sector(0).unwrap()), "directory");
    for (s, d) in user_profile_sectors(&b) {
        assert!(crc::is_valid(&d), "sector {s}");
    }
    // ROM sectors carry no CRC.
    assert!(!crc::is_valid(&b.sector(ROM_BASE + 1).unwrap()));
}

#[test]
fn profiles_round_trip_byte_exact() {
    let b = dump();
    for (s, raw) in user_profile_sectors(&b) {
        let p = Profile::decode(&raw, &b.info).unwrap();
        assert_eq!(p.encode().unwrap(), raw, "sector {s}");
        // A fresh decode of the encoding is the same profile.
        assert_eq!(Profile::decode(&p.encode().unwrap(), &b.info).unwrap(), p);
    }
}

#[test]
fn every_binding_round_trips() {
    let b = dump();
    for (_, raw) in user_profile_sectors(&b) {
        for &bytes in raw[OFF_BUTTONS..OFF_BUTTONS + 128].as_chunks::<4>().0 {
            assert_eq!(Binding::decode(bytes).encode(), bytes);
        }
    }
}

#[test]
fn directory_round_trip_byte_exact() {
    let b = dump();
    let raw = b.sector(0).unwrap();
    let d = Directory::decode(&raw, 5);
    assert_eq!(d.entries.len(), 5);
    assert!(!d.entries[0].enabled);
    assert!(d.entries[1..].iter().all(|e| e.enabled));
    assert_eq!(d.encode(raw.len()), raw);
}

#[test]
fn decoded_values() {
    let b = dump();
    let m = OnboardMemory::from_backup(&b).unwrap();
    assert!(m.directory_valid);
    assert!(m.profiles.iter().all(|p| p.source == Source::User));
    let p1 = &m.profiles[0].profile;
    assert_eq!(p1.report_rate_hz(), 1000);
    assert_eq!(p1.dpi_levels(), vec![1200, 2400, 3200, 6400]);
    assert_eq!(p1.default_dpi_index, 1);
    assert_eq!(p1.buttons.len(), 11);
    assert_eq!(p1.buttons[0], Binding::MouseButtons(1));
    assert_eq!(p1.buttons[6], Binding::function(Special::DpiDown));
    assert_eq!(p1.buttons[7], Binding::function(Special::DpiUp));
    assert_eq!(
        p1.leds[0],
        Led::Cycle {
            period_ms: 8000,
            brightness: 100
        }
    );
    let p3 = &m.profiles[2].profile;
    assert_eq!(p3.report_rate_hz(), 500);
    assert_eq!(p3.buttons[6].to_string(), "Ctrl + C");
    assert_eq!(p3.buttons[7].to_string(), "Ctrl + V");
    assert_eq!(p3.gshift_buttons[4].to_string(), "Ctrl + Shift + T");
    assert_eq!(
        p3.leds[1],
        Led::Static {
            color: Rgb(255, 255, 255),
            ramp: 0
        }
    );
}

#[test]
fn unchanged_memory_plans_no_writes() {
    let m = OnboardMemory::from_backup(&dump()).unwrap();
    assert!(m.plan(&m.clone()).unwrap().is_empty());
    assert!(m.diff(&m.clone()).is_empty());
}

#[test]
fn rom_profiles_equal_user_sectors_after_sealing() {
    // This mouse still has factory content, so each ROM profile with its CRC
    // filled in must equal the user sector byte for byte.
    let b = dump();
    for (s, user) in user_profile_sectors(&b) {
        let mut rom = b.sector(ROM_BASE + s).unwrap();
        crc::seal(&mut rom);
        assert_eq!(rom, user, "profile {s}");
    }
}

#[test]
fn factory_reset_only_touches_directory_here() {
    let m = OnboardMemory::from_backup(&dump()).unwrap();
    let f = m.factory_reset();
    let plan = m.plan(&f).unwrap();
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].sector, 0);
    assert!(f.profiles.iter().all(|p| p.enabled));
    assert_eq!(m.diff(&f), vec!["Profile 1: enabled".to_string()]);
}

#[test]
fn single_binding_change_touches_only_that_slot() {
    let m = OnboardMemory::from_backup(&dump()).unwrap();
    let mut e = m.clone();
    let ctrl_c: KeyCombo = "Ctrl + C".parse().unwrap();
    e.profiles[3].profile.buttons[6] = Binding::Key(ctrl_c);
    let plan = m.plan(&e).unwrap();
    assert_eq!(plan.len(), 1);
    let w = &plan[0];
    assert_eq!(w.sector, 4);
    let before = w.before.as_ref().unwrap();
    let changed: Vec<usize> = (0..256).filter(|&i| before[i] != w.after[i]).collect();
    let slot = OFF_BUTTONS + 6 * 4;
    assert!(
        changed
            .iter()
            .all(|&i| (slot..slot + 4).contains(&i) || i >= 254),
        "{changed:x?}"
    );
    assert_eq!(&w.after[slot..slot + 4], &[0x80, 0x02, 0x01, 0x06]);
    assert!(crc::is_valid(&w.after));
    assert_eq!(
        m.diff(&e),
        vec!["Profile 4: G7: DPI down → Ctrl + C".to_string()]
    );
}

#[test]
fn edits_of_every_kind_encode_and_decode() {
    let b = dump();
    let mut p = Profile::decode(&b.sector(1).unwrap(), &b.info).unwrap();
    p.report_interval_ms = 2;
    p.dpi = [800, 1600, 0, 0, 0];
    p.default_dpi_index = 1;
    p.shift_dpi_index = 0;
    p.buttons[3] = Binding::Consumer(0x00CD);
    p.buttons[4] = Binding::Disabled;
    p.buttons[5] = Binding::function(Special::ProfileNext);
    p.buttons[8] = Binding::mouse_button(3);
    p.leds[1] = Led::Breathing {
        color: Rgb(1, 2, 3),
        period_ms: 3000,
        waveform: 0,
        brightness: 50,
    };
    p.name = "Work".into();
    let enc = p.encode().unwrap();
    assert!(crc::is_valid(&enc));
    let back = Profile::decode(&enc, &b.info).unwrap();
    assert_eq!(back, p);
    assert_eq!(
        &enc[OFF_BUTTONS + 16..OFF_BUTTONS + 20],
        &[0x80, 0x00, 0xff, 0xff]
    );
}

#[test]
fn json_export_import_round_trip() {
    let b = dump();
    for (_, raw) in user_profile_sectors(&b) {
        let p = Profile::decode(&raw, &b.info).unwrap();
        let json = ProfileExport::new(0x046d, 0xc08b, &p).to_json().unwrap();
        let imported = ProfileExport::from_json(&json).unwrap();
        let mut target = Profile::decode(&raw, &b.info).unwrap();
        imported.apply_to(&mut target).unwrap();
        assert_eq!(target.encode().unwrap(), raw);
    }
    let p3 = Profile::decode(&b.sector(3).unwrap(), &b.info).unwrap();
    let json = ProfileExport::new(0x046d, 0xc08b, &p3).to_json().unwrap();
    assert!(json.contains("\"keys\": \"Ctrl + C\""), "{json}");
    assert!(json.contains("\"function\": \"g_shift\""));
}

#[test]
fn invalid_directory_falls_back_to_rom() {
    let mut b = dump();
    let s0 = b.sectors.iter_mut().find(|s| s.sector == 0).unwrap();
    let mut d = s0.bytes().unwrap();
    d[0] ^= 0xFF;
    s0.data = Some(hidpp::backup::to_hex(&d));
    let m = OnboardMemory::from_backup(&b).unwrap();
    assert!(!m.directory_valid);
    assert_eq!(m.profiles[2].source, Source::Rom(0x0103));
    assert!(
        m.profiles.iter().all(|p| p.enabled),
        "ROM directory enables all five"
    );
}

#[test]
fn refuses_to_disable_every_profile() {
    let m = OnboardMemory::from_backup(&dump()).unwrap();
    let mut e = m.clone();
    e.profiles.iter_mut().for_each(|p| p.enabled = false);
    assert!(m.plan(&e).is_err());
}

#[test]
fn invalid_profile_values_are_rejected() {
    let b = dump();
    let mut p = Profile::decode(&b.sector(1).unwrap(), &b.info).unwrap();
    p.default_dpi_index = 4; // only 4 levels
    assert!(p.encode().is_err());
    p.default_dpi_index = 0;
    p.dpi = [800, 0, 1600, 0, 0];
    assert!(p.encode().is_err());
}

#[test]
fn key_combo_parsing() {
    let k: KeyCombo = "Ctrl + Shift + T".parse().unwrap();
    assert_eq!((k.modifiers, k.key), (0x03, 0x17));
    let k: KeyCombo = "ctrl+v".parse().unwrap();
    assert_eq!((k.modifiers, k.key), (0x01, 0x19));
    assert_eq!("Ctrl + Num +".parse::<KeyCombo>().unwrap().key, 0x57);
    assert!("Ctrl + C + V".parse::<KeyCombo>().is_err());
    assert!("Hyper + C".parse::<KeyCombo>().is_err());
}
