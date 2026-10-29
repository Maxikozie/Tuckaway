<script lang="ts">
  import { fade } from "svelte/transition";
  import { app } from "./state.svelte";
  import Menu, { type MenuItem } from "./Menu.svelte";
  import type { ProfileView, Tab } from "./types";

  interface Props {
    onapply: () => void;
    settings: MenuItem[];
  }
  let { onapply, settings }: Props = $props();

  const TABS: [Tab, string][] = [
    ["buttons", "Buttons"],
    ["performance", "Performance"],
    ["lighting", "Lighting"],
  ];

  let menu = $state<{ items: MenuItem[]; x: number; y: number; align: "left" | "right"; label: string } | null>(null);
  let tabEls: HTMLButtonElement[] = $state([]);
  let bar = $state({ x: 0, w: 0 });

  $effect(() => {
    const i = TABS.findIndex(([t]) => t === app.tab);
    const el = tabEls[i];
    if (el) bar = { x: el.offsetLeft, w: el.offsetWidth };
  });

  function dot(p: ProfileView): string {
    const led = p.profile.leds[0];
    if (!led || led.mode === "off") return "transparent";
    if (led.mode === "cycle") return "conic-gradient(#ff5a5a, #ffd75a, #5aff8c, #5ac8ff, #b45aff, #ff5a5a)";
    if (led.mode === "static" || led.mode === "breathing") return led.color;
    return "#888";
  }

  function profileItems(p: ProfileView): MenuItem[] {
    const enabledCount = app.work.filter((x) => x.enabled).length;
    const isActive = p.sector === app.activeSector;
    return [
      {
        label: isActive ? "Active on the mouse" : "Make active",
        disabled: isActive || !p.enabled || !app.base.find((b) => b.index === p.index)?.enabled,
        action: () => app.activate(p.index),
      },
      p.enabled
        ? { label: "Disable", disabled: enabledCount <= 1 || isActive, action: () => app.setEnabled(p.index, false) }
        : { label: "Enable", action: () => app.setEnabled(p.index, true) },
    ];
  }

  function openProfileMenu(p: ProfileView, x: number, y: number) {
    menu = { items: profileItems(p), x, y, align: "left", label: `Profile ${p.index + 1}` };
  }

  function pillKey(e: KeyboardEvent, p: ProfileView) {
    if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
      e.preventDefault();
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      openProfileMenu(p, r.left, r.bottom + 6);
    } else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const n = app.work.length;
      app.selected = (app.selected + (e.key === "ArrowRight" ? 1 : n - 1)) % n;
      document.querySelector<HTMLElement>(`[data-pill="${app.selected}"]`)?.focus();
    }
  }
</script>

<header>
  <div class="left">
    {#if app.status === "connected"}
      <div class="pills" role="tablist" aria-label="Profiles">
        {#each app.work as p (p.index)}
          {@const active = p.sector === app.activeSector}
          <button
            class="pill"
            class:selected={p.index === app.selected}
            class:off={!p.enabled}
            class:active
            role="tab"
            data-pill={p.index}
            aria-selected={p.index === app.selected}
            tabindex={p.index === app.selected ? 0 : -1}
            aria-label="Profile {p.index + 1}{active ? ', active on the mouse' : ''}{p.enabled ? '' : ', disabled'}"
            title={active ? "Active on the mouse" : p.enabled ? undefined : "Disabled"}
            onclick={() => (app.selected = p.index)}
            oncontextmenu={(e) => {
              e.preventDefault();
              app.selected = p.index;
              openProfileMenu(p, e.clientX, e.clientY);
            }}
            onkeydown={(e) => pillKey(e, p)}
          >
            <span class="n num">{p.index + 1}</span>
            <span class="dot" style:background={dot(p)} class:hollow={dot(p) === "transparent"}></span>
          </button>
        {/each}
        <button
          class="more"
          aria-label="Profile {app.selected + 1} options"
          aria-haspopup="menu"
          onclick={(e) => {
            const p = app.current;
            const r = e.currentTarget.getBoundingClientRect();
            if (p) openProfileMenu(p, r.left, r.bottom + 6);
          }}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="3.5" cy="8" r="1.4" /><circle cx="8" cy="8" r="1.4" /><circle cx="12.5" cy="8" r="1.4" /></svg>
        </button>
      </div>
    {/if}
  </div>

  <div class="tabs" role="tablist" aria-label="Sections">
    {#each TABS as [t, label], i (t)}
      <button
        bind:this={tabEls[i]}
        role="tab"
        aria-selected={app.tab === t}
        class:on={app.tab === t}
        onclick={() => (app.tab = t)}>{label}</button
      >
    {/each}
    <span class="bar" style:translate="{bar.x}px 0" style:width="{bar.w}px"></span>
  </div>

  <div class="right">
    {#if app.dirty}
      <div class="pending" transition:fade={{ duration: 160 }}>
        <button class="discard" onclick={() => app.discard()}>Discard</button>
        <button class="apply" onclick={onapply}>
          Apply <span class="count num">{app.pending.length}</span>
        </button>
      </div>
    {/if}
    <button
      class="gear"
      aria-label="Settings"
      aria-haspopup="menu"
      disabled={app.status !== "connected"}
      onclick={(e) => {
        const r = e.currentTarget.getBoundingClientRect();
        menu = { items: settings, x: window.innerWidth - r.right, y: r.bottom + 6, align: "right", label: "Settings" };
      }}
    >
      <svg viewBox="0 0 20 20" aria-hidden="true">
        <path d="M3 6h8M15 6h2M3 14h2M9 14h8" />
        <circle cx="13" cy="6" r="2" />
        <circle cx="7" cy="14" r="2" />
      </svg>
    </button>
  </div>
</header>

{#if menu}
  <Menu {...menu} onclose={() => (menu = null)} />
{/if}

<style>
  header {
    height: 56px;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    padding: 0 16px 0 14px;
    position: relative;
    z-index: 10;
  }
  .left {
    display: flex;
    align-items: center;
  }
  .pills {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .pill {
    position: relative;
    display: flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    padding: 0 11px 0 12px;
    border-radius: 999px;
    color: var(--text-2);
    transition:
      background-color var(--t) var(--ease),
      color var(--t) var(--ease);
  }
  .pill:hover {
    color: var(--text);
    background: rgb(255 255 255 / 0.04);
  }
  .pill.selected {
    color: var(--text);
    background: rgb(255 255 255 / 0.085);
  }
  .n {
    font-weight: 560;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    transition: box-shadow 300ms var(--ease);
  }
  .dot.hollow {
    box-shadow: inset 0 0 0 1.5px var(--text-3);
  }
  .pill.active .dot {
    box-shadow:
      0 0 0 2px var(--bg),
      0 0 0 3.5px rgb(255 255 255 / 0.55);
  }
  .pill.selected.active .dot {
    box-shadow:
      0 0 0 2px #2e2e32,
      0 0 0 3.5px rgb(255 255 255 / 0.55);
  }
  .pill.off .n {
    color: var(--text-3);
  }
  .pill.off .dot {
    background: transparent !important;
    box-shadow: inset 0 0 0 1.5px var(--text-3);
  }
  .more {
    width: 30px;
    height: 30px;
    border-radius: 999px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    transition:
      background-color var(--t) var(--ease),
      color var(--t) var(--ease);
  }
  .more:hover {
    background: rgb(255 255 255 / 0.05);
    color: var(--text);
  }
  .more svg {
    width: 16px;
    height: 16px;
    fill: currentColor;
  }

  .tabs {
    position: relative;
    display: flex;
    gap: 6px;
  }
  .tabs button {
    height: 56px;
    padding: 0 12px;
    color: var(--text-2);
    font-weight: 520;
    font-size: var(--fs-tab);
    transition: color var(--t) var(--ease);
  }
  .tabs button:hover,
  .tabs button.on {
    color: var(--text);
  }
  .tabs button:focus-visible {
    outline-offset: -12px;
  }
  .bar {
    position: absolute;
    left: 0;
    bottom: 10px;
    height: 2px;
    border-radius: 2px;
    background: var(--accent);
    transition:
      translate 240ms var(--ease),
      width 240ms var(--ease),
      background-color 300ms var(--ease);
  }

  .right {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 10px;
  }
  .pending {
    display: flex;
    gap: 6px;
  }
  .discard,
  .apply {
    height: 32px;
    padding: 0 14px;
    border-radius: 8px;
    font-weight: 560;
    transition:
      background-color var(--t) var(--ease),
      filter var(--t) var(--ease);
  }
  .discard {
    color: var(--text-2);
  }
  .discard:hover {
    color: var(--text);
    background: rgb(255 255 255 / 0.06);
  }
  .apply {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--accent);
    color: var(--accent-ink);
    padding-right: 10px;
  }
  .apply:hover {
    filter: brightness(1.08);
  }
  .count {
    min-width: 20px;
    height: 20px;
    padding: 0 6px;
    border-radius: 999px;
    background: rgb(0 0 0 / 0.16);
    font-size: 12px;
    display: grid;
    place-items: center;
  }
  .gear {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    transition:
      background-color var(--t) var(--ease),
      color var(--t) var(--ease);
  }
  .gear:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.06);
    color: var(--text);
  }
  .gear:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .gear svg {
    width: 20px;
    height: 20px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
  }
</style>
