<script lang="ts">
  import { wizard } from "../../lib/wizardState.svelte";
  import { Label } from "../../lib/components/ui/label";
  import { Textarea } from "../../lib/components/ui/textarea";
</script>

<section class="space-y-3">
  <div class="space-y-1">
    <h3 class="text-base font-medium">CIF schema</h3>
    <p class="text-sm text-muted-foreground">The canonical shape both systems will be reconciled against.</p>
  </div>
  <div class="space-y-1.5">
    <Label for="cif-schema">Schema JSON</Label>
    <Textarea id="cif-schema" class="min-h-[280px]" spellcheck="false" bind:value={wizard.cifSchema} />
  </div>
</section>
