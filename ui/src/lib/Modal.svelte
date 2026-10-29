<script lang="ts">
  import { onMount, type Snippet } from "svelte";
  import { fade, scale } from "svelte/transition";

  interface Props {
    title: string;
    confirm: string;
    danger?: boolean;
    busy?: string | null;
    children?: Snippet;
    onconfirm: () => void;
    oncancel: () => void;
  }
  let { title, confirm, danger = false, busy = null, children, onconfirm, oncancel }: Props = $props();

  let box: HTMLDivElement;
  let cancel: HTMLButtonElement | undefined = $state();
  // Start on the safe choice, and ignore a confirm that arrives with the
  // click or keypress that opened the dialog: writes need a deliberate act.
  let armed = false;
  onMount(() => {
    cancel?.focus();
    const t = setTimeout(() => (armed = true), 300);
    return () => clearTimeout(t);
  });
  function confirmNow() {
    if (armed && !busy) onconfirm();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !busy) {
      e.preventDefault();
      oncancel();
    } else if (e.key === "Tab") {
      // keep focus inside the dialog
      const f = Array.from(box.querySelectorAll<HTMLElement>("button:not(:disabled)"));
      const i = f.indexOf(document.activeElement as HTMLElement);
      e.preventDefault();
      f[(i + (e.shiftKey ? f.length - 1 : 1)) % f.length]?.focus();
    }
  }
</script>

<div class="scrim" transition:fade={{ duration: 160 }}></div>
<div
  class="modal"
  role="alertdialog"
  aria-modal="true"
  aria-label={title}
  tabindex="-1"
  bind:this={box}
  onkeydown={onKey}
  transition:scale={{ start: 0.97, duration: 180 }}
>
  <h2>{title}</h2>
  {#if children}<div class="body">{@render children()}</div>{/if}
  <div class="actions">
    {#if busy}
      <span class="busy" aria-live="polite">{busy}…</span>
    {:else}
      <button class="ghost" bind:this={cancel} onclick={oncancel}>Cancel</button>
      <button class="primary" class:danger onclick={confirmNow}>{confirm}</button>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgb(10 10 12 / 0.55);
    z-index: 50;
  }
  .modal {
    position: fixed;
    left: 50%;
    top: 50%;
    translate: -50% -50%;
    width: min(440px, calc(100vw - 48px));
    max-height: calc(100vh - 96px);
    display: flex;
    flex-direction: column;
    background: var(--raised);
    border: 1px solid var(--hair);
    border-radius: 14px;
    box-shadow: 0 24px 70px rgb(0 0 0 / 0.55);
    padding: 22px 22px 18px;
    z-index: 51;
  }
  h2 {
    margin: 0 0 14px;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .body {
    overflow-y: auto;
    margin: 0 -4px 18px;
    padding: 0 4px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    min-height: 36px;
    align-items: center;
  }
  button {
    height: 34px;
    padding: 0 16px;
    border-radius: 8px;
    font-weight: 560;
    transition:
      background-color var(--t) var(--ease),
      filter var(--t) var(--ease);
  }
  .ghost:hover {
    background: rgb(255 255 255 / 0.07);
  }
  .primary {
    background: var(--accent);
    color: var(--accent-ink);
  }
  .primary:hover {
    filter: brightness(1.08);
  }
  .primary.danger {
    background: var(--danger);
    color: #1a0d0b;
  }
  .busy {
    color: var(--text-2);
  }
</style>
