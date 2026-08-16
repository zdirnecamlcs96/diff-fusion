<script lang="ts">
  import { getTests, getTest } from "../lib/api";
  import type { TestRecord, TestSummary } from "../lib/types";
  import { Button } from "../lib/components/ui/button";
  import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "../lib/components/ui/card";
  import { Badge, type BadgeVariant } from "../lib/components/ui/badge";

  const POLL_MS = 2000;

  let { onload }: { onload: (testId: string, t: TestRecord) => void } = $props();

  let tests: TestSummary[] = $state([]);
  let activeId: string | null = $state(null);
  let statusText: string = $state("no saved tests yet");
  let statusError: boolean = $state(false);
  let pollHandle: number | null = null;

  async function refresh() {
    try {
      tests = await getTests();
      statusText = tests.length
        ? `${tests.length} saved test${tests.length === 1 ? "" : "s"}`
        : "no saved tests yet";
      statusError = false;
    } catch (e: any) {
      statusText = `failed to list tests: ${e.message}`;
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

  async function pickTest(id: string) {
    try {
      const t = await getTest(id);
      activeId = id;
      onload(id, t);
    } catch (e: any) {
      statusText = `failed to load test: ${e.message}`;
      statusError = true;
    }
  }

  function relative(ms: number): string {
    if (!ms) return "never";
    const delta = Math.max(0, Date.now() - ms);
    if (delta < 1500) return "just now";
    if (delta < 60_000) return `${Math.floor(delta / 1000)}s ago`;
    if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`;
    return `${Math.floor(delta / 3_600_000)}h ago`;
  }

  function outcomeBadge(o: string | null): { variant: BadgeVariant; label: string } | null {
    if (!o) return null;
    if (o === "Synced") return { variant: "success", label: o };
    if (o === "Escalated") return { variant: "warning", label: o };
    if (o === "Error") return { variant: "destructive", label: o };
    return { variant: "outline", label: o };
  }
</script>

<Card>
  <CardHeader class="flex flex-row items-center justify-between space-y-0 pb-3">
    <div class="space-y-1">
      <CardTitle class="text-base">Saved tests</CardTitle>
      <CardDescription>In-memory snapshots from the New Test wizard. Cleared on server restart.</CardDescription>
    </div>
    <Button variant="outline" size="sm" onclick={refresh}>Refresh</Button>
  </CardHeader>
  <CardContent>
    <p class="mb-3 text-xs text-muted-foreground" class:text-destructive={statusError}>{statusText}</p>
    {#if tests.length === 0}
      <p class="rounded-md border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
        No saved tests yet — start one from <span class="font-medium text-foreground">+ New test</span>.
      </p>
    {:else}
      <ul class="space-y-2">
        {#each tests as t (t.test_id)}
          {@const badge = outcomeBadge(t.last_outcome)}
          <li>
            <button
              type="button"
              class="flex w-full items-center justify-between gap-4 rounded-md border border-border bg-card px-4 py-3 text-left transition-colors hover:bg-accent hover:text-accent-foreground"
              class:ring-1={activeId === t.test_id}
              class:ring-ring={activeId === t.test_id}
              onclick={() => pickTest(t.test_id)}
            >
              <div class="flex min-w-0 items-center gap-3">
                <span class="truncate text-sm font-medium">{t.name}</span>
                {#if badge}
                  <Badge variant={badge.variant}>{badge.label}</Badge>
                {/if}
              </div>
              <span class="shrink-0 font-mono text-xs text-muted-foreground">
                {t.system_a_name} ↔ {t.system_b_name} · saved {relative(t.saved_at_ms)}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </CardContent>
</Card>
