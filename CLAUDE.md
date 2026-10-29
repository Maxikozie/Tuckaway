# MouseForge

Windows 11 app that edits the onboard memory of a wired mouse, USB `046d:c08b`, over HID++ 2.0. It needs no vendor software. Rust workspace with three crates:

- `crates/hidpp`: protocol, device I/O, sector decode/encode, backups. No GUI dependencies; uses `thiserror`.
- `crates/cli`: `mouseforge-cli` with `list`, `dump [--raw] [--from F]`, `backup`, `restore F [--yes]`, `activate N`, `set P SLOT BINDING [--gshift] [--yes]`. Commands that write are dry runs unless given `--yes`.
- `crates/gui`: `mouseforge`, built on eframe/egui 0.37.

GUI module map:

| file | what it does |
|------|--------------|
| `app.rs` | UI and state |
| `worker.rs` | device thread, hot-plug, all I/O; talks to the UI over `Cmd`/`Ev` channels |
| `schematic.rs` | painter-drawn mouse with hotspots |
| `keycapture.rs` | scancode hook and layout labels |
| `theme.rs` | dark theme; Segoe UI fallback font |
| `ui_tests.rs` | kittest tests |

`README.md` has the full protocol write-up and user docs. `ref/` (gitignored) holds clones of libratbag, Solaar and piper: the sources for every protocol detail.

## Build / test / run

```bash
cargo build --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p gui -- --ignored hook
cargo run -p cli -- dump
cargo run -p gui
```

- The `--ignored hook` test injects real Right Ctrl + F24 keystrokes, which is why it is opt-in.
- `cargo run -p cli -- dump` is read-only, but it saves a backup.
- Release builds are static CRT (`.cargo/config.toml`), have no console (`windows_subsystem` in release only), use LTO and strip, and embed an icon and version info (`crates/gui/build.rs` via winresource; needs the Windows SDK `rc.exe`).
- Kill a running `mouseforge.exe` before rebuilding, or linking fails with "Access is denied".
- Keep clippy clean and run `cargo fmt`.

## Verified on the user's mouse (firmware U1 27.03.B0010)

- **HID collections.** HID++ is on interface 1, usage page `0xFF00`. On Windows these are separate paths: usage 1 = short reports `0x10` (`MI_01&Col04`), usage 2 = long reports `0x11` (`MI_01&Col05`). Device index is `0xFF`, our software id is `0x0C`. A "HID VHF Driver" device with the same VID:PID also exists (LampArray service); it is excluded by the usage-page filter.
- **Features.** `0x8070` LED effects (2 zones: 1 = primary, 2 = logo; effects off/static/breathing/cycle), `0x2201` DPI (100–25600, step 50), `0x8060` report rate (1/2/4/8 ms), `0x8100` onboard profiles. Feature indexes are always resolved through Root.
- **LED zones on the body** (confirmed by eye, 2026-10-29): zone 0 (location 1, "primary") is the 3-segment DPI strip; zone 1 (location 2, "logo") is the G logo.
- **LED live preview.** 0x8070 fn 3 setZoneEffect `[zone, 11-byte effect, persist=0]` works in onboard mode and the user saw the colours. Both zones report `persistency_caps = 0`.
  - An ACK is **not** proof that the colour is visible. Only the user looking at the mouse confirms it.
  - **Software-control flag.** fn 7 getSWControl / fn 8 setSWControl, one byte: 1 = host drives the LEDs, 0 = the firmware drives them from the active profile. Solaar's 0x8070 "LED Control" setting is exactly this (read `0x70`, write `0x80`). The flag survives profile switches.
  - **Preview** = setSWControl(1), then setZoneEffect.
  - **Revert** = setSWControl(0). Do this on Discard, on app close, on profile switch and after Apply.
  - Verified 2026-10-29: after clearing the flag, cycling with G9 showed each profile's own lighting (yellow, purple, cyan breathing, white). Re-selecting the profile, re-setting onboard mode or resending colours does **not** clear the flag.
  - The LampArray service had left the flag at 1.
- **Onboard memory.** 5 profiles, 5 ROM profiles, 11 buttons, 16 sectors × 256 B, profile format 2. Mechanical layout `0x0A` means a G-shift table and DPI shift are present.
- **Sectors.** `0x0000` is the directory (`sector BE16, enabled, 00` per profile, then `FF FF 00 00`). `0x0001`–`0x0005` are the profiles; `0x0006`–`0x000F` are all zeros (unused). ROM is `0x0100` (directory) and `0x0101`–`0x0105` (profiles). ROM sectors have **no CRC**.
- **Active profile.** getCurrentProfile and setCurrentProfile use the BE16 **sector** number.
- **Profile layout.** See the table in `profile.rs` / README:
  - DPI is little-endian at offset 3 (5 slots, 0 = unused).
  - Buttons are at 32, the G-shift table at 96.
  - The name is at 160 (UTF-16LE per Solaar; this mouse's names are all `FF`, so the encoding is untested).
  - LEDs are 4 × 11 B at 208. Zones 0/1 are edited; the alternate slots (`FF` here) are left untouched.
- **Checksum.** CRC-16/CCITT (seed `0xFFFF`, polynomial `0x1021`), big-endian in the last 2 bytes of every user sector.
- **Factory state.** User profiles were byte-identical to ROM apart from the CRC; a test asserts this. Factory reset = copy ROM → user sectors, seal the CRC, rebuild the directory from the ROM enabled flags.
- **Button slots** (`hidpp::layout`). 0 left, 1 right, 2 wheel click, 3 back (G4), 4 forward (G5), 5 sniper (G6), 6 G7, 7 G8, 8 G9 (profile cycle), 9 tilt right, 10 tilt left.
  - Derived from the ROM default bindings plus piper's `logitech-g502.svg`.
  - **Slots 6 and 7 are confirmed on hardware** with a test binding: slot 6 is the rear button left of left click (G7), slot 7 the front one (G8).
  - The rest are inferred from the defaults; they have not been individually tested.
- **"Disabled" encoding.** Written as `80 00 FF FF` (Solaar's NO_ACTION). libratbag writes type `0x00`, which decodes as a macro, so it is deliberately not followed. Not yet tested on hardware.
- **Function bindings.** Written as `90 code FF FF`, which matches the factory data.

**Current mouse state** (2026-10-29):

- Profile 1 is disabled; profiles 2–5 are enabled.
- Profiles 3 and 4 have G7 = Ctrl + C and G8 = Ctrl + V. Profile 3 was like that from the factory; profile 4 was written by us.
- Profiles 2 and 5 still have DPI down/up on G7/G8.
- The active profile changes when the user presses G9.

## Write safety rules (enforced in code; keep them)

- `Device::write_sector` needs a `BackupProof`, which only `Backup::save` can create. The backup is written, then parsed back.
- **Refused writes:**
  - ROM sectors (≥ `0x0100`)
  - sectors ≥ `sector_count`
  - data of the wrong length
  - data with an invalid CRC
- Every sector is read back and compared byte for byte. A failed sector is retried once from the start. Streamed `memoryWrite` chunks are never retried individually (`call_once`).
- After writing: `memory::finish` sets onboard mode and re-selects the active profile, then checks the mode.
- Encoding is a **patch** over the original bytes. Only changed fields are rewritten, so unknown and reserved bytes survive and an unedited profile re-encodes byte-exact.
- `OnboardMemory::plan` refuses to leave no profile enabled.
- The GUI's apply re-reads the mouse first. If its memory changed since load, nothing is written and the GUI reloads.
- **Hardware writes need the user's explicit OK.** Confirm unknown slots with a harmless test binding first; numpad keys are layout-independent.

## Keyboard bindings

They store **physical** HID usages; the user's layout is Belgian AZERTY. Capture uses a `WH_KEYBOARD_LL` hook because egui's `physical_key`:

- merges the numpad into the digit row;
- has no right-hand modifiers;
- turns Ctrl+C/X/V into clipboard events.

Labels come from `MapVirtualKeyExW` on the active layout. Tests load the `0000080C` and `00000409` layouts without activating them.

## Backups and fixtures

- **Backups:** `%LOCALAPPDATA%\MouseForge\backups\backup-<UTC stamp>-<label>.json` (hex of every user sector, the ROM directory and the ROM profiles). The GUI saves one on first connect per session and before every write, restore or factory reset. The repo's `backups/` folder (gitignored) holds early CLI probes.
- **Test fixture:** `crates/hidpp/tests/data/c08b-2026-10-29.json` is the real dump from before any write; `real_dump.rs` and `gui/src/ui_tests.rs` use it.
- **`assets/local/`** (gitignored) holds the user's reference photo of the mouse. Use it only to get button positions and proportions right. Never embed, trace or ship it: no manufacturer images, logos or trademarks in the app. The schematic must stay an original drawing.

## egui 0.37 / GUI notes (for the redesign)

- **API.**
  - `eframe::App::ui(&mut self, ui, frame)` replaces `update`.
  - Panels are `egui::Panel::top/left/bottom(..).show(ui, ..)`.
  - Modals use `egui::Modal::new(Id::unique(..)).show(ctx, ..)`.
  - Use `ui.scope_id()`, not `ui.id()`.
- **ScrollArea** inherits the parent layout. Wrap its contents in `ui.vertical` inside a horizontal parent (this bug happened once).
- **Click areas over children.** A whole-card click area must be registered *before* its children: `ui.scope_builder(UiBuilder::new().sense(click), ..)`. Otherwise it steals clicks from checkboxes inside it.
- **Glyphs.** The default font lacks arrows and symbols. `theme.rs` adds Segoe UI as a fallback, but painter shapes are preferred for icons.
- **kittest UI tests:**
  - They query AccessKit labels, so renaming visible labels breaks them.
  - Every widget needs an accessible name; the harness's accessibility check fails otherwise.
  - Hotspots are labelled `"mouse <slot>: <binding>"`.
  - Use `h.step()` instead of `h.run()` while a spinner shows.
- **Worker boundary.** Keep the `Worker` `Cmd`/`Ev` interface. Tests build `App::with_worker` with fake channels.

## Open issues / not yet verified on hardware

- Physical unplug/replug (only tested with a simulated disconnect).
- An Apply from the GUI. It uses the same lib path as the CLI writes, which did work.
- DPI, report-rate and LED writes; the `80 00 FF FF` disabled binding.
- Live shortcut recording on the user's AZERTY keyboard. Layout tests and the injected-key hook test pass.
- Macros are shown and preserved but not editable. Power mode, angle snapping and alternate LEDs are not editable.
- **Windows Dynamic Lighting overrides the LEDs.** While `logi_lamparray_service` runs, our colours flash and are taken back at once (the user saw purple breathing). The user stopped and removed the service on 2026-10-29. The app must only detect the service and say so in the Lighting tab, never stop it.
- Only `046d:c08b` is in `SUPPORTED_PRODUCT_IDS`.
- At small window heights the binding editor needs scrolling; this is part of the redesign brief.
