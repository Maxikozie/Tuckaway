// Display names for the button slots of 046d:c08b and the plain-language
// change list shown before Apply.

import type { ProfileView } from "./types";

/** Short names, indexed by slot (hidpp::layout). */
export const BUTTON_NAMES = [
  "Left button",
  "Right button",
  "Wheel click",
  "G4",
  "G5",
  "G6",
  "G7",
  "G8",
  "G9",
  "Wheel right",
  "Wheel left",
];

export const buttonName = (slot: number) => BUTTON_NAMES[slot] ?? `Button ${slot + 1}`;

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

export const hz = (ms: number) => Math.round(1000 / Math.max(1, ms));

export interface Change {
  profile: number;
  text: string;
}

/** Every difference between the loaded and the edited profiles, in words. */
export function changes(base: ProfileView[], work: ProfileView[]): Change[] {
  const out: Change[] = [];
  for (const w of work) {
    const b = base.find((p) => p.index === w.index);
    if (!b) continue;
    const add = (text: string) => out.push({ profile: w.index, text });
    if (b.enabled !== w.enabled) add(w.enabled ? "Enabled" : "Disabled");
    const bp = b.profile;
    const wp = w.profile;
    wp.buttons.forEach((bind, i) => {
      if (!same(bind, bp.buttons[i]))
        add(`${buttonName(i)}: ${b.actions.buttons[i]?.label} → ${w.actions.buttons[i]?.label}`);
    });
    wp.gshift_buttons.forEach((bind, i) => {
      if (!same(bind, bp.gshift_buttons[i]))
        add(`${buttonName(i)} with G-shift: ${b.actions.gshift[i]?.label} → ${w.actions.gshift[i]?.label}`);
    });
    if (bp.report_interval_ms !== wp.report_interval_ms)
      add(`Report rate: ${hz(bp.report_interval_ms)} → ${hz(wp.report_interval_ms)} Hz`);
    const levels = (p: typeof bp) => p.dpi.filter((d) => d > 0).join(", ");
    if (!same(bp.dpi, wp.dpi)) add(`DPI levels: ${levels(bp)} → ${levels(wp)}`);
    const at = (p: typeof bp, i: number) => p.dpi[i] ?? 0;
    if (at(bp, bp.default_dpi_index) !== at(wp, wp.default_dpi_index))
      add(`Default DPI: ${at(bp, bp.default_dpi_index)} → ${at(wp, wp.default_dpi_index)}`);
    if (at(bp, bp.shift_dpi_index) !== at(wp, wp.shift_dpi_index))
      add(`Shift DPI: ${at(bp, bp.shift_dpi_index)} → ${at(wp, wp.shift_dpi_index)}`);
    wp.leds.forEach((led, z) => {
      if (!same(led, bp.leds[z])) add(`${z === 0 ? "DPI lights" : "Logo light"}: ${ledLabel(bp.leds[z])} → ${ledLabel(led)}`);
    });
  }
  return out;
}

export function ledLabel(led: ProfileView["profile"]["leds"][number] | undefined): string {
  if (!led) return "None";
  switch (led.mode) {
    case "off":
      return "Off";
    case "static":
      return `Static ${led.color}`;
    case "breathing":
      return `Breathing ${led.color}`;
    case "cycle":
      return "Colour cycle";
    default:
      return "Custom effect";
  }
}
