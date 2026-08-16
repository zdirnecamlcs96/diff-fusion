<script lang="ts">
  import { getCaptures, getCapture } from "../lib/api";
  import type { Capture, CaptureSummary } from "../lib/types";
  import { Button } from "../lib/components/ui/button";
  import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "../lib/components/ui/card";

  const POLL_MS = 2000;

  let { onload }: { onload: (captureId: string, c: Capture) => void } = $props();

  let captures: CaptureSummary[] = $state([]);
  let activeId: string | null = $state(null);
  let statusText: string = $state("no captures yet");
  let statusError: boolean = $state(false);
  let pollHandle: number | null = null;

  async function refresh() {
    try {
      captures = await getCaptures();
      statusText = captures.length ? `${captures.length} capture${captures.length === 1 ? "" : "s"}` : "no captures yet";
      statusError = false;
    } catch (e: any) {
      statusText = `failed to list captures: ${e.message}`;
      statusError = true;
    }
  }

  $effect(() => {
    refresh();
    pollHandle = window.setInterval(refresh, POLL_MS);
    return () => {
      if (pollHandle !== null) window.clearInterval(pollHandle);
    };
  });

  async function pickCapture(id: string) {
    try {
      const c = await getCapture(id);
      activeId = id;
      onload(id, c);
    } catch (e: any) {
      statusText = `failed to load capture: ${e.message}`;
      statusError = true;
    }
  }

  function relative(ms: number): string {
    if (!ms) return "never";
    const delta = Math.max(0, Date.now() - ms);
    if (delta < 1500) return "just now";
    if (delta < 60_000) return `${Math.floor(delta / 1000)}s ago`;
    return `${Math.floor(delta / 60_000)}m ago`;
  }
</script>

<Card>
  <CardHeader class="flex flex-row items-center justify-between space-y-0 pb-3">
    <div class="space-y-1">
      <CardTitle class="text-base">Captures</CardTitle>
      <CardDescription>
        Snapshots posted via <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">diff_fusion::application::capture::capture</code>.
      </CardDescription>
    </div>
    <Button variant="outline" size="sm" onclick={refresh}>Refresh</Button>
  </CardHeader>
  <CardContent>
    <p class="mb-3 text-xs text-muted-foreground" class:text-destructive={statusError}>{statusText}</p>
    {#if captures.length === 0}
      <p class="rounded-md border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
        No captures — run <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">cargo run --example observe_demo</code>.
      </p>
    {:else}
      <ul class="space-y-2">
        {#each captures as c (c.capture_id)}
          <li>
            <button
              type="button"
              class="flex w-full items-center justify-between gap-4 rounded-md border border-border bg-card px-4 py-3 text-left transition-colors hover:bg-accent hover:text-accent-foreground"
              class:ring-1={activeId === c.capture_id}
              class:ring-ring={activeId === c.capture_id}
              onclick={() => pickCapture(c.capture_id)}
            >
              <span class="truncate font-mono text-sm">{c.capture_id}</span>
              <span class="shrink-0 font-mono text-xs text-muted-foreground">
                {c.entity_type}/{c.canonical_id} · saved {relative(c.saved_at_ms)}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </CardContent>
</Card>
