<script lang="ts">
  import { app } from "./state.svelte";
  import DpiTrack from "./DpiTrack.svelte";
  import Segmented from "./Segmented.svelte";

  let { onshift }: { onshift: (on: boolean) => void } = $props();

  const p = $derived(app.current?.profile);
  const rates = $derived(
    [...(app.snap?.rates_ms ?? [1, 2, 4, 8])].sort((a, b) => b - a).map((ms) => ({ value: ms, label: `${Math.round(1000 / ms)} Hz` })),
  );
</script>

{#if p}
  <section>
    <h2>DPI levels</h2>
    <DpiTrack
      dpi={p.dpi}
      def={p.default_dpi_index}
      shift={p.shift_dpi_index}
      values={app.snap?.dpi_values ?? []}
      hasShift={!!app.snap?.has_dpi_shift}
      onchange={(v) => {
        p.dpi = v.dpi;
        p.default_dpi_index = v.def;
        p.shift_dpi_index = v.shift;
      }}
      onfocusshift={onshift}
    />
  </section>

  <section>
    <h2>Report rate</h2>
    <Segmented label="Report rate" options={rates} value={p.report_interval_ms} onchange={(ms) => (p.report_interval_ms = ms)} />
  </section>
{/if}

<style>
  section + section {
    margin-top: 44px;
  }
  h2 {
    margin: 0 0 18px;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
</style>
