<script lang="ts">
  import { ANCHORS, LEFT_SLOTS, RIGHT_SLOTS } from "./art";
  import { buttonName } from "./names";
  import type { Geom } from "./Mouse.svelte";
  import type { Action } from "./types";

  interface Props {
    geom: Geom | null;
    height: number;
    actions: Action[];
    hovered: number | null;
    selected: number | null;
    coveredSide: "left" | "right" | null;
    layerKey: string;
    onhover: (slot: number | null) => void;
    onpick: (slot: number, side: "left" | "right", el: HTMLElement) => void;
  }
  let { geom, height, actions, hovered, selected, coveredSide, layerKey, onhover, onpick }: Props = $props();

  const big = $derived(height > 900);
  const ROW = $derived(big ? 58 : 50);
  const GAP = $derived(big ? 96 : 76);

  /** Spread labels so none overlap, keeping each as close to its button as possible. */
  function spread(want: number[], lo: number, hi: number): number[] {
    const y = [...want];
    for (let i = 1; i < y.length; i++) y[i] = Math.max(y[i], y[i - 1] + ROW);
    if (y.length && y[y.length - 1] > hi) {
      y[y.length - 1] = hi;
      for (let i = y.length - 2; i >= 0; i--) y[i] = Math.min(y[i], y[i + 1] - ROW);
    }
    if (y.length && y[0] < lo) {
      y[0] = lo;
      for (let i = 1; i < y.length; i++) y[i] = Math.max(y[i], y[i - 1] + ROW);
    }
    return y;
  }

  function column(slots: number[], side: "left" | "right") {
    if (!geom) return [];
    const g = geom;
    const sorted = [...slots].sort((a, b) => ANCHORS[a][1] - ANCHORS[b][1]);
    const ys = spread(
      sorted.map((s) => g.y(ANCHORS[s][1])),
      40,
      height - 40,
    );
    const edge = side === "left" ? g.left - GAP : g.right + GAP;
    return sorted.map((slot, i) => {
      const ax = g.x(ANCHORS[slot][0]);
      const ay = g.y(ANCHORS[slot][1]);
      const ly = ys[i];
      const elbow = side === "left" ? g.left - 20 : g.right + 20;
      const start = side === "left" ? edge + 12 : edge - 12;
      return { slot, side, ly, edge, ax, ay, d: `M${start} ${ly}H${elbow}L${ax} ${ay}` };
    });
  }

  let items = $derived([...column(LEFT_SLOTS, "left"), ...column(RIGHT_SLOTS, "right")].filter((it) => actions[it.slot]));
</script>

{#if geom}
  <svg class="leaders" aria-hidden="true">
    {#each items as it (it.slot)}
      {@const on = it.slot === hovered || it.slot === selected}
      <path d={it.d} class:on class:gone={it.side === coveredSide && it.slot !== selected} />
      <circle cx={it.ax} cy={it.ay} r={on ? 4.5 : 3} class:on class:gone={it.side === coveredSide && it.slot !== selected} />
    {/each}
  </svg>

  {#each items as it (it.slot)}
    {@const a = actions[it.slot]}
    <button
      class="callout {it.side}"
      class:on={it.slot === hovered}
      class:selected={it.slot === selected}
      class:gone={it.side === coveredSide}
      tabindex={it.side === coveredSide ? -1 : 0}
      style:top="{it.ly}px"
      style={it.side === "left" ? `right: calc(100% - ${it.edge}px)` : `left: ${it.edge}px`}
      aria-label="{buttonName(it.slot)}: {a.label}"
      aria-haspopup="dialog"
      onpointerenter={() => onhover(it.slot)}
      onpointerleave={() => onhover(null)}
      onfocus={() => onhover(it.slot)}
      onblur={() => onhover(null)}
      onclick={(e) => onpick(it.slot, it.side, e.currentTarget)}
    >
      {#key layerKey + a.label}
        <span class="action" class:keys={!!a.keys} class:quiet={a.binding.type === "unassigned"}>{a.label}</span>
      {/key}
      <span class="name">{buttonName(it.slot)}</span>
    </button>
  {/each}
{/if}

<style>
  .leaders {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    overflow: visible;
  }
  .leaders path {
    fill: none;
    stroke: rgb(255 255 255 / 0.2);
    stroke-width: 1;
    transition: stroke var(--t) var(--ease);
  }
  .leaders circle {
    fill: var(--bg);
    stroke: rgb(255 255 255 / 0.55);
    stroke-width: 1.25;
    transition:
      r var(--t) var(--ease),
      stroke var(--t) var(--ease),
      fill var(--t) var(--ease);
  }
  .leaders .gone,
  .callout.gone {
    opacity: 0;
    pointer-events: none;
  }
  .leaders path,
  .leaders circle,
  .callout {
    transition-property: opacity, stroke, fill, r, top, background-color;
    transition-duration: var(--t);
    transition-timing-function: var(--ease);
  }
  .leaders path.on {
    stroke: var(--accent);
  }
  .leaders circle.on {
    stroke: var(--accent);
    fill: var(--accent);
  }

  .callout {
    position: absolute;
    transform: translateY(-50%);
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px 10px;
    margin: 0 -10px;
    border-radius: var(--r-md);
    max-width: 230px;
    transition:
      top 220ms var(--ease),
      background-color var(--t) var(--ease);
  }
  .callout.left {
    align-items: flex-end;
    text-align: right;
  }
  .callout.right {
    align-items: flex-start;
    text-align: left;
  }
  .callout:hover,
  .callout.on {
    background: rgb(255 255 255 / 0.045);
  }
  .action {
    font-size: var(--fs-action);
    font-weight: 560;
    letter-spacing: -0.005em;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
    animation: swap 220ms var(--ease);
    transition: color var(--t) var(--ease);
  }
  .action.quiet {
    color: var(--text-3);
    font-weight: 480;
  }
  .action.keys {
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .name {
    font-size: var(--fs-cap);
    color: var(--text-2);
  }
  .callout.on .action,
  .callout.selected .action {
    color: var(--accent);
  }
  @keyframes swap {
    from {
      opacity: 0;
      transform: translateY(3px);
    }
  }
</style>
