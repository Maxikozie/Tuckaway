<script lang="ts">
  import { onMount, tick } from "svelte";
  import { fly } from "svelte/transition";
  import { api } from "./api";
  import type { Action, Group } from "./types";

  interface Props {
    title: string;
    current: Action | undefined;
    catalog: Action[];
    side: "left" | "right";
    top: number;
    x: number;
    onchoose: (a: Action) => void;
    onclose: () => void;
  }
  let { title, current, catalog, side, top, x, onchoose, onclose }: Props = $props();

  const GROUPS: [Group, string][] = [
    ["mouse", "Mouse"],
    ["shortcuts", "Shortcuts"],
    ["media", "Media"],
    ["dpi", "DPI"],
    ["profiles", "Profiles"],
    ["disabled", "Disabled"],
  ];
  const RECORD = "__record__";

  let query = $state("");
  let cursor = $state(0);
  let recording = $state(false);
  let held = $state<string[]>([]);
  let search: HTMLInputElement;
  let list: HTMLDivElement;
  let panel: HTMLDivElement;

  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  const isCurrent = (a: Action) => !!current && same(a.binding, current.binding);
  const customCurrent = $derived(current?.keys ? current : null);

  let sections = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return GROUPS.map(([g, name]) => {
      const groupHit = q !== "" && name.toLowerCase().includes(q);
      const items: (Action | typeof RECORD)[] = catalog.filter(
        (a) => a.group === g && (q === "" || groupHit || a.label.toLowerCase().includes(q)),
      );
      if (g === "shortcuts" && (q === "" || groupHit || "record shortcut custom keys".includes(q)))
        items.push(RECORD);
      return { g, name, items };
    }).filter((s) => s.items.length > 0);
  });
  let flat = $derived(sections.flatMap((s) => s.items));

  $effect(() => {
    // keep the cursor inside the list as the filter changes
    if (cursor >= flat.length) cursor = Math.max(0, flat.length - 1);
  });

  onMount(() => {
    const i = flat.findIndex((it) => it !== RECORD && isCurrent(it));
    cursor = i >= 0 ? i : 0;
    search.focus();
    tick().then(scrollToCursor);
    const outside = (e: PointerEvent) => {
      if (!panel.contains(e.target as Node)) onclose();
    };
    setTimeout(() => window.addEventListener("pointerdown", outside, true));
    return () => window.removeEventListener("pointerdown", outside, true);
  });

  let first = true;
  function scrollToCursor() {
    list?.querySelector<HTMLElement>(`[data-i="${cursor}"]`)?.scrollIntoView({ block: first ? "center" : "nearest" });
    first = false;
  }

  function activate(it: Action | typeof RECORD) {
    if (it === RECORD) startRecording();
    else onchoose(it);
  }

  function onKey(e: KeyboardEvent) {
    if (recording) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = flat.length;
      cursor = (cursor + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
      tick().then(scrollToCursor);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (flat[cursor] !== undefined) activate(flat[cursor]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    }
  }

  const MODS = ["ControlLeft", "ControlRight", "ShiftLeft", "ShiftRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight"];
  const MOD_TEXT: Record<string, string> = {
    ControlLeft: "Ctrl",
    ControlRight: "Right Ctrl",
    ShiftLeft: "Shift",
    ShiftRight: "Right Shift",
    AltLeft: "Alt",
    AltRight: "AltGr",
    MetaLeft: "Win",
    MetaRight: "Right Win",
  };

  let ctrlAt = -1e9;
  function startRecording() {
    recording = true;
    held = [];
    window.addEventListener("keydown", recDown, true);
    window.addEventListener("keyup", recUp, true);
  }
  function stopRecording() {
    recording = false;
    held = [];
    window.removeEventListener("keydown", recDown, true);
    window.removeEventListener("keyup", recUp, true);
    tick().then(() => search?.focus());
  }
  async function recDown(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.repeat) return;
    if (e.code === "Escape" && held.length === 0) return stopRecording();
    if (e.code === "ControlLeft") ctrlAt = e.timeStamp;
    if (MODS.includes(e.code)) {
      // Windows sends a fake Left Ctrl just before AltGr; drop it.
      if (e.code === "AltRight" && e.timeStamp - ctrlAt < 40) held = held.filter((c) => c !== "ControlLeft");
      if (!held.includes(e.code)) held = [...held, e.code];
      return;
    }
    const a = await api.shortcut([...held, e.code]).catch(() => null);
    stopRecording();
    if (a) onchoose(a);
  }
  function recUp(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    held = held.filter((c) => c !== e.code);
  }
  $effect(() => () => {
    window.removeEventListener("keydown", recDown, true);
    window.removeEventListener("keyup", recUp, true);
  });
</script>

<div
  class="picker {side}"
  style:top="{top}px"
  style:left="{x}px"
  bind:this={panel}
  role="dialog"
  aria-label="Action for {title}"
  tabindex="-1"
  transition:fly={{ x: side === "left" ? -10 : 10, duration: 180 }}
  onkeydown={onKey}
>
  <div class="head">
    <span class="title">{title}</span>
    <input
      bind:this={search}
      bind:value={query}
      placeholder="Search actions"
      aria-label="Search actions"
      spellcheck="false"
      autocomplete="off"
      oninput={() => (cursor = 0)}
    />
  </div>

  <div class="list" bind:this={list} role="listbox" aria-label="Actions">
    {#each sections as s (s.g)}
      <div class="group" role="group" aria-label={s.name}>
        <div class="gname" aria-hidden="true">{s.name}</div>
        {#each s.items as it (it === RECORD ? RECORD : it.label)}
          {@const i = flat.indexOf(it)}
          {#if it === RECORD}
            <button
              class="item record"
              class:cursor={i === cursor}
              class:recording
              data-i={i}
              role="option"
              aria-selected={i === cursor}
              aria-label="Record a shortcut"
              onpointermove={() => (cursor = i)}
              onclick={() => (recording ? stopRecording() : startRecording())}
            >
              {#if recording}
                <span class="rec-dot"></span>
                <span class="rec-text">{held.length ? held.map((c) => MOD_TEXT[c]).join(" + ") + " + …" : "Press a shortcut"}</span>
              {:else if customCurrent}
                <span class="label">{customCurrent.label}</span>
                <span class="hint">Record</span>
                <svg class="check" viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" /></svg>
              {:else}
                <span class="label muted">Record a shortcut</span>
              {/if}
            </button>
          {:else}
            <button
              class="item"
              class:cursor={i === cursor}
              data-i={i}
              role="option"
              aria-selected={isCurrent(it)}
              onpointermove={() => (cursor = i)}
              onclick={() => activate(it)}
            >
              <span class="label">{it.label}</span>
              {#if isCurrent(it)}
                <svg class="check" viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" /></svg>
              {/if}
            </button>
          {/if}
        {/each}
      </div>
    {:else}
      <div class="empty">No action matches “{query}”</div>
    {/each}
  </div>
</div>

<style>
  .picker {
    position: absolute;
    width: 300px;
    max-height: min(470px, calc(100% - 24px));
    display: flex;
    flex-direction: column;
    background: var(--raised);
    border: 1px solid var(--hair);
    border-radius: 12px;
    box-shadow:
      0 18px 50px rgb(0 0 0 / 0.45),
      0 2px 8px rgb(0 0 0 / 0.3);
    z-index: 20;
    overflow: hidden;
  }
  .head {
    padding: 14px 14px 10px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-bottom: 1px solid var(--hair);
  }
  .title {
    font-size: 12px;
    color: var(--text-2);
  }
  input {
    background: var(--raised-2);
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 8px 10px;
    outline: none;
    transition: border-color var(--t) var(--ease);
  }
  input:focus {
    border-color: var(--hair-2);
  }
  input::placeholder {
    color: var(--text-3);
  }
  .list {
    overflow-y: auto;
    padding: 6px;
    scrollbar-width: thin;
    scrollbar-color: rgb(255 255 255 / 0.15) transparent;
  }
  .group + .group {
    margin-top: 6px;
  }
  .gname {
    font-size: 12px;
    color: var(--text-3);
    padding: 8px 10px 4px;
  }
  .item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 7px;
    text-align: left;
    transition: background-color 120ms var(--ease);
  }
  .item.cursor {
    background: rgb(255 255 255 / 0.07);
  }
  .label {
    flex: 1;
  }
  .label.muted,
  .hint {
    color: var(--text-2);
  }
  .hint {
    font-size: 12px;
  }
  .check {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .record.recording {
    background: rgb(255 255 255 / 0.07);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .rec-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--danger);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .rec-text {
    flex: 1;
  }
  .empty {
    padding: 18px 10px;
    color: var(--text-2);
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
</style>
