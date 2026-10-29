<script lang="ts">
  interface Props {
    label: string;
    value: number;
    min: number;
    max: number;
    step?: number;
    text: string;
    onchange: (v: number) => void;
  }
  let { label, value, min, max, step = 1, text, onchange }: Props = $props();
  const pct = $derived(((value - min) / (max - min)) * 100);
</script>

<label class="slider">
  <span class="head"><span>{label}</span><span class="val num">{text}</span></span>
  <input
    type="range"
    {min}
    {max}
    {step}
    {value}
    aria-valuetext={text}
    style:--p="{pct}%"
    oninput={(e) => onchange(Number(e.currentTarget.value))}
  />
</label>

<style>
  .slider {
    display: block;
  }
  .head {
    display: flex;
    justify-content: space-between;
    margin-bottom: 8px;
    font-weight: 520;
  }
  .val {
    color: var(--text-2);
    font-weight: 450;
  }
  input {
    width: 100%;
    height: 22px;
    margin: 0;
    background: none;
    appearance: none;
    cursor: pointer;
  }
  input::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 4px;
    background: linear-gradient(to right, var(--accent) var(--p), rgb(255 255 255 / 0.1) var(--p));
  }
  input::-webkit-slider-thumb {
    appearance: none;
    width: 18px;
    height: 18px;
    margin-top: -7px;
    border-radius: 50%;
    background: #f2f0eb;
    box-shadow:
      0 0 0 3px var(--bg),
      0 2px 6px rgb(0 0 0 / 0.5);
    transition: scale var(--t) var(--ease);
  }
  input:hover::-webkit-slider-thumb {
    scale: 1.1;
  }
  input:focus-visible {
    outline: none;
  }
  input:focus-visible::-webkit-slider-thumb {
    box-shadow:
      0 0 0 3px var(--bg),
      0 0 0 5px var(--accent);
  }
</style>
