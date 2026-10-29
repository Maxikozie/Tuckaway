import { describe, expect, it } from "vitest";
import { changes } from "./names";
import type { Action, ProfileView } from "./types";

const act = (label: string, binding: Action["binding"]): Action => ({ label, group: "dpi", editable: true, binding });

function view(): ProfileView {
  return {
    index: 3,
    sector: 4,
    enabled: true,
    from_rom: false,
    profile: {
      report_interval_ms: 2,
      default_dpi_index: 1,
      shift_dpi_index: 0,
      dpi: [400, 800, 1600, 0, 0],
      color: "#ffffff",
      power_mode: 0,
      angle_snapping: 0,
      buttons: [{ type: "function", function: "dpi_down" }],
      gshift_buttons: [{ type: "unassigned" }],
      name: "",
      leds: [{ mode: "static", color: "#ffdc00" }],
    },
    actions: {
      buttons: [act("DPI down", { type: "function", function: "dpi_down" })],
      gshift: [act("Not assigned", { type: "unassigned" })],
    },
  };
}

describe("changes", () => {
  it("is empty when nothing changed", () => {
    expect(changes([view()], [view()])).toEqual([]);
  });
  it("describes edits in plain language", () => {
    const w = view();
    w.profile.buttons[0] = { type: "key", keys: "Ctrl + C" };
    w.actions.buttons[0] = act("Copy", { type: "key", keys: "Ctrl + C" });
    w.profile.report_interval_ms = 1;
    w.enabled = false;
    expect(changes([view()], [w]).map((c) => c.text)).toEqual([
      "Disabled",
      "Left button: DPI down → Copy",
      "Report rate: 500 → 1000 Hz",
    ]);
  });
  it("names DPI and lighting changes", () => {
    const w = view();
    w.profile.dpi = [400, 1200, 1600, 0, 0];
    w.profile.leds[0] = { mode: "breathing", color: "#00dcff", period_ms: 8000, brightness: 100 };
    expect(changes([view()], [w]).map((c) => c.text)).toEqual([
      "DPI levels: 400, 800, 1600 → 400, 1200, 1600",
      "Default DPI: 800 → 1200",
      "DPI lights: Static #ffdc00 → Breathing #00dcff",
    ]);
  });
});
