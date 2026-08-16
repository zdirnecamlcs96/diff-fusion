<script lang="ts">
  import type { OutcomeDto, RunContext, StagesDto } from "../lib/types";
  import FieldChangelog from "./FieldChangelog.svelte";
  import { Card, CardContent } from "../lib/components/ui/card";
  import { Badge, type BadgeVariant } from "../lib/components/ui/badge";
  import { cn } from "../lib/utils";

  let { stages, context }: { stages: StagesDto; context: RunContext | null } = $props();
  let outcome = $derived(stages.outcome ?? null);
  let wouldWrite = $derived(stages.policy?.would_write ?? null);

  function summaryLine(o: OutcomeDto): string {
    if (o.kind === "Synced") {
      return "Pushed to: " + (o.pushed_to.length ? o.pushed_to.join(", ") : "(nothing to push)");
    }
    if (o.kind === "Escalated") {
      return ` ${o.conflicts.length} conflict(s) queued for review.`;
    }
    return "Neither side changed since ancestor.";
  }

  function strongLabel(o: OutcomeDto): string {
    if (o.kind === "Synced") return "Synced";
    if (o.kind === "Escalated") return "Escalated";
    return "No-op";
  }

  function summaryClass(o: OutcomeDto): string {
    if (o.kind === "Synced") return "border-ok/40 bg-ok/5";
    if (o.kind === "Escalated") return "border-warn/40 bg-warn/5";
    return "border-border bg-muted/40";
  }

  function badgeVariant(kind: string): BadgeVariant {
    if (kind === "PolicyConflict") return "destructive";
    if (kind === "InvariantViolation") return "warning";
    return "outline";
  }
</script>

{#if outcome}
  <section class="space-y-4">
    <h3 class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Outcome detail</h3>
    <div class={cn("rounded-md border px-4 py-3 text-sm", summaryClass(outcome))}>
      <span class="font-semibold">{strongLabel(outcome)}.</span>
      <span class="text-muted-foreground"> {summaryLine(outcome)}</span>
    </div>

    {#if outcome.conflicts && outcome.conflicts.length}
      <div class="space-y-2">
        <h4 class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Conflicts</h4>
        <ul class="space-y-2">
          {#each outcome.conflicts as c (c.path + c.reason)}
            <li>
              <Card>
                <CardContent class="flex flex-col gap-1 pt-4">
                  <div class="flex items-center gap-2">
                    <span class="font-mono text-sm">{c.path}</span>
                    <Badge variant={badgeVariant(c.class)}>{c.class}</Badge>
                  </div>
                  <p class="text-sm text-muted-foreground">{c.reason}</p>
                </CardContent>
              </Card>
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    {#if outcome.kind === "Synced" && wouldWrite !== null}
      <div class="space-y-2">
        <h4 class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Merged CIF (would write)</h4>
        <pre class="max-h-[320px] overflow-auto rounded-md bg-muted/50 p-3 font-mono text-xs leading-snug">{JSON.stringify(wouldWrite, null, 2)}</pre>
      </div>
    {/if}

    {#if context}
      <FieldChangelog {outcome} {context} />
    {/if}
  </section>
{/if}
