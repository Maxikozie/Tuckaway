// The mouse illustration. `mouse.svg` is the traced drawing without the
// manufacturer logo (the logo LED zone is a round light window) and is what
// public builds ship. A local build may use assets/local/mouse.svg instead,
// the same drawing with the logo, which is never committed.
// Both share one viewBox, the same `data-slot` hit groups and LED groups.

import committed from "./mouse.svg?raw";

const local = import.meta.glob("../../../assets/local/mouse.svg", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

// Public builds (VITE_PUBLIC=1) never ship the local drawing.
export const mouseArt: string =
  import.meta.env.VITE_PUBLIC === "1" ? committed : (Object.values(local)[0] ?? committed);

export const VIEWBOX = { x: 880, y: 110, w: 700, h: 1190 };

/** Where each slot's callout line ends, in viewBox units. */
export const ANCHORS: Record<number, [number, number]> = {
  0: [1128, 330],
  1: [1398, 330],
  2: [1268, 468],
  3: [1024, 826],
  4: [995, 640],
  5: [969, 668],
  6: [1036, 532],
  7: [1040, 352],
  8: [1272, 664],
  9: [1348, 424],
  10: [1183, 424],
};

/** Slots whose callouts sit left of the mouse (the thumb side and G7/G8). */
export const LEFT_SLOTS = [0, 7, 10, 6, 4, 5, 3];
export const RIGHT_SLOTS = [1, 2, 9, 8];
