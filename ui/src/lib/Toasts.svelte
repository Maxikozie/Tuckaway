<script lang="ts">
  import { fly } from "svelte/transition";
  import type { Toast } from "./state.svelte";

  let { toasts }: { toasts: Toast[] } = $props();
</script>

<div class="toasts" aria-live="polite">
  {#each toasts as t (t.id)}
    <div class="toast" class:error={t.error} role={t.error ? "alert" : "status"} transition:fly={{ y: 12, duration: 200 }}>
      {#if !t.error}<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" /></svg>{/if}
      {t.text}
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    left: 50%;
    bottom: 26px;
    translate: -50% 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    z-index: 60;
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 560px;
    padding: 9px 16px 9px 12px;
    border-radius: 999px;
    background: var(--raised-2);
    border: 1px solid var(--hair);
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.4);
    font-weight: 520;
  }
  .toast.error {
    padding-left: 16px;
    border-color: rgb(240 106 91 / 0.45);
    color: #ffd9d3;
  }
  svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
