import { describe, expect, it } from "vitest";
import { fromLevels, fromT, newLevel, snap, toLevels, toT } from "./dpi";

const values = Array.from({ length: 511 }, (_, i) => 100 + i * 50);

describe("dpi", () => {
  it("reads the leading non-zero slots", () => {
    expect(toLevels([400, 800, 1600, 0, 0], 1, 0).map((l) => l.v)).toEqual([400, 800, 1600]);
    expect(toLevels([400, 0, 1600, 0, 0], 0, 0).length).toBe(1);
  });
  it("sorts and keeps default and shift on their level", () => {
    const l = toLevels([400, 800, 1600, 0, 0], 2, 0);
    l[0].v = 3200; // the shift level moves past the others
    expect(fromLevels(l)).toEqual({ dpi: [800, 1600, 3200, 0, 0], def: 1, shift: 2 });
  });
  it("snaps to the sensor's steps", () => {
    expect(snap(1234, values)).toBe(1250);
    expect(snap(20, values)).toBe(100);
    expect(snap(99999, values)).toBe(25600);
  });
  it("uses a log scale", () => {
    expect(toT(100, 100, 25600)).toBe(0);
    expect(toT(25600, 100, 25600)).toBeCloseTo(1);
    expect(toT(1600, 100, 25600)).toBeCloseTo(0.5);
    expect(fromT(0.5, 100, 25600)).toBeCloseTo(1600);
  });
  it("adds a level in the widest gap", () => {
    const v = newLevel(toLevels([400, 800, 1600, 0, 0], 0, 0), 100, 25600, values);
    expect(v).toBeGreaterThan(1600);
  });
});
