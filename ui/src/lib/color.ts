// Colour helpers: LED colours for display and the app accent derived from
// them. OKLab/OKLCH per Björn Ottosson.

import type { Led } from "./types";

type Rgb = [number, number, number];

export function hexToRgb(hex: string): Rgb {
  const h = hex.replace("#", "");
  const n = parseInt(h.length === 3 ? h.replace(/./g, "$&$&") : h, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function rgbToHex([r, g, b]: Rgb): string {
  const c = (v: number) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, "0");
  return `#${c(r)}${c(g)}${c(b)}`;
}

const toLin = (c: number) => {
  c /= 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
};
const fromLin = (c: number) => 255 * (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);

export function rgbToOklch(rgb: Rgb): [number, number, number] {
  const [r, g, b] = rgb.map(toLin);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return [L, Math.hypot(A, B), ((Math.atan2(B, A) * 180) / Math.PI + 360) % 360];
}

export function oklchToRgb([L, C, H]: [number, number, number]): Rgb {
  const a = C * Math.cos((H * Math.PI) / 180);
  const b = C * Math.sin((H * Math.PI) / 180);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    fromLin(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    fromLin(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    fromLin(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ];
}

function inGamut(rgb: Rgb) {
  return rgb.every((v) => v >= -0.5 && v <= 255.5);
}

/** Neutral accent for white, grey, off and colour-cycle profiles. */
export const NEUTRAL_ACCENT = "#e6e2d9";

/**
 * Accent from an LED colour: lightness clamped so it reads on the dark
 * stage, chroma capped so saturated LEDs do not glare. Near-greys fall back to
 * the neutral accent.
 */
export function accentFrom(hex: string | null): string {
  if (!hex) return NEUTRAL_ACCENT;
  const [L, C, H] = rgbToOklch(hexToRgb(hex));
  if (C < 0.035) return NEUTRAL_ACCENT;
  let l = Math.min(0.86, Math.max(0.74, L));
  let c = Math.min(0.16, C);
  // pull chroma in until the colour fits sRGB
  while (c > 0 && !inGamut(oklchToRgb([l, c, H]))) c -= 0.005;
  return rgbToHex(oklchToRgb([l, c, H]));
}

/** The colour an LED effect shows, or null for off / cycle / unknown. */
export function ledColor(led: Led | undefined): string | null {
  if (!led) return null;
  if (led.mode === "static" || led.mode === "breathing") return led.color;
  return null;
}

/** Readable text on top of `hex` (for the accent-filled Apply button). */
export function inkOn(hex: string): string {
  return rgbToOklch(hexToRgb(hex))[0] > 0.66 ? "#17171a" : "#f5f3ee";
}
