<script lang="ts">
  import { wizard } from "../../lib/wizardState.svelte";
  import { Input } from "../../lib/components/ui/input";
  import { Label } from "../../lib/components/ui/label";
  import { Card, CardContent } from "../../lib/components/ui/card";
  import { cn } from "../../lib/utils";

  function lineCount(s: string): number {
    if (!s.trim()) return 0;
    return s.split("\n").length;
  }
  function ok(s: string): boolean {
    return s.trim().length > 0;
  }

  const items = $derived([
    {
      key: "Schema",
      meta: `${lineCount(wizard.cifSchema)} lines`,
      filled: ok(wizard.cifSchema),
    },
    {
      key: "Policy",
      meta: `${lineCount(wizard.policy)} lines`,
      filled: ok(wizard.policy),
    },
    {
      key: "Transformers",
      meta: `${wizard.systemAName} ↔ ${wizard.systemBName}`,
      filled: ok(wizard.transformerA) && ok(wizard.transformerB),
    },
    {
      key: "Data",
      meta: `A: ${lineCount(wizard.systemA)} · B: ${lineCount(wizard.systemB)} · ancestor: ${ok(wizard.ancestor) ? `${lineCount(wizard.ancestor)} lines` : "none"}`,
      filled: ok(wizard.systemA) && ok(wizard.systemB),
    },
  ]);
</script>

<section class="space-y-5">
  <div class="space-y-1">
    <h3 class="text-base font-medium">Review &amp; run</h3>
    <p class="text-sm text-muted-foreground">
      Final check before kicking off the pipeline. The dialog will animate through transform → diff
      → policy → outcome as Rust streams progress events.
    </p>
  </div>

  <div class="space-y-1.5">
    <Label for="test-name">Test name <span class="text-muted-foreground">(shown in the run dialog)</span></Label>
    <Input id="test-name" bind:value={wizard.testName} />
  </div>

  <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
    {#each items as it (it.key)}
      <Card class={cn("transition-colors", it.filled ? "border-ok/40" : "")}>
        <CardContent class="space-y-1 pt-4">
          <p
            class={cn(
              "text-xs uppercase tracking-wider",
              it.filled ? "text-ok" : "text-muted-foreground",
            )}
          >{it.key}</p>
          <p class="font-mono text-sm">{it.meta}</p>
        </CardContent>
      </Card>
    {/each}
  </div>
</section>
