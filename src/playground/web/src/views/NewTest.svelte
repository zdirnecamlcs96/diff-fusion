<script lang="ts">
  import Stepper from "../components/Stepper.svelte";
  import SchemaStep from "../components/steps/SchemaStep.svelte";
  import PolicyStep from "../components/steps/PolicyStep.svelte";
  import TransformersStep from "../components/steps/TransformersStep.svelte";
  import DataStep from "../components/steps/DataStep.svelte";
  import ReviewStep from "../components/steps/ReviewStep.svelte";
  import {
    STEPS,
    gotoStep,
    initSample,
    isLastStep,
    nextStep,
    prevStep,
    setStatus,
    toTestRecord,
    wizard,
  } from "../lib/wizardState.svelte";
  import { runSync } from "../lib/runFlow";
  import { putTest } from "../lib/api";
  import { Button } from "../lib/components/ui/button";
  import { Separator } from "../lib/components/ui/separator";
  import { AlertDialog } from "../lib/components/ui/alert-dialog";
  import { cn } from "../lib/utils";

  let { onback }: { onback: () => void } = $props();

  let showExampleConfirm: boolean = $state(false);

  function confirmLoadExample() {
    showExampleConfirm = false;
    initSample();
    gotoStep(0);
  }

  async function handleRun() {
    wizard.running = true;
    try {
      if (!wizard.testId) {
        wizard.testId = `test-${Date.now()}`;
      }
      const testId = wizard.testId;
      const record = toTestRecord();

      try {
        await putTest(testId, record);
      } catch (e: any) {
        setStatus(`Save failed: ${e.message}`, "err");
      }

      const res = await runSync({
        fields: {
          systemA: wizard.systemA,
          systemB: wizard.systemB,
          cifSchema: wizard.cifSchema,
          policy: wizard.policy,
          ancestor: wizard.ancestor,
          transformerA: wizard.transformerA,
          transformerB: wizard.transformerB,
          systemAName: wizard.systemAName,
          systemBName: wizard.systemBName,
        },
        setStatus,
        title: wizard.testName?.trim() || "New Test · Run Sync",
      });

      const outcome = res?.error ? "Error" : (res?.stages?.outcome?.kind ?? null);
      try {
        await putTest(testId, { ...record, last_outcome: outcome });
      } catch {
        // status already shows the run result
      }
    } finally {
      wizard.running = false;
    }
  }
</script>

<div class="space-y-6">
  <div class="flex items-baseline justify-between gap-3">
    <div class="flex items-baseline gap-3">
      <Button variant="link" size="sm" class="px-0 text-muted-foreground hover:text-foreground" onclick={onback}>
        ← Back to dashboard
      </Button>
      <h2 class="text-xl font-semibold tracking-tight">{wizard.testName || "New test"}</h2>
    </div>
    <Button variant="outline" size="sm" onclick={() => (showExampleConfirm = true)}>
      Load example
    </Button>
  </div>

  <Stepper steps={STEPS} current={wizard.currentStep} ongoto={gotoStep} />

  <div class="min-h-[260px]">
    {#if wizard.currentStep === 0}
      <SchemaStep />
    {:else if wizard.currentStep === 1}
      <PolicyStep />
    {:else if wizard.currentStep === 2}
      <TransformersStep />
    {:else if wizard.currentStep === 3}
      <DataStep />
    {:else}
      <ReviewStep />
    {/if}
  </div>

  <Separator />

  <div class="flex items-center gap-4">
    <Button variant="outline" onclick={prevStep} disabled={wizard.currentStep === 0}>
      ← Back
    </Button>
    <p
      class={cn(
        "flex-1 text-sm",
        wizard.statusTone === "ok" && "text-ok",
        wizard.statusTone === "err" && "text-destructive",
        wizard.statusTone === "" && "text-muted-foreground",
      )}
    >{wizard.status}</p>
    {#if isLastStep()}
      <Button variant="default" onclick={handleRun} disabled={wizard.running}>
        {wizard.running ? "Running…" : "Run Sync ▶"}
      </Button>
    {:else}
      <Button variant="default" onclick={nextStep}>Next →</Button>
    {/if}
  </div>
</div>

<AlertDialog
  open={showExampleConfirm}
  title="Load example data?"
  description="This will overwrite the current form with the bundled sample test (PO fulfillment across erp ↔ inv) and reset the stepper to step 1."
  cancelLabel="Cancel"
  actionLabel="Overwrite and load"
  onCancel={() => (showExampleConfirm = false)}
  onAction={confirmLoadExample}
/>
