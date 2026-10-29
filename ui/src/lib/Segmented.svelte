<script lang="ts" generics="T extends string | number">
  interface Props {
    options: { value: T; label: string }[];
    value: T | null;
    label: string;
    onchange: (v: T) => void;
  }
  let { options, value, label, onchange }: Props = $props();

  let els: HTMLButtonElement[] = $state([]);
  let thumb = $state({ x: 0, w: 0 });
  const i = $derived(options.findIndex((o) => o.value === value));

  $effect(() => {
    const el = els[i];
    thumb = el ? { x: el.offsetLeft, w: el.offsetWidth } : { x: 0, w: 0 };
  });

  function key(e: KeyboardEvent) {
    const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const n = options.length;
    const next = ((i < 0 ? 0 : i) + d + n) % n;
    onchange(options[next].value);
    els[next]?.focus();
  }
</script>

<div class="seg" role="radiogroup" aria-label={label}>
  <span class="thumb" class:none={i < 0} style:translate="{thumb.x}px 0" style:width="{thumb.w}px"></span>
  {#each options as o, k (o.value)}
    <button
      bind:this={els[k]}
      role="radio"
      aria-checked={o.value === value}
      tabindex={k === (i < 0 ? 0 : i) ? 0 : -1}
      class:on={o.value === value}
      onclick={() => onchange(o.value)}
      onkeydown={key}>{o.label}</button
    >
  {/each}
</div>

<style>
  .seg {
    position: relative;
    display: inline-flex;
    padding: 3px;
    border-radius: 10px;
    background: rgb(255 255 255 / 0.06);
  }
  .thumb {
    position: absolute;
    top: 3px;
    bottom: 3px;
    left: 0;
    border-radius: 8px;
    background: rgb(255 255 255 / 0.13);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.3);
    transition:
      translate 220ms var(--ease),
      width 220ms var(--ease),
      opacity var(--t) var(--ease);
  }
  .thumb.none {
    opacity: 0;
  }
  button {
    position: relative;
    height: 32px;
    padding: 0 16px;
    border-radius: 8px;
    color: var(--text-2);
    font-weight: 520;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    transition: color var(--t) var(--ease);
  }
  button:hover,
  button.on {
    color: var(--text);
  }
</style>
