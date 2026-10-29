<script lang="ts">
  import { hexToRgb, rgbToHex } from "./color";

  interface Props {
    color: string;
    onchange: (hex: string) => void;
  }
  let { color, onchange }: Props = $props();

  const SWATCHES: [string, string][] = [
    ["#ffffff", "White"],
    ["#ff2a1a", "Red"],
    ["#ff7a00", "Orange"],
    ["#ffdc00", "Yellow"],
    ["#2dff4a", "Green"],
    ["#00dcff", "Cyan"],
    ["#1a4dff", "Blue"],
    ["#8000ff", "Purple"],
    ["#ff2ab4", "Pink"],
  ];

  // LEDs have no "value" axis worth picking: brightness is its own slider.
  function toHs(hex: string): [number, number] {
    const [r, g, b] = hexToRgb(hex).map((v) => v / 255);
    const mx = Math.max(r, g, b);
    const mn = Math.min(r, g, b);
    const d = mx - mn;
    let h = 0;
    if (d) {
      if (mx === r) h = ((g - b) / d) % 6;
      else if (mx === g) h = (b - r) / d + 2;
      else h = (r - g) / d + 4;
    }
    return [((h * 60) + 360) % 360, mx ? d / mx : 0];
  }
  function fromHs(h: number, s: number): string {
    const f = (n: number) => {
      const k = (n + h / 60) % 6;
      return 255 * (1 - s * Math.max(0, Math.min(k, 4 - k, 1)));
    };
    return rgbToHex([f(5), f(3), f(1)]);
  }

  let h = $state(0);
  let s = $state(1);
  let last = "";
  let hexText = $state("");
  let area: HTMLDivElement;

  $effect(() => {
    // follow outside changes, but keep our hue when saturation hits zero
    if (color.toLowerCase() !== last) {
      const [nh, ns] = toHs(color);
      if (ns > 0.001) h = nh;
      s = ns;
      last = color.toLowerCase();
    }
    hexText = color.toUpperCase();
  });

  function emit() {
    last = fromHs(h, s);
    onchange(last);
  }

  function at(e: PointerEvent) {
    const r = area.getBoundingClientRect();
    h = Math.min(359.9, Math.max(0, ((e.clientX - r.left) / r.width) * 360));
    s = Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height));
    emit();
  }

  function key(e: KeyboardEvent) {
    const k = e.shiftKey ? 10 : 2;
    if (e.key === "ArrowLeft") h = (h - k + 360) % 360;
    else if (e.key === "ArrowRight") h = (h + k) % 360;
    else if (e.key === "ArrowUp") s = Math.min(1, s + k / 100);
    else if (e.key === "ArrowDown") s = Math.max(0, s - k / 100);
    else return;
    e.preventDefault();
    emit();
  }

  function commitHex() {
    const m = hexText.trim().replace(/^#?/, "#");
    if (/^#[0-9a-fA-F]{6}$/.test(m)) {
      last = "";
      onchange(m.toLowerCase());
    } else hexText = color.toUpperCase();
  }
</script>

<div class="picker">
  <div
    class="area"
    bind:this={area}
    role="slider"
    tabindex="0"
    aria-label="Colour"
    aria-valuetext="{color}, hue {Math.round(h)}°, saturation {Math.round(s * 100)}%"
    aria-valuenow={Math.round(h)}
    onpointerdown={(e) => {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      at(e);
    }}
    onpointermove={(e) => e.buttons & 1 && at(e)}
    onkeydown={key}
  >
    <span class="handle" style:left="{(h / 360) * 100}%" style:top="{(1 - s) * 100}%" style:background={color}></span>
  </div>

  <div class="under">
    <label class="hex">
      <span class="chip" style:background={color}></span>
      <input
        bind:value={hexText}
        aria-label="Hex colour"
        spellcheck="false"
        maxlength="7"
        onkeydown={(e) => e.key === "Enter" && commitHex()}
        onblur={commitHex}
      />
    </label>
    <div class="swatches">
      {#each SWATCHES as [c, name] (c)}
        <button
          class="sw"
          class:on={c === color.toLowerCase()}
          style:background={c}
          aria-label={name}
          title={name}
          onclick={() => {
            last = "";
            onchange(c);
          }}
        ></button>
      {/each}
    </div>
  </div>
</div>

<style>
  .area {
    position: relative;
    height: clamp(150px, 22vh, 240px);
    border-radius: 10px;
    background:
      linear-gradient(to bottom, transparent, #fff),
      linear-gradient(to right, #f00, #ff0 16.7%, #0f0 33.3%, #0ff 50%, #00f 66.7%, #f0f 83.3%, #f00);
    cursor: crosshair;
    touch-action: none;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.25);
  }
  .area:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 3px;
  }
  .handle {
    position: absolute;
    width: 18px;
    height: 18px;
    translate: -50% -50%;
    border-radius: 50%;
    box-shadow:
      0 0 0 2.5px #fff,
      0 2px 8px rgb(0 0 0 / 0.55);
    pointer-events: none;
  }
  .under {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-top: 14px;
  }
  .hex {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px 0 8px;
    border-radius: 8px;
    background: rgb(255 255 255 / 0.06);
  }
  .chip {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.3);
  }
  .hex input {
    width: 78px;
    border: 0;
    background: none;
    outline: none;
    font-variant-numeric: tabular-nums;
    font-weight: 520;
    letter-spacing: 0.02em;
  }
  .swatches {
    display: flex;
    gap: 7px;
    flex-wrap: wrap;
  }
  .sw {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.3);
    transition:
      scale var(--t) var(--ease),
      box-shadow var(--t) var(--ease);
  }
  .sw:hover {
    scale: 1.12;
  }
  .sw.on {
    box-shadow:
      inset 0 0 0 1px rgb(0 0 0 / 0.3),
      0 0 0 2px var(--bg),
      0 0 0 4px var(--text);
  }
</style>
