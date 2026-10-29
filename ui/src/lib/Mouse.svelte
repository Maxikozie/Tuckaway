<script lang="ts" module>
  export interface Geom {
    /** viewBox point → px, relative to the mouse element's offset parent */
    x: (vx: number) => number;
    y: (vy: number) => number;
    left: number;
    right: number;
    top: number;
    bottom: number;
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { mouseArt, VIEWBOX } from "./art";
  import type { Led } from "./types";

  interface Props {
    hovered?: number | null;
    selected?: number | null;
    leds?: (Led | undefined)[];
    zoneSelected?: number | null;
    zonesInteractive?: boolean;
    slotsInteractive?: boolean;
    dim?: boolean;
    geom?: Geom | null;
    onhover?: (slot: number | null) => void;
    onpick?: (slot: number) => void;
    onzone?: (zone: number) => void;
  }

  let {
    hovered = null,
    selected = null,
    leds = [],
    zoneSelected = null,
    zonesInteractive = false,
    slotsInteractive = true,
    dim = false,
    geom = $bindable(null),
    onhover,
    onpick,
    onzone,
  }: Props = $props();

  let host: HTMLDivElement;
  let groups: SVGGElement[] = [];
  let zones: SVGGElement[] = [];

  function measure() {
    const svg = host?.querySelector("svg");
    if (!svg) return;
    const w = host.clientWidth;
    const h = host.clientHeight;
    const s = Math.min(w / VIEWBOX.w, h / VIEWBOX.h);
    const ox = host.offsetLeft + (w - VIEWBOX.w * s) / 2;
    const oy = host.offsetTop + (h - VIEWBOX.h * s) / 2;
    const x = (vx: number) => ox + (vx - VIEWBOX.x) * s;
    const y = (vy: number) => oy + (vy - VIEWBOX.y) * s;
    // the mouse body spans roughly x 904..1529, y 148..1244 in the viewBox
    geom = { x, y, left: x(904), right: x(1529), top: y(148), bottom: y(1244) };
  }

  onMount(() => {
    // highlight layer: a copy of each button's or zone's own shape
    const addHighlight = (g: SVGGElement) => {
      const shape = g.querySelector(":scope > use, :scope > path, :scope > rect, :scope > circle");
      if (!shape) return;
      const hl = shape.cloneNode() as SVGElement;
      hl.removeAttribute("id");
      hl.setAttribute("class", "hl");
      g.appendChild(hl);
    };
    groups = Array.from(host.querySelectorAll<SVGGElement>("g[data-slot]"));
    for (const g of groups) {
      addHighlight(g);
      const slot = Number(g.dataset.slot);
      g.addEventListener("pointerenter", () => slotsInteractive && onhover?.(slot));
      g.addEventListener("pointerleave", () => slotsInteractive && onhover?.(null));
      g.addEventListener("click", () => slotsInteractive && onpick?.(slot));
    }
    zones = Array.from(host.querySelectorAll<SVGGElement>("g[data-zone]"));
    for (const z of zones) {
      addHighlight(z);
      const zone = Number(z.dataset.zone);
      z.addEventListener("click", () => zonesInteractive && onzone?.(zone));
    }
    const ro = new ResizeObserver(measure);
    ro.observe(host);
    measure();
    return () => ro.disconnect();
  });

  $effect(() => {
    for (const g of groups) {
      const s = Number(g.dataset.slot);
      g.classList.toggle("is-hover", s === hovered);
      g.classList.toggle("is-selected", s === selected);
      g.classList.toggle("is-live", slotsInteractive);
    }
  });

  $effect(() => {
    for (const z of zones) {
      const i = Number(z.dataset.zone);
      const led = leds[i];
      const el = z as unknown as HTMLElement;
      let on = 0;
      let color = "#000";
      let mode = "off";
      let period = 4000;
      if (led && (led.mode === "static" || led.mode === "breathing")) {
        on = Math.max(0.15, (led.mode === "breathing" ? led.brightness : 100) / 100);
        color = led.color;
        mode = led.mode;
        if (led.mode === "breathing") period = led.period_ms;
      } else if (led && led.mode === "cycle") {
        on = Math.max(0.15, led.brightness / 100);
        color = "#ff3b30";
        mode = "cycle";
        period = led.period_ms;
      }
      el.style.setProperty("--led-on", String(on));
      el.style.setProperty("--led-color", color);
      el.style.setProperty("--led-period", `${Math.max(500, period)}ms`);
      z.classList.toggle("fx-breathing", mode === "breathing");
      z.classList.toggle("fx-cycle", mode === "cycle");
      z.classList.toggle("is-selected", zoneSelected === i);
      z.classList.toggle("is-dimmed", zoneSelected !== null && zoneSelected !== i);
      z.classList.toggle("is-live", zonesInteractive);
    }
  });
</script>

<div class="mouse" class:dim bind:this={host} aria-hidden="true">
  {@html mouseArt}
</div>

<style>
  .mouse {
    position: absolute;
    inset: 0;
    transition: opacity 300ms var(--ease), filter 300ms var(--ease);
  }
  .mouse.dim {
    opacity: 0.35;
    filter: saturate(0.4);
  }
  .mouse :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
    overflow: visible;
  }
  .mouse :global(g.is-live[data-slot]) {
    cursor: pointer;
  }
  .mouse :global(g[data-slot] > .hl),
  .mouse :global(g.led > .hl) {
    fill: var(--accent);
    fill-opacity: 0;
    stroke: var(--accent);
    stroke-opacity: 0;
    stroke-width: 2.5;
    stroke-linejoin: round;
    transition:
      fill-opacity var(--t) var(--ease),
      stroke-opacity var(--t) var(--ease);
    pointer-events: none;
  }
  .mouse :global(g.is-hover > .hl) {
    fill-opacity: 0.16;
    stroke-opacity: 0.55;
  }
  .mouse :global(g.is-selected > .hl) {
    fill-opacity: 0.24;
    stroke-opacity: 0.95;
  }
  .mouse :global(g.led .led-glow) {
    transition:
      opacity 400ms var(--ease),
      color 250ms var(--ease);
  }
  .mouse :global(g.led.is-live) {
    cursor: pointer;
  }
  .mouse :global(g.led > .hl) {
    fill: none;
    stroke-width: 3;
  }
  .mouse :global(g.led.is-live:hover > .hl) {
    stroke-opacity: 0.45;
  }
  .mouse :global(g.led > .hl) {
    stroke: #f2f0eb;
  }
  .mouse :global(g.led.is-selected > .hl) {
    stroke-opacity: 0.7;
  }
  .mouse :global(g.led.is-dimmed .led-glow) {
    opacity: calc(var(--led-on) * 0.28);
  }
  .mouse :global(g.led.is-dimmed.fx-breathing .led-glow),
  .mouse :global(g.led.is-dimmed.fx-cycle .led-glow) {
    animation-play-state: paused;
  }
  .mouse :global(g.led.is-selected .led-glow-blur) {
    filter: url(#f-glow) brightness(1.15);
  }
  .mouse :global(g.led.fx-breathing .led-glow) {
    animation: breathe var(--led-period) ease-in-out infinite;
  }
  .mouse :global(g.led.fx-cycle .led-glow) {
    animation: cycle var(--led-period) linear infinite;
  }
  @keyframes breathe {
    0%,
    100% {
      opacity: calc(var(--led-on) * 0.12);
    }
    50% {
      opacity: var(--led-on);
    }
  }
  @keyframes cycle {
    0%,
    100% {
      color: #ff3b30;
    }
    17% {
      color: #ffd23a;
    }
    33% {
      color: #3dff6a;
    }
    50% {
      color: #2fe0ff;
    }
    67% {
      color: #3a5cff;
    }
    83% {
      color: #d43aff;
    }
  }
</style>
