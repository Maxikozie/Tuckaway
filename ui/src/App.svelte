<script lang="ts">
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { api } from "./lib/api";
  import { inkOn } from "./lib/color";
  import { buttonName } from "./lib/names";
  import { app } from "./lib/state.svelte";
  import ActionPicker from "./lib/ActionPicker.svelte";
  import Callouts from "./lib/Callouts.svelte";
  import Modal from "./lib/Modal.svelte";
  import Mouse, { type Geom } from "./lib/Mouse.svelte";
  import Switch from "./lib/Switch.svelte";
  import Toasts from "./lib/Toasts.svelte";
  import TopBar from "./lib/TopBar.svelte";
  import PerformancePanel from "./lib/PerformancePanel.svelte";
  import LightingPanel from "./lib/LightingPanel.svelte";
  import { fly } from "svelte/transition";
  import type { MenuItem } from "./lib/Menu.svelte";
  import type { Action } from "./lib/types";

  let stageH = $state(0);
  let stageW = $state(0);
  let geom = $state<Geom | null>(null);
  let hovered = $state<number | null>(null);
  type Picker = { slot: number; side: "left" | "right"; top: number; x: number };
  let picker = $state<Picker | null>(null);
  // the popover keeps its last position while it animates out
  let shown = $state<Picker | null>(null);
  let confirm = $state<null | { kind: "apply" | "restore" | "reset"; path?: string }>(null);

  onMount(() => {
    app.start().catch((e) => app.toast(String(e), true));
  });

  $effect(() => {
    document.documentElement.style.setProperty("--accent", app.accent);
    document.documentElement.style.setProperty("--accent-ink", inkOn(app.accent));
  });

  const layer = $derived(app.gshift ? "gshift" : "buttons");
  const actions = $derived<Action[]>(app.current ? app.current.actions[layer] : []);
  const connected = $derived(app.status === "connected");

  // The mouse box: as tall as the stage allows, keeping the art's aspect.
  const ART = 700 / 1190;
  const PICKER_W = 300;
  const mouseBox = $derived.by(() => {
    const h = Math.max(320, stageH - 84);
    const w = h * ART;
    // Buttons: centred between the callouts. Other tabs: left, beside the panel.
    const cx = app.tab === "buttons" ? stageW / 2 : Math.max(w / 2 + 40, stageW * 0.3);
    return { w, h, left: cx - w / 2, top: 6 };
  });
  const panel = $derived.by(() => {
    const left = mouseBox.left + mouseBox.w + (stageW > 1600 ? 110 : 64);
    // Performance has a fixed height, so it sits centred; Lighting grows downward.
    const top = app.tab === "performance" ? Math.max(36, (stageH - 360) / 2) : Math.max(36, stageH * 0.13);
    return { left, width: Math.min(stageW > 1600 ? 700 : 620, stageW - left - 48), top };
  });
  let shiftHover = $state(false);

  function pick(slot: number, side: "left" | "right", el: HTMLElement) {
    if (picker?.slot === slot) return (picker = null);
    const r = el.getBoundingClientRect();
    const stageTop = 56;
    const center = r.top + r.height / 2 - stageTop;
    const top = Math.min(Math.max(12, center - 160), Math.max(12, stageH - 482));
    // the popover takes the place of its side's callout column
    const x = side === "left" ? Math.max(16, r.right - PICKER_W) : Math.min(stageW - PICKER_W - 16, r.left);
    picker = shown = { slot, side, top, x };
  }

  function choose(a: Action) {
    if (picker) app.setBinding(picker.slot, a);
    const slot = picker?.slot;
    picker = null;
    // return focus to the callout that opened the picker
    if (slot !== undefined) setTimeout(() => document.querySelector<HTMLElement>(`[aria-label^="${buttonName(slot)}:"]`)?.focus());
  }

  async function doApply() {
    const ok = await app.apply();
    if (ok) confirm = null;
  }

  async function doRestore(path: string) {
    await app.run("Restoring", async () => {
      const r = await api.restore(path);
      return r.sectors ? `Restored ${r.sectors} sectors from the backup` : "The mouse already matches this backup";
    });
    confirm = null;
  }

  async function doReset() {
    await app.run("Resetting", async () => {
      await api.factoryReset();
      return "Factory profiles restored";
    });
    confirm = null;
  }

  const settings: MenuItem[] = [
    {
      label: "Back up now",
      action: () =>
        app.run("Backing up", async () => {
          const r = await api.backupNow();
          return r.path ? "Backup saved" : "Backups are skipped for the mock mouse";
        }),
    },
    {
      label: "Restore from backup…",
      action: async () => {
        const path = await open({
          title: "Restore from backup",
          defaultPath: await api.backupDir(),
          filters: [{ name: "MouseForge backup", extensions: ["json"] }],
        });
        if (typeof path === "string") confirm = { kind: "restore", path };
      },
    },
    {
      label: "Import profile…",
      separator: true,
      action: async () => {
        const path = await open({ title: "Import profile", filters: [{ name: "Profile", extensions: ["json"] }] });
        if (typeof path !== "string" || !app.current) return;
        const target = app.current;
        await app.run("Importing", async () => {
          const r = await api.importProfile(target.index, path);
          target.profile = r.profile;
          target.actions = r.actions;
          return `Imported into profile ${target.index + 1}`;
        });
      },
    },
    {
      label: "Export profile…",
      action: async () => {
        const p = app.current;
        if (!p) return;
        const path = await save({
          title: "Export profile",
          defaultPath: `profile-${p.index + 1}.json`,
          filters: [{ name: "Profile", extensions: ["json"] }],
        });
        if (!path) return;
        await app.run("Exporting", async () => {
          await api.exportProfile(p.profile, path);
          return `Exported profile ${p.index + 1}`;
        });
      },
    },
    { label: "Factory reset…", separator: true, danger: true, action: () => (confirm = { kind: "reset" }) },
  ];

  const byProfile = $derived.by(() => {
    const m = new Map<number, string[]>();
    for (const c of app.pending) m.set(c.profile, [...(m.get(c.profile) ?? []), c.text]);
    return [...m.entries()].sort((a, b) => a[0] - b[0]);
  });
</script>

<TopBar onapply={() => (confirm = { kind: "apply" })} {settings} />

<main class="stage" bind:clientHeight={stageH} bind:clientWidth={stageW}>
  <div class="spot" style:--cx="{mouseBox.left + mouseBox.w / 2}px" style:--cy="{mouseBox.top + mouseBox.h * 0.52}px"></div>

  <div
    class="mouse-box"
    style:width="{mouseBox.w}px"
    style:height="{mouseBox.h}px"
    style:left="{mouseBox.left}px"
    style:top="{mouseBox.top}px"
  >
    <Mouse
      bind:geom
      dim={!connected}
      hovered={app.tab === "performance" ? (shiftHover ? 5 : null) : hovered}
      selected={picker?.slot ?? null}
      leds={app.current?.profile.leds ?? []}
      slotsInteractive={connected && app.tab === "buttons"}
      zonesInteractive={connected && app.tab === "lighting"}
      zoneSelected={app.tab === "lighting" ? app.zone : null}
      onzone={(z) => (app.zone = z)}
      onhover={(s) => (hovered = s)}
      onpick={(s) => {
        const el = document.querySelector<HTMLElement>(`[aria-label^="${buttonName(s)}:"]`);
        if (el) pick(s, el.classList.contains("left") ? "left" : "right", el);
      }}
    />
  </div>

  {#if connected && app.tab === "buttons" && geom}
    <div class="overlay" in:fade={{ duration: 200, delay: 160 }} out:fade={{ duration: 140 }}>
      <Callouts
        geom={geom && {
          ...geom,
          x: (v) => geom!.x(v) + mouseBox.left,
          y: (v) => geom!.y(v) + mouseBox.top,
          left: geom.left + mouseBox.left,
          right: geom.right + mouseBox.left,
        }}
        height={stageH}
        {actions}
        {hovered}
        selected={picker?.slot ?? null}
        coveredSide={picker?.side ?? null}
        layerKey={layer}
        onhover={(s) => (hovered = s)}
        onpick={pick}
      />
    </div>
    {#if app.snap?.has_gshift}
      <div class="under" style:top="{mouseBox.top + mouseBox.h + 18}px">
        <Switch
          checked={app.gshift}
          label="G-shift layer"
          title="What each button does while a button set to G-shift is held"
          onchange={(v) => {
            app.gshift = v;
            picker = null;
          }}
        />
      </div>
    {/if}
  {/if}

  {#if connected && app.tab !== "buttons"}
    {#key app.tab}
      <div
        class="panel"
        style:left="{panel.left}px"
        style:width="{panel.width}px"
        style:top="{panel.top}px"
        in:fly={{ x: 14, duration: 240, delay: 120 }}
        out:fade={{ duration: 120 }}
      >
        {#if app.tab === "performance"}
          <PerformancePanel onshift={(on) => (shiftHover = on)} />
        {:else}
          <LightingPanel />
        {/if}
      </div>
    {/key}
  {/if}

  {#if !connected}
    <p class="status" style:top="{mouseBox.top + mouseBox.h + 22}px" transition:fade={{ duration: 200 }}>
      {app.status === "disconnected" && app.note ? app.note : "Connect your mouse"}
    </p>
  {/if}

  {#if picker && shown && app.current}
    <ActionPicker
      title={app.gshift ? `${buttonName(shown.slot)} with G-shift` : buttonName(shown.slot)}
      current={actions[shown.slot]}
      catalog={app.catalog}
      side={shown.side}
      top={shown.top}
      x={shown.x}
      onchoose={choose}
      onclose={() => (picker = null)}
    />
  {/if}
</main>

{#if confirm?.kind === "apply"}
  <Modal
    title={app.pending.length === 1 ? "Apply 1 change?" : `Apply ${app.pending.length} changes?`}
    confirm="Apply"
    busy={app.busy}
    onconfirm={doApply}
    oncancel={() => (confirm = null)}
  >
    {#each byProfile as [index, lines] (index)}
      <section class="changes">
        {#if byProfile.length > 1 || index !== app.selected}<h3>Profile {index + 1}</h3>{/if}
        <ul>
          {#each lines as l (l)}<li>{l}</li>{/each}
        </ul>
      </section>
    {/each}
  </Modal>
{:else if confirm?.kind === "restore" && confirm.path}
  {@const path = confirm.path}
  <Modal title="Restore this backup?" confirm="Restore" danger busy={app.busy} onconfirm={() => doRestore(path)} oncancel={() => (confirm = null)}>
    <p class="file">{path.split(/[\\/]/).pop()}</p>
    <p>The profiles on the mouse are replaced. The current state is backed up first.</p>
  </Modal>
{:else if confirm?.kind === "reset"}
  <Modal title="Reset to factory profiles?" confirm="Reset" danger busy={app.busy} onconfirm={doReset} oncancel={() => (confirm = null)}>
    <p>All five profiles go back to how they left the factory. The current state is backed up first.</p>
  </Modal>
{/if}

{#if app.busy && !confirm}
  <div class="busy" transition:fade={{ duration: 150 }} aria-live="polite">{app.busy}…</div>
{/if}

<Toasts toasts={app.toasts} />

<style>
  .stage {
    position: relative;
    height: calc(100% - 56px);
    overflow: hidden;
  }
  .spot {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(ellipse 30% 46% at var(--cx) var(--cy), rgb(255 255 255 / 0.075), transparent 70%),
      radial-gradient(ellipse 62% 78% at var(--cx) var(--cy), var(--stage) 0%, #2a2b2f 38%, var(--bg) 78%);
    pointer-events: none;
  }
  .mouse-box {
    position: absolute;
    transition: left 320ms var(--ease);
  }
  .spot {
    transition: --cx 320ms var(--ease);
  }
  .panel {
    position: absolute;
  }
  .overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .overlay :global(button) {
    pointer-events: auto;
  }
  .under {
    position: absolute;
    left: 50%;
    translate: -50% 0;
  }
  .status {
    position: absolute;
    left: 0;
    right: 0;
    margin: 0;
    text-align: center;
    font-size: 15px;
    color: var(--text-2);
  }
  .changes + .changes {
    margin-top: 14px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  li {
    font-size: 14px;
  }
  p {
    margin: 0 0 8px;
    color: var(--text-2);
    line-height: 1.45;
  }
  .file {
    color: var(--text);
    font-weight: 520;
    word-break: break-all;
  }
  .busy {
    position: fixed;
    left: 50%;
    bottom: 26px;
    translate: -50% 0;
    padding: 9px 16px;
    border-radius: 999px;
    background: var(--raised-2);
    border: 1px solid var(--hair);
    color: var(--text-2);
    z-index: 55;
  }
</style>
