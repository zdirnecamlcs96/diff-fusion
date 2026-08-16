<script lang="ts">
  import { fade } from "svelte/transition";
  import { runState } from "../lib/runState.svelte";
  import Pipeline from "./Pipeline.svelte";
  import OutcomeDetail from "./OutcomeDetail.svelte";
  import { Button } from "../lib/components/ui/button";

  let { open, ondismiss }: { open: boolean; ondismiss: () => void } = $props();

  const MIN_STEP_MS = 280;

  let dialogEl: HTMLDialogElement;
  let revealedStep = $state(0);
  let showOutcome = $state(false);

  let targetStep = $derived.by(() => {
    const s = runState.stages;
    if (s.outcome) return 5;
    if (s.policy) return 4;
    if (s.diff) return 3;
    if (s.transform_a && s.transform_b) return 2;
    return 1;
  });

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) dialogEl.showModal();
    else if (!open && dialogEl.open) dialogEl.close();
  });

  $effect(() => {
    if (!open) {
      revealedStep = 0;
      showOutcome = false;
    }
  });

  $effect(() => {
    if (!open) return;
    if (revealedStep >= targetStep) return;
    const t = window.setTimeout(() => {
      revealedStep = revealedStep + 1;
    }, MIN_STEP_MS);
    return () => window.clearTimeout(t);
  });

  $effect(() => {
    if (!open) return;
    if (revealedStep < 5) {
      showOutcome = false;
      return;
    }
    const t = window.setTimeout(() => {
      showOutcome = true;
    }, 120);
    return () => window.clearTimeout(t);
  });

  function onClose() {
    ondismiss();
  }

  function onBackdropClick(e: MouseEvent) {
    if (e.target === dialogEl) ondismiss();
  }
</script>

<dialog
  bind:this={dialogEl}
  class="run-dialog max-h-[90vh] w-[min(1280px,96vw)] overflow-hidden rounded-xl border border-border bg-card p-0 text-card-foreground shadow-xl backdrop:bg-black/60 backdrop:backdrop-blur-sm"
  onclose={onClose}
  onclick={onBackdropClick}
>
  <header class="sticky top-0 z-10 flex items-start justify-between border-b border-border bg-card/95 px-6 py-4 backdrop-blur supports-[backdrop-filter]:bg-card/80">
    <div class="space-y-0.5">
      <h2 class="text-base font-semibold tracking-tight">{runState.title || "Run details"}</h2>
      {#if runState.subtitle}
        <p class="font-mono text-xs text-muted-foreground">{runState.subtitle}</p>
      {/if}
    </div>
    <Button variant="ghost" size="icon" onclick={ondismiss} aria-label="Close" class="h-8 w-8 text-muted-foreground hover:text-foreground">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
    </Button>
  </header>
  <div class="max-h-[calc(90vh-64px)] space-y-6 overflow-y-auto px-6 py-6">
    <Pipeline stages={runState.stages} activeStep={revealedStep} />
    {#if showOutcome}
      <div in:fade={{ duration: 320 }}>
        <OutcomeDetail stages={runState.stages} context={runState.context} />
      </div>
    {/if}
  </div>
</dialog>

<style>
  /* dialog needs a tiny escape hatch — the native element does not pick
     up Tailwind's font/color resets through ::backdrop. */
  .run-dialog { color-scheme: dark; }
</style>
