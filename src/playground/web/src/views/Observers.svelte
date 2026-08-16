<script lang="ts">
  /** Inbound producer directory.
   *
   *  Each "observer" registers a `capture_id` the playground expects to
   *  receive at `POST /api/captures/:capture_id`. The page shows a
   *  copy-pasteable `HttpObserver` snippet so the user can wire their
   *  external diff-fusion code into this playground in seconds, plus a
   *  "last seen" badge that bumps every time a matching capture lands.
   *
   *  The playground itself is the sink — this page never sends anything
   *  outbound. Captures arrive from outside via the existing
   *  `crates/observe::HttpObserver` cable. */
  import { deleteObserver, getObservers, putObserver } from "../lib/api";
  import type { ObserverSummary } from "../lib/types";
  import { Button } from "../lib/components/ui/button";
  import { Input } from "../lib/components/ui/input";
  import { Label } from "../lib/components/ui/label";
  import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "../lib/components/ui/card";
  import { Badge } from "../lib/components/ui/badge";

  const POLL_MS = 2000;

  let observers: ObserverSummary[] = $state([]);
  let listError: string = $state("");

  let formName: string = $state("");
  let formCaptureId: string = $state("");
  let formStatus: string = $state("");
  let formTone: "" | "ok" | "err" = $state("");
  let saving: boolean = $state(false);
  let pollHandle: number | null = null;

  // The playground's own URL — used in the snippet for copy-paste. We
  // read window.location at mount; if the playground is reverse-proxied,
  // that's still the right value.
  let playgroundBase: string = $state("http://localhost:3000");
  $effect(() => {
    if (typeof window !== "undefined") {
      // In dev, vite is on :5173 but the backend is :3000. Hardcode :3000
      // since that's the binding in `playground/src/main.rs`.
      playgroundBase = `${window.location.protocol}//${window.location.hostname}:3000`;
    }
  });

  async function refresh() {
    try {
      observers = await getObservers();
      listError = "";
    } catch (e: any) {
      listError = `failed to list observers: ${e.message}`;
    }
  }

  $effect(() => {
    refresh();
    pollHandle = window.setInterval(refresh, POLL_MS);
    return () => {
      if (pollHandle !== null) window.clearInterval(pollHandle);
    };
  });

  function setFormStatus(msg: string, tone: "" | "ok" | "err" = "") {
    formStatus = msg;
    formTone = tone;
  }

  async function add() {
    const name = formName.trim();
    const captureId = formCaptureId.trim();
    if (!name) return setFormStatus("Name is required", "err");
    if (!captureId) return setFormStatus("Capture id is required", "err");
    saving = true;
    try {
      const id = `obs-${Date.now()}`;
      await putObserver(id, { name, capture_id: captureId });
      setFormStatus(`Added "${name}"`, "ok");
      formName = "";
      formCaptureId = "";
      await refresh();
    } catch (e: any) {
      setFormStatus(`Save failed: ${e.message}`, "err");
    } finally {
      saving = false;
    }
  }

  async function remove(o: ObserverSummary) {
    if (!confirm(`Remove observer "${o.name}"?`)) return;
    try {
      await deleteObserver(o.observer_id);
      await refresh();
    } catch (e: any) {
      listError = `delete failed: ${e.message}`;
    }
  }

  function relative(ms: number | null): string {
    if (!ms) return "never";
    const delta = Math.max(0, Date.now() - ms);
    if (delta < 1500) return "just now";
    if (delta < 60_000) return `${Math.floor(delta / 1000)}s ago`;
    if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`;
    return `${Math.floor(delta / 3_600_000)}h ago`;
  }

  function snippet(captureId: string): string {
    return `let observer = Arc::new(diff_fusion_observe::HttpObserver::new(
    "${playgroundBase}",
    "${captureId}",
));
diff_fusion::application::capture::capture(
    &side_a, &side_b, "po", "PO-1", &*observer,
).await?;`;
  }

  async function copySnippet(captureId: string) {
    try {
      await navigator.clipboard.writeText(snippet(captureId));
    } catch {
      // Clipboard API may be unavailable; fall back silently.
    }
  }
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-2xl font-semibold tracking-tight">Observers</h2>
    <p class="text-sm text-muted-foreground">
      Register an inbound producer label. The playground listens at
      <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">POST /api/captures/&lbrace;capture_id&rbrace;</code> —
      configure your external code with <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">HttpObserver</code>
      and any matching <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">capture_id</code> here gets a
      live last-seen badge below.
    </p>
  </div>

  <Card>
    <CardHeader class="pb-4">
      <CardTitle class="text-base">Register observer</CardTitle>
      <CardDescription>Stored in memory; cleared on server restart.</CardDescription>
    </CardHeader>
    <CardContent class="space-y-4">
      <div class="grid gap-4 md:grid-cols-[1fr_1fr_auto] md:items-end">
        <div class="space-y-1.5">
          <Label for="obs-name">Name</Label>
          <Input id="obs-name" placeholder="e.g. Production ERP" bind:value={formName} disabled={saving} />
        </div>
        <div class="space-y-1.5">
          <Label for="obs-capture-id">Capture id</Label>
          <Input id="obs-capture-id" placeholder="e.g. prod-erp-cycle" bind:value={formCaptureId} disabled={saving} />
        </div>
        <Button onclick={add} disabled={saving}>
          {saving ? "Saving…" : "Register"}
        </Button>
      </div>
      {#if formStatus}
        <p
          class="text-sm"
          class:text-ok={formTone === "ok"}
          class:text-destructive={formTone === "err"}
          class:text-muted-foreground={formTone === ""}
        >{formStatus}</p>
      {/if}
    </CardContent>
  </Card>

  <Card>
    <CardHeader class="flex flex-row items-center justify-between space-y-0 pb-3">
      <CardTitle class="text-base">Registered ({observers.length})</CardTitle>
      <Button variant="outline" size="sm" onclick={refresh}>Refresh</Button>
    </CardHeader>
    <CardContent>
      {#if listError}
        <p class="mb-3 text-sm text-destructive">{listError}</p>
      {/if}
      {#if observers.length === 0}
        <p class="rounded-md border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
          No observers yet. Register one above; then point your external
          diff-fusion code at <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">{playgroundBase}/api/captures/&lbrace;capture_id&rbrace;</code>.
        </p>
      {:else}
        <ul class="space-y-3">
          {#each observers as o (o.observer_id)}
            <li class="rounded-md border border-border bg-card">
              <div class="flex items-center justify-between gap-4 px-4 py-3">
                <div class="min-w-0 space-y-0.5">
                  <div class="flex items-center gap-2">
                    <span class="text-sm font-medium">{o.name}</span>
                    <Badge variant={o.last_seen_ms ? "success" : "outline"}>
                      {o.last_seen_ms ? `last seen ${relative(o.last_seen_ms)}` : "never received"}
                    </Badge>
                  </div>
                  <code class="block truncate font-mono text-xs text-muted-foreground">
                    capture_id: {o.capture_id}
                  </code>
                </div>
                <div class="flex shrink-0 items-center gap-2">
                  <Button variant="outline" size="sm" onclick={() => copySnippet(o.capture_id)}>
                    Copy snippet
                  </Button>
                  <Button variant="ghost" size="sm" class="text-muted-foreground hover:text-destructive" onclick={() => remove(o)}>
                    Remove
                  </Button>
                </div>
              </div>
              <pre class="whitespace-pre-wrap break-words rounded-b-md bg-muted/40 px-4 py-3 font-mono text-xs leading-snug text-foreground/80"
              >{snippet(o.capture_id)}</pre>
            </li>
          {/each}
        </ul>
      {/if}
    </CardContent>
  </Card>
</div>
