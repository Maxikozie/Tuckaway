<script lang="ts" module>
  export interface MenuItem {
    label: string;
    action: () => void;
    disabled?: boolean;
    danger?: boolean;
    separator?: boolean;
  }
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import { scale } from "svelte/transition";

  interface Props {
    items: MenuItem[];
    x: number;
    y: number;
    align?: "left" | "right";
    label: string;
    onclose: () => void;
  }
  let { items, x, y, align = "left", label, onclose }: Props = $props();

  let el: HTMLDivElement;
  let cursor = $state(-1);
  const enabled = $derived(items.map((it, i) => (it.disabled ? -1 : i)).filter((i) => i >= 0));

  onMount(() => {
    cursor = enabled[0] ?? -1;
    tick().then(() => el.querySelector<HTMLElement>(`[data-i="${cursor}"]`)?.focus());
    const outside = (e: PointerEvent) => {
      if (!el.contains(e.target as Node)) onclose();
    };
    setTimeout(() => window.addEventListener("pointerdown", outside, true));
    return () => window.removeEventListener("pointerdown", outside, true);
  });

  function move(d: number) {
    const k = enabled.indexOf(cursor);
    cursor = enabled[(k + d + enabled.length) % enabled.length];
    el.querySelector<HTMLElement>(`[data-i="${cursor}"]`)?.focus();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown") (e.preventDefault(), move(1));
    else if (e.key === "ArrowUp") (e.preventDefault(), move(-1));
    else if (e.key === "Escape" || e.key === "Tab") (e.preventDefault(), onclose());
  }
</script>

<div
  class="menu"
  role="menu"
  aria-label={label}
  tabindex="-1"
  bind:this={el}
  style:top="{y}px"
  style={align === "left" ? `left:${x}px` : `right:${x}px`}
  onkeydown={onKey}
  transition:scale={{ start: 0.97, duration: 140 }}
>
  {#each items as it, i (it.label)}
    {#if it.separator}<div class="sep" role="separator"></div>{/if}
    <button
      role="menuitem"
      data-i={i}
      class:danger={it.danger}
      disabled={it.disabled}
      tabindex={i === cursor ? 0 : -1}
      onpointermove={() => !it.disabled && (cursor = i)}
      onclick={() => {
        onclose();
        it.action();
      }}>{it.label}</button
    >
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    min-width: 210px;
    padding: 6px;
    background: var(--raised);
    border: 1px solid var(--hair);
    border-radius: 12px;
    box-shadow: 0 16px 44px rgb(0 0 0 / 0.45);
    z-index: 40;
    transform-origin: top;
  }
  button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 7px 10px;
    border-radius: 7px;
  }
  button:focus-visible {
    outline: none;
    background: rgb(255 255 255 / 0.07);
  }
  button:hover:not(:disabled) {
    background: rgb(255 255 255 / 0.07);
  }
  button:disabled {
    color: var(--text-3);
    cursor: default;
  }
  .danger {
    color: var(--danger);
  }
  .sep {
    height: 1px;
    background: var(--hair);
    margin: 5px 4px;
  }
</style>
