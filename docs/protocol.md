# How Tuckaway talks to the mouse

The protocol details come from reading libratbag (`src/hidpp20.c`,
`src/driver-hidpp20.c`, `data/devices/logitech-g502-hero.device`) and Solaar
(`lib/logitech_receiver/hidpp20.py`, `base.py`, `common.py`), then checking them
against dumps of the real mouse. Comments in `crates/hidpp` name the functions
each piece follows.

## Transport (`device.rs`)

The mouse exposes several HID interfaces. HID++ uses the vendor collections on
interface 1, usage page `0xFF00`. On Windows every top-level collection is a
separate device path:

| usage | report id | length | collection |
|-------|-----------|--------|------------|
| `0x0001` | `0x10` short | 7 bytes | `MI_01&Col04` |
| `0x0002` | `0x11` long | 20 bytes | `MI_01&Col05` |

Both are opened. Requests with up to 3 parameter bytes go out as short reports and
the rest as long reports. Replies are read from either collection.

A message is `[report id, device index, feature index, function << 4 | sw id, params…]`.
A wired device uses device index `0xFF`. The software id is `0x0C`; Solaar
documents `0x07`, `0x0A`, `0x0B`, `0x0D` and `0x0F` as taken by other tools.

A reply matches when the feature index and function/sw-id byte match. An error
reply is `[.., 0xFF, feature index, fn|sw, code]` (HID++ 2.0) or uses `0x8F`
(HID++ 1.0).

* Timeouts and "busy" errors are retried.
* Streamed memory writes are never resent chunk by chunk. A failed sector write is
  restarted from the beginning of the sector.

Feature indexes are resolved with **Root** (`0x0000`) `getFeature(id)`. The tool
also reads:

* `0x0001` feature set (dump only)
* `0x0003` firmware
* `0x0005` name
* `0x2201` DPI list: `0xE000 | step` marks a range, as in Solaar's `produce_dpi_list`
* `0x8060` report rates: bit *n* means *n + 1* ms
* `0x8070` LED zones and their effect ids

## Onboard Profiles, feature `0x8100` (`onboard.rs`)

| fn | name | parameters |
|----|------|------------|
| 0 | getProfilesDescr | → memory model, profile format, macro format, profile count, ROM profile count, button count, sector count, sector size (BE16), mechanical layout, various info |
| 1 | setOnboardMode | `1` onboard, `2` host |
| 2 | getOnboardMode | |
| 3 | setCurrentProfile | sector (BE16) |
| 4 | getCurrentProfile | → sector (BE16) |
| 5 | memoryRead | sector, offset (BE16 each) → 16 bytes |
| 6 | memoryAddrWrite | sector, offset, length (BE16 each) |
| 7 | memoryWrite | 16 data bytes |
| 8 | memoryWriteEnd | |
| 11 / 12 | get / set current DPI index | |

For `046d:c08b` the descriptor reports:

* memory model 1, profile format 2, macro format 1
* 5 profiles (5 in ROM) with 11 buttons each
* 16 sectors of 256 bytes
* mechanical layout `0x0A`: G-shift table and DPI shift present

**Reading.** A sector is read in 16-byte chunks. The firmware rejects reads that
go past `size - 16`, so the last chunk is re-aligned to end at the sector end.

**Writing.** A sector is written as `memoryAddrWrite(sector, 0, size)`, then
`size / 16` × `memoryWrite`, then `memoryWriteEnd`.

## Memory layout (`profile.rs`, `memory.rs`)

* **User memory**
  * Sector `0x0000` is the profile directory.
  * Sectors `0x0001`–`0x0005` are the profiles.
  * The remaining user sectors hold macros (empty on this mouse).
* **ROM**
  * `0x0100` is the ROM directory. It has no CRC and also carries a firmware string.
  * `0x0101`–`0x0105` are the factory profiles. They have no CRC either.

**Checksum.** Every user sector ends in a CRC-16/CCITT stored big-endian in its last
two bytes: seed `0xFFFF`, polynomial `0x1021`, no reflection, computed over the rest
of the sector. This is libratbag's `hidpp_crc_ccitt`; Solaar's table-driven `crc16`
is the same function.

**Directory.** Four bytes per profile: `sector (BE16), enabled, 00`. The list ends
with `FF FF 00 00`, and the rest of the sector is `FF` up to the CRC.

**Profile sector:**

| offset | size | field |
|--------|------|-------|
| 0 | 1 | report interval in ms (1 = 1000 Hz) |
| 1 | 1 | default DPI index |
| 2 | 1 | shift DPI index |
| 3 | 10 | 5 × DPI, little endian, 0 = unused |
| 13 | 3 | profile colour |
| 16 | 1 | power mode |
| 17 | 1 | angle snapping |
| 18 | 14 | write counter, reserved, power timeouts |
| 32 | 64 | 16 × 4-byte button bindings |
| 96 | 64 | 16 × 4-byte G-shift bindings |
| 160 | 48 | name (UTF-16LE; all `FF` = unnamed) |
| 208 | 44 | 4 × 11-byte LED effects: zone 0, zone 1, two alternates |
| 254 | 2 | CRC |

**Button binding:**

| bytes | meaning |
|-------|---------|
| `80 01 HH LL` | mouse buttons, BE16 bitmask (1 = left, 2 = right, 4 = middle, 8 = back, 16 = forward) |
| `80 02 MM KK` | keyboard: modifier bits `MM` (Ctrl 1, Shift 2, Alt 4, Win 8; right-hand ones ×16) and HID keyboard usage `KK` |
| `80 03 HH LL` | consumer (media) usage, BE16 |
| `80 00 FF FF` | no action ("Disabled"). This follows Solaar; libratbag writes a type byte of 0, which the firmware would read as a macro. |
| `90 FF xx DD` | function `FF` (see below); `xx` and `DD` are `FF` in the factory data |
| `0S SS AA AA` | run the macro at sector `S`, address `A` (kept as is, not editable) |
| `FF FF FF FF` | slot not assigned |

Function codes for `90`-type bindings:

| code | function |
|------|----------|
| 1 / 2 | wheel tilt left / right |
| 3 / 4 | DPI up / down |
| 5 | DPI cycle |
| 6 | DPI default |
| 7 | DPI shift |
| 8 / 9 | next / previous profile |
| 10 | cycle profiles |
| 11 | G-shift |
| 16 / 17 | scroll down / up |

**LED effect.** Byte 0 is the mode; the remaining bytes depend on it:

| mode | layout |
|------|--------|
| `00` off | |
| `01` static | colour at bytes 1–3 |
| `03` cycle | period at bytes 6–7 (BE16), intensity at byte 8 (0 = 100 %) |
| `0A` breathing | colour at bytes 1–3, period at 4–5, waveform at 6, intensity at 7 |

**Encoding is a patch.** The editor keeps the original sector bytes and only
rewrites fields whose decoded value changed. Unknown and reserved bytes are
preserved, and an unedited profile re-encodes to exactly the bytes that were read.

**Button slots.** libratbag and Solaar address buttons only by slot number. The
names in `layout.rs` come from three sources:

* the factory bindings: slots 0–4 send buttons 1–5, then DPI shift, DPI down,
  DPI up, profile cycle, tilt right, tilt left;
* piper's `logitech-g502.svg`, which places `button0`…`button10`;
* a physical test on the real mouse: the rear button left of left click is slot 6
  (G7) and the front one is slot 7 (G8).

**Fallback and factory reset.** If the directory CRC is invalid, or a profile's
sector is, that profile is shown from ROM, the same as libratbag's
`hidpp20_onboard_profiles_initialize`. Factory reset copies each ROM profile into
its user sector with a fresh CRC and rebuilds the directory from the ROM
directory's enabled flags.

## Safety rules (enforced in code)

* **Backup first.** `Device::write_sector` requires a `BackupProof`, which only
  `Backup::save` can create after the file has been written and parsed back.
* **What can be written.** Writes to ROM sectors, to sectors beyond the user range,
  of the wrong length, or with an invalid CRC are refused.
* **Verification.** Every written sector is read back and compared byte for byte.
* **Onboard mode.** After writing, the mouse is set to onboard mode and the active
  profile is re-selected; the mode is checked afterwards.
* **Stale data.** Before applying, the app re-reads the mouse. If the memory changed
  since it was loaded, it writes nothing and reloads.
* **Restore.** Restoring writes only CRC-valid user sectors that differ from the mouse.
* **Profiles.** At least one profile must stay enabled.

## Key capture

Shortcuts are recorded in the webview from `KeyboardEvent.code`, which names the
*physical* key: it tells the numpad from the digit row and left from right
modifiers, and `preventDefault` keeps Ctrl+C from copying. The fake Left Ctrl that
Windows sends with AltGr is dropped. `crates/app/src/keymap.rs` maps codes to HID
usages.

Labels come from the active layout: scancode → virtual key → character with
`MapVirtualKeyExW`. The one-click presets (Copy, Paste, Select all, …) are resolved
the same way, so "Select all" on Belgian AZERTY sends Ctrl + the key US calls Q.
Tests load the Belgian AZERTY (`0000080C`) and US (`00000409`) layouts without
activating them.
