//! Tuckaway desktop app: Tauri commands over the `hidpp` crate. All protocol
//! work happens here and in `hidpp`; the Svelte frontend only renders.

mod actions;
mod keymap;
mod mouse;
mod winsvc;
mod worker;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hidpp::profile::{Led, Profile};
use serde_json::Value;
use tauri::{Emitter, Manager, State};

use actions::Action;
use mouse::{Connector, MockConnector, RealConnector};
use worker::{Ev, Op, ProfileEdit, Worker};

struct App {
    worker: Worker,
    /// Last device event, for a frontend that loads after it was sent.
    latest: Arc<Mutex<Ev>>,
}

type CmdResult<T> = Result<T, String>;

async fn run(app: State<'_, App>, op: Op) -> CmdResult<Value> {
    let w = app.worker.clone();
    tauri::async_runtime::spawn_blocking(move || w.call(op))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn device_state(app: State<'_, App>) -> Ev {
    app.latest.lock().unwrap().clone()
}

#[tauri::command]
fn catalog() -> Vec<Action> {
    actions::catalog(keymap::current_layout())
}

/// A recorded shortcut from `KeyboardEvent.code`s held together.
#[tauri::command]
fn shortcut(codes: Vec<String>) -> Option<Action> {
    let combo = keymap::combo_from_codes(&codes)?;
    Some(actions::describe(
        &hidpp::profile::Binding::Key(combo),
        keymap::current_layout(),
    ))
}

/// Re-describe a profile's bindings (after an import or a layout change).
#[tauri::command]
fn describe_profile(profile: Profile) -> worker::ActionsView {
    worker::actions_for(&profile, keymap::current_layout())
}

#[tauri::command]
fn lighting_service_running() -> bool {
    winsvc::service_running(winsvc::LAMPARRAY_SERVICE)
}

#[tauri::command]
async fn reload(app: State<'_, App>) -> CmdResult<Value> {
    run(app, Op::Reload).await
}

#[tauri::command]
async fn apply(app: State<'_, App>, edits: Vec<ProfileEdit>, active: u16) -> CmdResult<Value> {
    run(app, Op::Apply { edits, active }).await
}

#[tauri::command]
async fn activate(app: State<'_, App>, sector: u16) -> CmdResult<Value> {
    run(app, Op::Activate(sector)).await
}

#[tauri::command]
async fn led_preview(app: State<'_, App>, zone: u8, led: Led) -> CmdResult<Value> {
    run(app, Op::LedPreview { zone, led }).await
}

#[tauri::command]
async fn led_release(app: State<'_, App>) -> CmdResult<Value> {
    run(app, Op::LedRelease).await
}

#[tauri::command]
async fn backup_now(app: State<'_, App>) -> CmdResult<Value> {
    run(app, Op::BackupNow).await
}

#[tauri::command]
async fn restore(app: State<'_, App>, path: PathBuf) -> CmdResult<Value> {
    run(app, Op::Restore(path)).await
}

#[tauri::command]
async fn factory_reset(app: State<'_, App>) -> CmdResult<Value> {
    run(app, Op::FactoryReset).await
}

#[tauri::command]
async fn export_profile(app: State<'_, App>, profile: Profile, path: PathBuf) -> CmdResult<Value> {
    run(
        app,
        Op::Export {
            profile: Box::new(profile),
            path,
        },
    )
    .await
}

#[tauri::command]
async fn import_profile(app: State<'_, App>, index: usize, path: PathBuf) -> CmdResult<Value> {
    run(app, Op::Import { index, path }).await
}

#[tauri::command]
fn backup_dir() -> PathBuf {
    hidpp::backup::default_backup_dir()
}

/// `--mock [fixture.json]` or `TUCKAWAY_MOCK=1|fixture.json` serves a raw
/// backup instead of the mouse.
fn mock_path() -> Option<PathBuf> {
    let default = || {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../hidpp/tests/data/c08b-2026-10-29.json")
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--mock" {
            return Some(args.next().map(PathBuf::from).unwrap_or_else(default));
        }
    }
    match std::env::var("TUCKAWAY_MOCK") {
        Ok(v) if v == "1" => Some(default()),
        Ok(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    }
}

/// The app was called MouseForge; copy its backups into the new folder once.
/// Never moves or deletes anything.
fn adopt_old_backups() {
    let new = hidpp::backup::default_backup_dir();
    let Some(old) = new
        .parent()
        .and_then(|p| p.parent())
        .map(|base| base.join("MouseForge").join("backups"))
    else {
        return;
    };
    if new.exists() || !old.is_dir() {
        return;
    }
    if std::fs::create_dir_all(&new).is_err() {
        return;
    }
    for e in std::fs::read_dir(&old).into_iter().flatten().flatten() {
        let _ = std::fs::copy(e.path(), new.join(e.file_name()));
    }
}

pub fn main() {
    adopt_old_backups();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let connector: Box<dyn Connector> = match mock_path() {
                Some(p) => Box::new(
                    MockConnector::load(&p).map_err(|e| format!("mock {}: {e}", p.display()))?,
                ),
                None => {
                    Box::new(RealConnector::new().map_err(|e| format!("HID unavailable: {e}"))?)
                }
            };
            let latest = Arc::new(Mutex::new(Ev::Searching));
            let (l, handle) = (latest.clone(), app.handle().clone());
            let worker = Worker::spawn(connector, move |ev| {
                {
                    let mut cur = l.lock().unwrap();
                    match (&ev, &mut *cur) {
                        (Ev::Active { sector, mode }, Ev::Connected(s)) => {
                            s.active_sector = *sector;
                            s.mode = *mode;
                        }
                        (Ev::Busy(_), _) => {}
                        _ => *cur = ev.clone(),
                    }
                }
                let _ = handle.emit("device", ev);
            });
            app.manage(App { worker, latest });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                // Hand the LEDs back to the firmware before we go.
                let app = window.state::<App>();
                let _ = app
                    .worker
                    .call_timeout(Op::LedRelease, Duration::from_millis(1500));
            }
        })
        .invoke_handler(tauri::generate_handler![
            device_state,
            catalog,
            shortcut,
            describe_profile,
            lighting_service_running,
            reload,
            apply,
            activate,
            led_preview,
            led_release,
            backup_now,
            restore,
            factory_reset,
            export_profile,
            import_profile,
            backup_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tuckaway");
}
