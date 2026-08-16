<script lang="ts">
  import { wizard } from "../../lib/wizardState.svelte";
  import { Input } from "../../lib/components/ui/input";
  import { Label } from "../../lib/components/ui/label";
  import { Textarea } from "../../lib/components/ui/textarea";
</script>

<section class="space-y-3">
  <div class="space-y-1">
    <h3 class="text-base font-medium">Transformers</h3>
    <p class="text-sm text-muted-foreground">
      One per system. Names must match policy <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">owned_by.system</code>.
    </p>
  </div>
  <div class="grid gap-4 md:grid-cols-2">
    <div class="space-y-1.5">
      <div class="flex items-center justify-between gap-2">
        <Label for="sys-a-name">System A name</Label>
        <Input id="sys-a-name" class="h-7 max-w-[160px] font-mono text-xs" bind:value={wizard.systemAName} />
      </div>
      <Textarea class="min-h-[260px]" spellcheck="false" bind:value={wizard.transformerA} />
    </div>
    <div class="space-y-1.5">
      <div class="flex items-center justify-between gap-2">
        <Label for="sys-b-name">System B name</Label>
        <Input id="sys-b-name" class="h-7 max-w-[160px] font-mono text-xs" bind:value={wizard.systemBName} />
      </div>
      <Textarea class="min-h-[260px]" spellcheck="false" bind:value={wizard.transformerB} />
    </div>
  </div>
</section>
