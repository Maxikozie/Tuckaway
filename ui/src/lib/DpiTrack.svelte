<script lang="ts">
  import { tick } from "svelte";
  import { fromLevels, fromT, newLevel, SLOTS, snap, toLevels, toT, type Level } from "./dpi";

  interface Props {
    dpi: number[];
    def: number;
    shift: number;
    values: number[];
    hasShift: boolean;
    onchange: (v: { dpi: number[]; def: number; shift: number }) => void;
    onfocusshift?: (on: boolean) => void;
  }
  let { dpi, def, shift, values, hasShift, onchange, onfocusshift }: Props = $props();

  const min = $derived(values[0] ?? 100);
  const max = $derived(values[values.length - 1] ?? 25600);
  const step = $derived(values.length > 1 ? values[1] - values[0] : 50);
  const TICKS = [100, 400, 1600, 6400, 25600];

  let width = $state(600);
  let drag = $state<{ i: number; levels: Level[]; moved: boolean; x0: number } | null>(null);
  let selected = $state<number | null>(null);
  let editing = $state<number | null>(null);
  let draft = $state("");
  let input: HTMLInputElement | undefined = $state();
  let track: HTMLDivElement;

  const committed = $derived(toLevels(dpi, def, shift));
  const levels = $derived(drag ? drag.levels : committed);
  const x = (v: number) => toT(v, min, max) * width;
  // value labels keep at least LABEL px apart even when stops nearly touch
  const LABEL = 52;
  const labelX = $derived.by(() => {
    const xs = levels.map((l, i) => ({ i, x: x(l.v) })).sort((a, b) => a.x - b.x);
    const out = xs.map((p) => p.x);
    for (let k = 1; k < out.length; k++) out[k] = Math.max(out[k], out[k - 1] + LABEL);
    const over = out.length ? out[out.length - 1] - Math.max(width, xs[xs.length - 1].x) : 0;
    if (over > 0) for (let k = out.length - 1; k >= 0; k--) out[k] = Math.min(out[k], (k === out.length - 1 ? out[k] - over : out[k + 1] - LABEL));
    const byIndex: number[] = [];
    xs.forEach((p, k) => (byIndex[p.i] = out[k] - p.x));
    return byIndex;
  });
  const span = $derived.by(() => {
    if (!levels.length) return { a: 0, b: 0 };
    const vs = levels.map((l) => l.v);
    return { a: x(Math.min(...vs)), b: x(Math.max(...vs)) };
  });

  function commit(ls: Level[], keep?: Level) {
    const r = fromLevels(ls);
    onchange(r);
    // keep the same level selected after sorting
    if (keep) selected = [...ls].sort((a, b) => a.v - b.v).indexOf(keep);
  }

  function down(e: PointerEvent, i: number) {
    if (editing !== null) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { i, levels: committed.map((l) => ({ ...l })), moved: false, x0: e.clientX };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    if (Math.abs(e.clientX - drag.x0) > 3) drag.moved = true;
    if (!drag.moved) return;
    const r = track.getBoundingClientRect();
    drag.levels[drag.i].v = snap(fromT((e.clientX - r.left) / r.width, min, max), values);
  }
  function up() {
    if (!drag) return;
    const d = drag;
    drag = null;
    if (d.moved) commit(d.levels, d.levels[d.i]);
    else startEdit(d.i);
  }

  async function startEdit(i: number) {
    selected = i;
    editing = i;
    draft = String(committed[i].v);
    await tick();
    input?.focus();
    input?.select();
  }
  function finishEdit(save: boolean) {
    if (editing === null) return;
    const i = editing;
    editing = null;
    const n = Number(draft.replace(/[^\d]/g, ""));
    if (save && n > 0) {
      const ls = committed.map((l) => ({ ...l }));
      ls[i].v = snap(n, values);
      commit(ls, ls[i]);
    }
    tick().then(() => track.querySelector<HTMLElement>(`[data-stop="${selected}"]`)?.focus());
  }

  function nudge(e: KeyboardEvent, i: number) {
    const ls = committed.map((l) => ({ ...l }));
    const big = e.shiftKey ? 10 : 1;
    let v = ls[i].v;
    switch (e.key) {
      case "ArrowRight":
      case "ArrowUp":
        v += step * big;
        break;
      case "ArrowLeft":
      case "ArrowDown":
        v -= step * big;
        break;
      case "PageUp":
        v += step * 10;
        break;
      case "PageDown":
        v -= step * 10;
        break;
      case "Home":
        v = min;
        break;
      case "End":
        v = max;
        break;
      case "Enter":
        e.preventDefault();
        startEdit(i);
        return;
      case "Delete":
      case "Backspace":
        e.preventDefault();
        remove(i);
        return;
      default:
        return;
    }
    e.preventDefault();
    ls[i].v = snap(v, values);
    commit(ls, ls[i]);
    tick().then(() => track.querySelector<HTMLElement>(`[data-stop="${selected}"]`)?.focus());
  }

  function add() {
    if (committed.length >= SLOTS) return;
    const l: Level = { v: newLevel(committed, min, max, values), def: false, shift: false };
    commit([...committed.map((c) => ({ ...c })), l], l);
  }
  function remove(i: number) {
    if (committed.length <= 1) return;
    const ls = committed.map((l) => ({ ...l }));
    const [gone] = ls.splice(i, 1);
    // a removed default or shift level hands its role to the nearest level
    const near = ls.reduce((a, b) => (Math.abs(b.v - gone.v) < Math.abs(a.v - gone.v) ? b : a));
    if (gone.def) near.def = true;
    if (gone.shift) near.shift = true;
    selected = null;
    onchange(fromLevels(ls));
  }
  function setRole(i: number, role: "def" | "shift") {
    const ls = committed.map((l, k) => ({ ...l, [role]: k === i }));
    commit(ls, ls[i]);
  }

  const sel = $derived(selected !== null && selected < committed.length ? committed[selected] : null);
</script>

<div class="dpi">
  <div class="track" bind:this={track} bind:clientWidth={width}>
    <div class="rail"></div>
    <div class="fill" style:left="{span.a}px" style:width="{span.b - span.a}px"></div>

    {#each TICKS as t (t)}
      <span class="tick num" style:left="{x(t)}px">{t}</span>
    {/each}

    {#each levels as l, i (i)}
      {@const isSel = selected === i}
      <div class="stop-wrap" class:dragging={drag?.i === i && drag.moved} style:left="{x(l.v)}px">
        <span class="label" style:translate="{labelX[i] ?? 0}px 0">
        <span class="tags">
          {#if l.def}<span class="tag def">Default</span>{/if}
          {#if hasShift && l.shift}<span class="tag">Shift</span>{/if}
        </span>
        {#if editing === i}
          <input
            bind:this={input}
            bind:value={draft}
            class="value num"
            inputmode="numeric"
            aria-label="DPI value"
            onkeydown={(e) => {
              if (e.key === "Enter") finishEdit(true);
              if (e.key === "Escape") finishEdit(false);
            }}
            onblur={() => finishEdit(true)}
          />
        {:else}
          <span class="value num">{l.v}</span>
        {/if}
        </span>
        <button
          class="stop"
          class:def={l.def}
          class:sel={isSel}
          data-stop={i}
          role="slider"
          aria-label="DPI level {i + 1}{l.def ? ', default' : ''}{hasShift && l.shift ? ', shift' : ''}"
          aria-valuemin={min}
          aria-valuemax={max}
          aria-valuenow={l.v}
          onpointerdown={(e) => down(e, i)}
          onpointermove={move}
          onpointerup={up}
          onpointercancel={() => (drag = null)}
          onfocus={() => (selected = i)}
          onkeydown={(e) => nudge(e, i)}
        ></button>
      </div>
    {/each}
  </div>

  <div class="row">
    <div class="sel-actions">
      {#if sel && selected !== null}
        {@const i = selected}
        <button class="link" disabled={sel.def} onclick={() => setRole(i, "def")}>{sel.def ? "Default level" : "Make default"}</button>
        {#if hasShift}
          <button
            class="link"
            disabled={sel.shift}
            onclick={() => setRole(i, "shift")}
            onpointerenter={() => onfocusshift?.(true)}
            onpointerleave={() => onfocusshift?.(false)}>{sel.shift ? "Shift level" : "Use for DPI shift"}</button
          >
        {/if}
        <button class="link" disabled={committed.length <= 1} onclick={() => remove(i)}>Remove</button>
      {/if}
    </div>
    <button class="link add" disabled={committed.length >= SLOTS} onclick={add}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3.5v9M3.5 8h9" /></svg>
      Add level
    </button>
  </div>
</div>

<style>
  .dpi {
    padding: 0 10px;
  }
  .track {
    position: relative;
    height: 112px;
    touch-action: none;
  }
  .rail,
  .fill {
    position: absolute;
    top: 64px;
    height: 4px;
    border-radius: 4px;
  }
  .rail {
    left: 0;
    right: 0;
    background: rgb(255 255 255 / 0.09);
  }
  .fill {
    background: rgb(255 255 255 / 0.22);
    transition:
      left 160ms var(--ease),
      width 160ms var(--ease);
  }
  .tick {
    position: absolute;
    top: 86px;
    translate: -50% 0;
    font-size: var(--fs-cap);
    color: var(--text-3);
  }
  .tick:first-of-type {
    translate: 0 0;
  }
  .tick:last-of-type {
    translate: -100% 0;
  }
  .stop-wrap {
    position: absolute;
    top: 0;
    width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    transition: left 160ms var(--ease);
  }
  .stop-wrap.dragging {
    transition: none;
    z-index: 2;
  }
  .label {
    display: flex;
    flex-direction: column;
    align-items: center;
    transition: translate 160ms var(--ease);
  }
  .tags {
    height: 18px;
    display: flex;
    gap: 10px;
    white-space: nowrap;
  }
  .tag {
    font-size: 11.5px;
    color: var(--text-2);
  }
  .tag.def {
    color: var(--accent);
  }
  .value {
    height: 22px;
    min-width: 56px;
    text-align: center;
    font-size: var(--fs-action);
    font-weight: 560;
    white-space: nowrap;
  }
  input.value {
    width: 72px;
    padding: 0 6px;
    border: 1px solid var(--hair-2);
    border-radius: 6px;
    background: var(--raised-2);
    outline: none;
    margin-top: -1px;
  }
  .stop {
    margin-top: 13px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #f2f0eb;
    box-shadow:
      0 0 0 3px var(--bg),
      0 2px 6px rgb(0 0 0 / 0.5);
    cursor: grab;
    transition:
      box-shadow var(--t) var(--ease),
      background-color var(--t) var(--ease),
      scale var(--t) var(--ease);
  }
  .stop:hover {
    scale: 1.12;
  }
  .stop.def {
    background: var(--accent);
  }
  .stop.sel,
  .stop:focus-visible {
    outline: none;
    box-shadow:
      0 0 0 3px var(--bg),
      0 0 0 5px var(--accent);
  }
  .dragging .stop {
    cursor: grabbing;
    scale: 1.18;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    min-height: 32px;
    margin: 4px -10px 0;
  }
  .sel-actions {
    display: flex;
    gap: 2px;
  }
  .link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    border-radius: 8px;
    color: var(--text-2);
    font-weight: 520;
    transition:
      color var(--t) var(--ease),
      background-color var(--t) var(--ease);
  }
  .link:hover:not(:disabled) {
    color: var(--text);
    background: rgb(255 255 255 / 0.06);
  }
  .link:disabled {
    color: var(--text-3);
    cursor: default;
  }
  .add svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
  }
</style>
