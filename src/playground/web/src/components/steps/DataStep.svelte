<script lang="ts">
  import { wizard } from "../../lib/wizardState.svelte";
  import { Label } from "../../lib/components/ui/label";
  import { Textarea } from "../../lib/components/ui/textarea";
</script>

<section class="space-y-3">
  <div class="space-y-1">
    <h3 class="text-base font-medium">Data</h3>
    <p class="text-sm text-muted-foreground">Raw JSON from each system plus the optional last-synced ancestor.</p>
  </div>
  <div class="grid gap-4 md:grid-cols-2">
    <div class="space-y-1.5">
      <Label>System A <span class="text-muted-foreground">({wizard.systemAName || "system_a"})</span></Label>
      <Textarea class="min-h-[220px]" spellcheck="false" bind:value={wizard.systemA} />
    </div>
    <div class="space-y-1.5">
      <Label>System B <span class="text-muted-foreground">({wizard.systemBName || "system_b"})</span></Label>
      <Textarea class="min-h-[220px]" spellcheck="false" bind:value={wizard.systemB} />
    </div>
    <div class="space-y-1.5 md:col-span-2">
      <Label>Ancestor <span class="text-muted-foreground">(optional — leave blank for first sync)</span></Label>
      <Textarea class="min-h-[180px]" spellcheck="false" bind:value={wizard.ancestor} />
    </div>
  </div>
</section>
