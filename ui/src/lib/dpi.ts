// DPI levels as the track edits them. A profile has 5 slots; the levels are
// the leading non-zero ones. Default and shift are slot indices.

export interface Level {
  v: number;
  def: boolean;
  shift: boolean;
}

export const SLOTS = 5;

export function toLevels(dpi: number[], def: number, shift: number): Level[] {
  const out: Level[] = [];
  for (let i = 0; i < dpi.length && dpi[i] > 0; i++) out.push({ v: dpi[i], def: i === def, shift: i === shift });
  return out;
}

/** Sorted ascending, padded to 5 slots; default and shift follow their level. */
export function fromLevels(levels: Level[]): { dpi: number[]; def: number; shift: number } {
  const sorted = [...levels].sort((a, b) => a.v - b.v);
  const dpi = sorted.map((l) => l.v);
  while (dpi.length < SLOTS) dpi.push(0);
  const def = Math.max(0, sorted.findIndex((l) => l.def));
  const s = sorted.findIndex((l) => l.shift);
  return { dpi, def, shift: s < 0 ? def : s };
}

/** Nearest value the sensor supports. `values` is sorted ascending. */
export function snap(v: number, values: number[]): number {
  if (!values.length) return Math.round(v);
  let lo = 0;
  let hi = values.length - 1;
  while (hi - lo > 1) {
    const mid = (lo + hi) >> 1;
    if (values[mid] < v) lo = mid;
    else hi = mid;
  }
  return Math.abs(values[lo] - v) <= Math.abs(values[hi] - v) ? values[lo] : values[hi];
}

/** Log position in 0..1, so 100–800 gets as much room as 3200–25600. */
export const toT = (v: number, min: number, max: number) => Math.log(v / min) / Math.log(max / min);
export const fromT = (t: number, min: number, max: number) => min * (max / min) ** Math.min(1, Math.max(0, t));

/** Where a new level goes: the middle of the widest gap on the log scale. */
export function newLevel(levels: Level[], min: number, max: number, values: number[]): number {
  const ts = [0, ...levels.map((l) => toT(l.v, min, max)).sort((a, b) => a - b), 1];
  let best = 0;
  let at = 0.5;
  for (let i = 1; i < ts.length; i++) {
    if (ts[i] - ts[i - 1] > best) {
      best = ts[i] - ts[i - 1];
      at = (ts[i] + ts[i - 1]) / 2;
    }
  }
  return snap(fromT(at, min, max), values);
}
