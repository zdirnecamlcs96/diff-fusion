<script lang="ts">
  import { wizard, setStatus } from "../../lib/wizardState.svelte";
  import { suggestPolicyFromSchema } from "../../lib/runFlow";
  import { Button } from "../../lib/components/ui/button";
  import { Label } from "../../lib/components/ui/label";
  import { Textarea } from "../../lib/components/ui/textarea";

  async function suggest() {
    wizard.suggesting = true;
    try {
      wizard.policy = await suggestPolicyFromSchema(wizard.cifSchema);
      setStatus("Wrote suggested policy. Tweak as needed.", "ok");
    } catch (e: any) {
      setStatus(e.message, "err");
    } finally {
      wizard.suggesting = false;
    }
  }
</script>

<section class="space-y-3">
  <div class="space-y-1">
    <h3 class="text-base font-medium">Merge policy</h3>
    <p class="text-sm text-muted-foreground">
      Per-field merge rules. Click <span class="font-medium text-foreground">Suggest</span> to seed from
      the schema (calls the same <code class="rounded bg-muted px-1 py-0.5 font-mono text-xs">suggest_policies</code> the CLI uses).
    </p>
  </div>
  <div class="space-y-1.5">
    <div class="flex items-center justify-between">
      <Label for="policy">Per-field declarations</Label>
      <Button variant="outline" size="sm" onclick={suggest} disabled={wizard.suggesting}>
        {wizard.suggesting ? "Suggesting…" : "Suggest from schema"}
      </Button>
    </div>
    <Textarea id="policy" class="min-h-[280px]" spellcheck="false" bind:value={wizard.policy} />
  </div>
</section>
