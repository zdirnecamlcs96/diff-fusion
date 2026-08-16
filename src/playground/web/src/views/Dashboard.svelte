<script lang="ts">
  import CapturesPanel from "../components/CapturesPanel.svelte";
  import SavedTestsPanel from "../components/SavedTestsPanel.svelte";
  import { getCaptures, getTests } from "../lib/api";
  import { initFromCapture, initFromTest } from "../lib/wizardState.svelte";
  import type { Capture, CaptureSummary, TestRecord, TestSummary } from "../lib/types";
  import { Card, CardContent } from "../lib/components/ui/card";

  let { onstart }: { onstart: () => void } = $props();

  let captures: CaptureSummary[] = $state([]);
  let tests: TestSummary[] = $state([]);
  let lastActivityMs: number = $state(0);

  async function refreshSummary() {
    try {
      const [c, t] = await Promise.all([getCaptures(), getTests()]);
      captures = c;
      tests = t;
      lastActivityMs = [...c.map((x) => x.saved_at_ms), ...t.map((x) => x.saved_at_ms)].reduce(
        (m, v) => Math.max(m, v),
        0,
      );
    } catch {
      // Per-panel errors surface in their own status lines.
    }
  }

  $effect(() => {
    refreshSummary();
    const handle = window.setInterval(refreshSummary, 4000);
    return () => window.clearInterval(handle);
  });

  function relative(ms: number): string {
    if (!ms) return "—";
    const delta = Math.max(0, Date.now() - ms);
    if (delta < 1500) return "just now";
    if (delta < 60_000) return `${Math.floor(delta / 1000)}s ago`;
    if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`;
    return `${Math.floor(delta / 3_600_000)}h ago`;
  }

  function loadCapture(id: string, c: Capture) { initFromCapture(id, c); onstart(); }
  function loadTest(id: string, t: TestRecord) { initFromTest(id, t); onstart(); }
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-2xl font-semibold tracking-tight">Dashboard</h2>
    <p class="text-sm text-muted-foreground">Saved tests and captured snapshots.</p>
  </div>

  <div class="grid gap-4 md:grid-cols-3">
    <Card>
      <CardContent class="pt-6">
        <p class="text-xs uppercase tracking-wider text-muted-foreground">Saved tests</p>
        <p class="mt-1 text-3xl font-semibold tabular-nums">{tests.length}</p>
      </CardContent>
    </Card>
    <Card>
      <CardContent class="pt-6">
        <p class="text-xs uppercase tracking-wider text-muted-foreground">Captures</p>
        <p class="mt-1 text-3xl font-semibold tabular-nums">{captures.length}</p>
      </CardContent>
    </Card>
    <Card>
      <CardContent class="pt-6">
        <p class="text-xs uppercase tracking-wider text-muted-foreground">Last activity</p>
        <p class="mt-1 text-3xl font-semibold">{relative(lastActivityMs)}</p>
      </CardContent>
    </Card>
  </div>

  <SavedTestsPanel onload={loadTest} />

  <CapturesPanel onload={loadCapture} />
</div>
