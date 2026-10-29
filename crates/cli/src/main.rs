//! Debug CLI: `list`, `dump`, `backup`, `restore`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use hidpp::backup::{self, Backup};
use hidpp::memory::{self, OnboardMemory, Source};
use hidpp::onboard::{MODE_HOST, MODE_ONBOARD, ROM_BASE};
use hidpp::profile::{Binding, Special};
use hidpp::{device, layout, Device, HidApi};

const USAGE: &str = "\
usage: mouseforge-cli <command>

  list                      HID collections and supported mice
  dump [--raw] [--from F]   read, back up and decode the onboard memory
                            (--from decodes a backup file instead of the mouse)
  backup [DIR]              save a raw backup (default: %LOCALAPPDATA%\\MouseForge\\backups)
  restore FILE [--yes]      write a raw backup back (dry run without --yes)
  activate PROFILE          select the active onboard profile (1-based)
  set PROFILE SLOT BINDING [--gshift] [--yes]
                            change one button (dry run without --yes); BINDING is
                            key:Ctrl + C | mouse:N | consumer:Play/Pause |
                            function:dpi_up | disabled
";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| {
        args.iter()
            .position(|a| a == f)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    match args.first().map(String::as_str) {
        Some("list") => list(),
        Some("dump") => dump(flag("--raw"), value("--from")),
        Some("backup") => {
            let dir = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(backup::default_backup_dir);
            let mut dev = open()?;
            let b = Backup::read_from(&mut dev)?;
            let proof = b.save(&dir, "manual")?;
            println!("saved {}", proof.path().display());
            Ok(())
        }
        Some("restore") => {
            let file = args
                .get(1)
                .filter(|a| !a.starts_with("--"))
                .context("restore needs a backup file")?;
            restore(Path::new(file), flag("--yes"))
        }
        Some("activate") => {
            let n: u16 = args
                .get(1)
                .context("activate needs a profile number")?
                .parse()?;
            let mut dev = open()?;
            let mem = OnboardMemory::from_backup(&Backup::read_from(&mut dev)?)?;
            let slot = mem
                .profiles
                .get(usize::from(n).wrapping_sub(1))
                .context("no such profile")?;
            if !slot.enabled {
                bail!("profile {n} is disabled");
            }
            memory::finish(&mut dev, slot.sector)?;
            println!(
                "active profile: sector {:#06x}",
                dev.current_profile_sector()?
            );
            Ok(())
        }
        Some("set") => {
            let [p, slot, binding] = [1, 2, 3].map(|i| args.get(i).cloned().unwrap_or_default());
            set(&p, &slot, &binding, flag("--gshift"), flag("--yes"))
        }
        _ => {
            print!("{USAGE}");
            Ok(())
        }
    }
}

fn open() -> Result<Device> {
    let api = HidApi::new()?;
    Device::open_first(&api).context("no supported mouse found (expected 046d:c08b)")
}

fn list() -> Result<()> {
    let api = HidApi::new()?;
    println!("HID collections, vendor 046d:");
    for c in device::enumerate_collections(&api) {
        println!(
            "  {:04x}:{:04x} if={:>2} page={:#06x} usage={:#06x} {:?}\n      {}",
            c.vendor_id, c.product_id, c.interface, c.usage_page, c.usage, c.product, c.path
        );
    }
    let mice = device::find_mice(&api);
    println!("\nSupported mice with HID++ collections: {}", mice.len());
    for m in &mice {
        println!(
            "  {} {:?} interface {} short={} long={}",
            m.id_string(),
            m.product,
            m.interface,
            m.short_path.is_some(),
            m.long_path.is_some()
        );
    }
    Ok(())
}

fn dump(raw: bool, from: Option<String>) -> Result<()> {
    let b = match from {
        Some(f) => Backup::load(Path::new(&f))?,
        None => {
            let mut dev = open()?;
            let (major, minor) = dev.protocol_version()?;
            println!(
                "Device      {} ({})",
                dev.device_name().unwrap_or_default(),
                dev.candidate.id_string()
            );
            println!("HID++       {major}.{minor}");
            for fw in dev.firmware().unwrap_or_default() {
                println!("            {fw}");
            }
            println!("Features");
            for f in dev.list_features()? {
                println!(
                    "  [{:2}] {:#06x} {}{}",
                    f.index,
                    f.id,
                    f.name,
                    if f.flags & 0x40 != 0 { " (hidden)" } else { "" }
                );
            }
            if let Ok(d) = dev.dpi_list() {
                println!(
                    "Sensor DPI  {} .. {} ({} steps)",
                    d.first().unwrap_or(&0),
                    d.last().unwrap_or(&0),
                    d.len()
                );
            }
            if let Ok(r) = dev.report_rates_ms() {
                let hz: Vec<String> = r
                    .iter()
                    .map(|ms| format!("{} Hz", 1000 / u32::from(*ms)))
                    .collect();
                println!("Rates       {}", hz.join(", "));
            }
            if let Ok(zones) = dev.led_zones() {
                for z in zones {
                    let fx: Vec<String> = z.effects.iter().map(|e| format!("{e:#04x}")).collect();
                    println!(
                        "LED zone {}  location {} ({}), effects [{}]",
                        z.index,
                        z.location,
                        z.location_name(),
                        fx.join(", ")
                    );
                }
            }
            let b = Backup::read_from(&mut dev)?;
            let proof = b.save(&backup::default_backup_dir(), "dump")?;
            println!("Backup      {}", proof.path().display());
            b
        }
    };

    let mem = OnboardMemory::from_backup(&b)?;
    let i = &b.info;
    println!();
    println!(
        "Onboard     memory model {}, profile format {}, macro format {}",
        i.memory_model, i.profile_format, i.macro_format
    );
    println!(
        "            {} profiles ({} in ROM), {} buttons, {} sectors x {} bytes, G-shift {}, DPI shift {}",
        i.profile_count,
        i.rom_profile_count,
        i.button_count,
        i.sector_count,
        i.sector_size,
        yes(i.has_g_shift()),
        yes(i.has_dpi_shift())
    );
    let mode = match b.onboard_mode {
        Some(MODE_ONBOARD) => "onboard".to_string(),
        Some(MODE_HOST) => "host".to_string(),
        Some(m) => format!("unknown ({m})"),
        None => "?".into(),
    };
    println!("Mode        {mode}");
    match b.current_profile_sector {
        Some(s) if s >= ROM_BASE => println!("Active      ROM profile {}", s - ROM_BASE),
        Some(s) => println!("Active      profile {s} (sector {s:#06x})"),
        None => println!("Active      ?"),
    }
    println!(
        "Directory   {}",
        if mem.directory_valid {
            "valid CRC"
        } else {
            "INVALID CRC (mouse uses ROM profiles)"
        }
    );

    let labels = layout::slots_for(b.device.product_id);
    for slot in &mem.profiles {
        let p = &slot.profile;
        println!();
        let src = match slot.source {
            Source::User => format!("sector {:#06x}", slot.sector),
            Source::Rom(s) => format!("user sector invalid, showing ROM {s:#06x}"),
        };
        let rom_same = slot.index < mem.rom_profiles.len() && mem.rom_profiles[slot.index] == *p;
        println!(
            "== Profile {} [{}] {}{}{}",
            slot.index + 1,
            if slot.enabled { "enabled" } else { "disabled" },
            src,
            if b.current_profile_sector == Some(slot.sector) {
                "  << active"
            } else {
                ""
            },
            if rom_same {
                "  (identical to factory)"
            } else {
                "  (differs from factory)"
            }
        );
        if !p.name.is_empty() {
            println!("   name        {:?}", p.name);
        }
        println!(
            "   report rate {} Hz ({} ms)",
            p.report_rate_hz(),
            p.report_interval_ms
        );
        let dpis: Vec<String> = p
            .dpi_levels()
            .iter()
            .enumerate()
            .map(|(n, d)| {
                let mut s = d.to_string();
                if n == usize::from(p.default_dpi_index) {
                    s.push_str(" (default)");
                }
                if n == usize::from(p.shift_dpi_index) {
                    s.push_str(" (shift)");
                }
                s
            })
            .collect();
        println!("   DPI         {}", dpis.join(", "));
        println!(
            "   angle snap  {}   power mode {:#04x}",
            p.angle_snapping, p.power_mode
        );
        println!("   buttons");
        for (n, bnd) in p.buttons.iter().enumerate() {
            let l = labels.get(n).map(|s| s.label).unwrap_or("?");
            println!(
                "     slot {n:2}  {l:<18} {:<22} [{}]",
                bnd.to_string(),
                hex4(bnd)
            );
        }
        if p.gshift_buttons.iter().any(|b| *b != Binding::Unassigned) {
            println!("   G-shift buttons");
            for (n, bnd) in p.gshift_buttons.iter().enumerate() {
                if *bnd == Binding::Unassigned {
                    continue;
                }
                let l = labels.get(n).map(|s| s.label).unwrap_or("?");
                println!(
                    "     slot {n:2}  {l:<18} {:<22} [{}]",
                    bnd.to_string(),
                    hex4(bnd)
                );
            }
        }
        for (z, led) in p.leds.iter().enumerate() {
            println!("   LED zone {z}  {}", led.label());
        }
    }

    if raw {
        println!();
        for s in &b.sectors {
            println!(
                "sector {:#06x} crc {}",
                s.sector,
                if s.crc_valid { "ok" } else { "--" }
            );
            if let Some(d) = s.bytes() {
                for (n, c) in d.chunks(16).enumerate() {
                    println!(
                        "  {:03x}: {}",
                        n * 16,
                        c.iter()
                            .map(|x| format!("{x:02x}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    );
                }
            } else if let Some(e) = &s.error {
                println!("  read error: {e}");
            }
        }
    }
    Ok(())
}

fn restore(file: &Path, yes: bool) -> Result<()> {
    let wanted = Backup::load(file)?;
    let mut dev = open()?;
    let current = Backup::read_from(&mut dev)?;
    let writes = memory::restore_plan(&wanted, &current)?;
    let skipped: Vec<String> = wanted
        .sectors
        .iter()
        .filter(|s| s.sector < ROM_BASE && !s.crc_valid)
        .map(|s| format!("{:#06x}", s.sector))
        .collect();
    if !skipped.is_empty() {
        println!(
            "not restorable (no valid CRC in backup, left as is): {}",
            skipped.join(" ")
        );
    }
    if writes.is_empty() {
        println!("mouse already matches the backup; nothing to write");
        return Ok(());
    }
    for w in &writes {
        let changed = w
            .before
            .as_ref()
            .map(|b| b.iter().zip(&w.after).filter(|(x, y)| x != y).count())
            .unwrap_or(w.after.len());
        println!(
            "would write sector {:#06x} ({changed} bytes differ)",
            w.sector
        );
    }
    if !yes {
        println!("dry run: pass --yes to write");
        return Ok(());
    }
    let proof = current.save(&backup::default_backup_dir(), "pre-restore")?;
    println!("backup of current state: {}", proof.path().display());
    let active = current
        .current_profile_sector
        .filter(|s| *s < ROM_BASE)
        .unwrap_or(1);
    memory::apply(&mut dev, &proof, &current.info, &writes, active)?;
    println!(
        "restored and verified {} sector(s); mouse is in onboard mode",
        writes.len()
    );
    Ok(())
}

fn parse_binding(s: &str) -> Result<Binding> {
    let (kind, arg) = s.split_once(':').unwrap_or((s, ""));
    Ok(match kind.trim() {
        "key" => Binding::Key(arg.parse()?),
        "mouse" => Binding::mouse_button(arg.trim().parse()?),
        "consumer" => {
            Binding::Consumer(hidpp::keys::consumer_from_name(arg).context("unknown consumer key")?)
        }
        "function" => Binding::function(Special::from_id(arg.trim()).context("unknown function")?),
        "disabled" => Binding::Disabled,
        _ => bail!("unknown binding {s:?}"),
    })
}

fn set(profile: &str, slot: &str, binding: &str, gshift: bool, yes: bool) -> Result<()> {
    let profile: usize = profile
        .parse()
        .context("PROFILE must be a number (1-based)")?;
    let slot: usize = slot.parse().context("SLOT must be a number (0-based)")?;
    let binding = parse_binding(binding)?;
    let mut dev = open()?;
    let current = Backup::read_from(&mut dev)?;
    let mem = OnboardMemory::from_backup(&current)?;
    let mut edited = mem.clone();
    let p = edited
        .profiles
        .get_mut(profile.wrapping_sub(1))
        .context("no such profile")?;
    let list = if gshift {
        &mut p.profile.gshift_buttons
    } else {
        &mut p.profile.buttons
    };
    *list.get_mut(slot).context("no such slot")? = binding;
    let plan = mem.plan(&edited)?;
    for line in mem.diff(&edited) {
        println!("{line}");
    }
    if plan.is_empty() {
        println!("no change");
        return Ok(());
    }
    for w in &plan {
        println!("would write sector {:#06x}", w.sector);
    }
    if !yes {
        println!("dry run: pass --yes to write");
        return Ok(());
    }
    let proof = current.save(&backup::default_backup_dir(), "pre-write")?;
    println!("backup of current state: {}", proof.path().display());
    let active = current
        .current_profile_sector
        .filter(|s| *s < ROM_BASE)
        .unwrap_or(1);
    memory::apply(&mut dev, &proof, &current.info, &plan, active)?;
    let after = OnboardMemory::from_backup(&Backup::read_from(&mut dev)?)?;
    if after.profiles != edited.profiles {
        bail!("read-back after write does not match the edit");
    }
    println!("written, read back and verified; mouse is in onboard mode");
    Ok(())
}

fn yes(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

fn hex4(b: &Binding) -> String {
    b.encode()
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
