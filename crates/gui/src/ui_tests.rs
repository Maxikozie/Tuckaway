//! Headless UI tests (egui_kittest + AccessKit) on the real 046d:c08b dump,
//! with a fake device worker so no hardware is touched.

use std::sync::mpsc::{self, Receiver, Sender};

use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use hidpp::backup::Backup;
use hidpp::device::LedZone;
use hidpp::memory::OnboardMemory;
use hidpp::profile::{Binding, KeyCombo};

use crate::app::App;
use crate::worker::{Cmd, Ev, Loaded, Worker};

struct Fake {
    cmds: Receiver<Cmd>,
    events: Sender<Ev>,
}

fn fixture() -> OnboardMemory {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../hidpp/tests/data/c08b-2026-10-29.json"
    );
    OnboardMemory::from_backup(&Backup::load(std::path::Path::new(path)).unwrap()).unwrap()
}

fn loaded(mode: u8) -> Loaded {
    Loaded {
        name: "Test Mouse".into(),
        id: "046d:c08b".into(),
        firmware: vec![],
        memory: fixture(),
        dpi_values: (100..=25600).step_by(50).collect(),
        rates_ms: vec![1, 2, 4, 8],
        led_zones: vec![
            LedZone {
                index: 0,
                location: 1,
                effects: vec![0, 1, 0x0a, 3],
            },
            LedZone {
                index: 1,
                location: 2,
                effects: vec![0, 1, 0x0a, 3],
            },
        ],
        active_sector: 4,
        mode,
    }
}

fn harness(mode: u8) -> (Harness<'static, App>, Fake) {
    let (tx, cmds) = mpsc::channel();
    let (events, rx) = mpsc::channel();
    events.send(Ev::Connected(Box::new(loaded(mode)))).unwrap();
    let h = Harness::builder()
        .with_size(egui::vec2(1200.0, 820.0))
        .build_eframe(move |cc| App::with_worker(cc, Worker { tx, rx }));
    let mut h = h;
    h.run();
    (h, Fake { cmds, events })
}

#[test]
fn shows_device_and_profiles() {
    let (h, _f) = harness(1);
    h.get_by_label_contains("Test Mouse");
    for n in 1..=5 {
        h.get_by_label(&format!("Profile {n}"));
    }
    h.get_by_label("ACTIVE");
    // Active profile 4 is preselected and the G7 editor shows its binding.
    h.get_by_label("Editing profile 4");
}

#[test]
fn hotspot_opens_binding_editor() {
    let (mut h, _f) = harness(1);
    h.get_by_label_contains("mouse G9:").click();
    h.run();
    h.get_by_label_contains("behind the scroll wheel");
    let action = h.get_by_label("Action");
    assert_eq!(
        action.accesskit_node().value().as_deref(),
        Some("Profile switch")
    );
}

#[test]
fn edit_apply_flow_sends_the_edit() {
    let (mut h, f) = harness(1);
    h.get_by_label_contains("mouse G8:").click();
    h.run();
    h.get_by_label("Action").click();
    h.run();
    h.get_by_label("Keyboard shortcut").click();
    h.run();
    // Default keyboard binding is Ctrl + C; change the key to V.
    h.get_by_label("1 unsaved change");
    h.get_by_label("Apply to mouse").click();
    h.run();
    h.get_by_label_contains("Profile 4: G8: DPI up");
    h.get_by_label("Write").click();
    h.step(); // busy spinner keeps animating until the worker replies
    match f.cmds.try_recv().expect("an apply command") {
        Cmd::Apply {
            base,
            edited,
            active,
        } => {
            assert_eq!(active, 4);
            assert_eq!(base.profiles, fixture().profiles);
            let b = &edited.profiles[3].profile.buttons[7];
            assert_eq!(
                *b,
                Binding::Key(KeyCombo {
                    modifiers: 1,
                    key: 0x06
                })
            );
            assert!(base.plan(&edited).unwrap().len() == 1);
        }
        _ => panic!("expected Cmd::Apply"),
    }
}

#[test]
fn revert_discards_edits() {
    let (mut h, _f) = harness(1);
    h.get_by_label_contains("mouse G7:").click();
    h.run();
    h.get_by_label("Action").click();
    h.run();
    h.get_by_label("Disabled").click();
    h.run();
    h.get_by_label("1 unsaved change");
    h.get_by_label("Revert").click();
    h.run();
    assert!(h.query_by_label("1 unsaved change").is_none());
}

#[test]
fn disabling_every_profile_is_refused() {
    let (mut h, f) = harness(1);
    for _ in 0..4 {
        h.get_all_by_label("Enabled")
            .find(|n| n.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True))
            .unwrap()
            .click();
        h.run();
    }
    h.get_by_label("Apply to mouse").click();
    h.run();
    h.get_by_label_contains("at least one profile must stay enabled");
    assert!(f.cmds.try_recv().is_err());
}

#[test]
fn dpi_and_lighting_tabs() {
    let (mut h, _f) = harness(1);
    h.get_by_label("DPI & report rate").click();
    h.run();
    h.get_by_label("Level 1");
    let rate = h.get_by_label("polling rate");
    assert_eq!(rate.accesskit_node().value().as_deref(), Some("1000 Hz"));
    h.get_by_label("Lighting").click();
    h.run();
    h.get_by_label("Main zone");
    h.get_by_label("Logo");
}

#[test]
fn host_mode_offers_switch() {
    let (mut h, f) = harness(2);
    h.get_by_label("Switch to onboard mode").click();
    h.run();
    assert!(matches!(f.cmds.try_recv(), Ok(Cmd::SetOnboardMode)));
}

#[test]
fn disconnect_shows_empty_state() {
    let (mut h, f) = harness(1);
    f.events.send(Ev::Disconnected("unplugged".into())).unwrap();
    h.run();
    h.get_by_label("Mouse not connected");
    f.events.send(Ev::Connected(Box::new(loaded(1)))).unwrap();
    h.run();
    h.get_by_label("Profile 1");
}
