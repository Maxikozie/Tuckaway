<script lang="ts">
  import { app } from "./state.svelte";
  import ColorArea from "./ColorArea.svelte";
  import Segmented from "./Segmented.svelte";
  import Slider from "./Slider.svelte";
  import type { Led } from "./types";

  type Mode = "static" | "breathing" | "cycle" | "off";
  const MODES: { value: Mode; label: string; id: number }[] = [
    { value: "static", label: "Static", id: 0x01 },
    { value: "breathing", label: "Breathing", id: 0x0a },
    { value: "cycle", label: "Colour cycle", id: 0x03 },
    { value: "off", label: "Off", id: 0x00 },
  ];
  const ZONE_NAMES = ["DPI lights", "Logo"];

  const zones = $derived((app.snap?.led_zones ?? []).map((z) => ({ value: z.index, label: ZONE_NAMES[z.index] ?? `Zone ${z.index + 1}` })));
  const supported = $derived.by(() => {
    const z = app.snap?.led_zones.find((z) => z.index === app.zone);
    return MODES.filter((m) => !z || z.effects.includes(m.id));
  });
  const led = $derived<Led | undefined>(app.current?.profile.leds[app.zone]);
  const mode = $derived<Mode | null>(led && led.mode !== "raw" ? led.mode : null);

  // colour a zone returns to when switching back from cycle or off
  let lastColor: Record<number, string> = {};
  const colorOf = (l: Led | undefined) => (l && (l.mode === "static" || l.mode === "breathing") ? l.color : null);

  function setMode(m: Mode) {
    const c = colorOf(led) ?? lastColor[app.zone] ?? "#ffffff";
    // returning to the profile's own effect starts from its saved settings
    const saved = app.base.find((b) => b.index === app.selected)?.profile.leds[app.zone];
    if (saved && saved.mode === m) {
      app.setLed(app.zone, saved.mode === "static" || saved.mode === "breathing" ? { ...saved, color: c } : { ...saved });
      return;
    }
    const period = led && (led.mode === "breathing" || led.mode === "cycle") ? led.period_ms : m === "cycle" ? 8000 : 4000;
    const brightness = led && (led.mode === "breathing" || led.mode === "cycle") ? led.brightness : 100;
    const next: Led =
      m === "static"
        ? { mode: "static", color: c, ramp: 0 }
        : m === "breathing"
          ? { mode: "breathing", color: c, period_ms: period, waveform: 0, brightness }
          : m === "cycle"
            ? { mode: "cycle", period_ms: period, brightness }
            : { mode: "off" };
    app.setLed(app.zone, next);
  }

  function setColor(c: string) {
    if (!led || (led.mode !== "static" && led.mode !== "breathing")) return;
    lastColor[app.zone] = c;
    app.setLed(app.zone, { ...led, color: c });
  }

  // speed: right is faster; periods from 20 s down to 1 s on a log scale
  const MAXP = 20000;
  const MINP = 1000;
  const toSpeed = (p: number) => (100 * Math.log(MAXP / p)) / Math.log(MAXP / MINP);
  const toPeriod = (s: number) => Math.round((MAXP * (MINP / MAXP) ** (s / 100)) / 100) * 100;

  function setPeriod(s: number) {
    if (led && (led.mode === "breathing" || led.mode === "cycle")) app.setLed(app.zone, { ...led, period_ms: toPeriod(s) });
  }
  function setBrightness(b: number) {
    if (led && (led.mode === "breathing" || led.mode === "cycle")) app.setLed(app.zone, { ...led, brightness: b });
  }

  $effect(() => {
    app.checkLighting();
    const t = setInterval(() => app.checkLighting(), 5000);
    return () => clearInterval(t);
  });
</script>

{#if led}
  {#if app.lightingBlocked}
    <p class="blocked" role="status">Windows is controlling the mouse lighting. Changes won't show until its lighting service is stopped.</p>
  {/if}

  <section class="zone">
    <Segmented label="Lighting zone" options={zones} value={app.zone} onchange={(z) => (app.zone = z)} />
  </section>

  <section>
    <h2>Effect</h2>
    <Segmented label="Effect" options={supported} value={mode} onchange={setMode} />
  </section>

  {#if led.mode === "static" || led.mode === "breathing"}
    <section>
      <h2>Colour</h2>
      <ColorArea color={led.color} onchange={setColor} />
    </section>
  {/if}

  {#if led.mode === "breathing" || led.mode === "cycle"}
    <section class="sliders">
      <Slider label="Speed" min={0} max={100} value={toSpeed(led.period_ms)} text="{(led.period_ms / 1000).toFixed(1)} s" onchange={setPeriod} />
      <Slider label="Brightness" min={1} max={100} value={led.brightness} text="{led.brightness}%" onchange={setBrightness} />
    </section>
  {/if}
{/if}

<style>
  section + section {
    margin-top: 30px;
  }
  .zone {
    margin-bottom: 34px;
  }
  h2 {
    margin: 0 0 14px;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .sliders {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 28px;
  }
  .blocked {
    margin: 0 0 22px;
    padding-left: 12px;
    border-left: 2px solid var(--danger);
    color: var(--text);
    line-height: 1.45;
  }
</style>
