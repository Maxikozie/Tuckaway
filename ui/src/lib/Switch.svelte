<script lang="ts">
  interface Props {
    checked: boolean;
    label: string;
    title?: string;
    onchange: (v: boolean) => void;
  }
  let { checked, label, title, onchange }: Props = $props();
</script>

<button class="switch" role="switch" aria-checked={checked} {title} onclick={() => onchange(!checked)}>
  <span class="track"><span class="knob"></span></span>
  <span class="text">{label}</span>
</button>

<style>
  .switch {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-radius: 999px;
    color: var(--text-2);
    transition:
      color var(--t) var(--ease),
      background-color var(--t) var(--ease);
  }
  .switch:hover {
    background: rgb(255 255 255 / 0.05);
    color: var(--text);
  }
  .track {
    width: 30px;
    height: 18px;
    border-radius: 999px;
    background: rgb(255 255 255 / 0.14);
    position: relative;
    transition: background-color var(--t) var(--ease);
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #f2f0eb;
    transition: translate 200ms var(--ease);
  }
  [aria-checked="true"] {
    color: var(--text);
  }
  [aria-checked="true"] .track {
    background: var(--accent);
  }
  [aria-checked="true"] .knob {
    translate: 12px 0;
    background: var(--accent-ink);
  }
</style>
