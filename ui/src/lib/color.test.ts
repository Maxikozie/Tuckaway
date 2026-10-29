import { describe, expect, it } from "vitest";
import { accentFrom, hexToRgb, inkOn, NEUTRAL_ACCENT, oklchToRgb, rgbToHex, rgbToOklch } from "./color";

describe("accent", () => {
  it("falls back to neutral for white, grey and missing colours", () => {
    expect(accentFrom("#ffffff")).toBe(NEUTRAL_ACCENT);
    expect(accentFrom("#808080")).toBe(NEUTRAL_ACCENT);
    expect(accentFrom(null)).toBe(NEUTRAL_ACCENT);
  });
  it("lifts dark LED colours into a readable lightness", () => {
    const [L] = rgbToOklch(hexToRgb(accentFrom("#8000ff")));
    expect(L).toBeGreaterThanOrEqual(0.73);
  });
  it("keeps hue and stays in gamut", () => {
    const a = accentFrom("#ffdc00");
    const [, , h0] = rgbToOklch(hexToRgb("#ffdc00"));
    const [, , h1] = rgbToOklch(hexToRgb(a));
    expect(Math.abs(h0 - h1)).toBeLessThan(4);
    expect(a).toMatch(/^#[0-9a-f]{6}$/);
  });
  it("round-trips through OKLCH", () => {
    for (const hex of ["#00dcff", "#ffdc00", "#8000ff", "#123456"])
      expect(rgbToHex(oklchToRgb(rgbToOklch(hexToRgb(hex))))).toBe(hex);
  });
  it("picks dark ink on light accents", () => {
    expect(inkOn("#ffdc00")).toBe("#17171a");
    expect(inkOn("#3a2a80")).not.toBe("#17171a");
  });
});
