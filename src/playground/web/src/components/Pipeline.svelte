<script lang="ts">
  import { renderStageBody } from "../lib/render";
  import type { StagesDto } from "../lib/types";
  import { cn } from "../lib/utils";

  let { stages, activeStep = 5 }: { stages: StagesDto; activeStep?: number } = $props();

  function bodyText(key: string, data: unknown): string {
    return renderStageBody(key, data);
  }

  function fmtMs(ms: number | null | undefined): string {
    if (ms == null) return "";
    if (ms < 1) return "<1 ms";
    if (ms < 1000) return `${ms} ms`;
    return `${(ms / 1000).toFixed(2)} s`;
  }

  type StageState = "pending" | "active" | "done";
  function stageState(stageNumber: number): StageState {
    if (activeStep === stageNumber) return "active";
    if (activeStep > stageNumber) return "done";
    return "pending";
  }

  const stageBox = (state: StageState) =>
    cn(
      "flex flex-col rounded-md border bg-card p-3 transition-colors",
      state === "active" && "border-primary/60 ring-1 ring-primary/30",
      state === "done" && "border-ok/40",
      state === "pending" && "border-border opacity-60",
    );

  const dot = (state: StageState) =>
    cn(
      "h-2 w-2 rounded-full transition-colors",
      state === "active" && "bg-primary animate-pulse",
      state === "done" && "bg-ok",
      state === "pending" && "bg-muted-foreground/40",
    );

  const timing = (state: StageState) =>
    cn(
      "ml-auto rounded bg-muted px-1.5 py-0.5 font-mono text-[11px]",
      state === "active" && "text-primary",
      state === "done" && "text-ok",
      state === "pending" && "text-muted-foreground",
    );

  const body =
    "mt-2 max-h-[260px] overflow-auto whitespace-pre-wrap break-words rounded bg-muted/50 p-2 font-mono text-[11px] leading-snug text-foreground/90";

  const arrow = (lit: boolean) =>
    cn(
      "mx-2 h-px flex-1 transition-colors",
      lit ? "bg-primary/50" : "bg-border",
    );
</script>

<section class="space-y-3">
  <h3 class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Pipeline</h3>
  <div class="flex items-stretch gap-2 overflow-x-auto pb-1">
    <!-- Parallel transforms group -->
    <div class="relative flex w-[320px] shrink-0 flex-col gap-2 rounded-md border border-dashed border-border p-2">
      <span class="absolute -top-2 left-3 bg-card px-1.5 font-mono text-[10px] uppercase tracking-wider text-muted-foreground">parallel</span>
      <div class={stageBox(stageState(1))}>
        <div class="flex items-center gap-2">
          <span class={dot(stageState(1))}></span>
          <span class="text-sm font-medium">1a. Transform A → CIF</span>
          {#if stages.timings?.transform_a_ms != null}
            <span class={timing(stageState(1))}>{fmtMs(stages.timings.transform_a_ms)}</span>
          {/if}
        </div>
        <pre class={body}>{stages.transform_a ? bodyText("transform_a", stages.transform_a) : ""}</pre>
      </div>
      <div class={stageBox(stageState(1))}>
        <div class="flex items-center gap-2">
          <span class={dot(stageState(1))}></span>
          <span class="text-sm font-medium">1b. Transform B → CIF</span>
          {#if stages.timings?.transform_b_ms != null}
            <span class={timing(stageState(1))}>{fmtMs(stages.timings.transform_b_ms)}</span>
          {/if}
        </div>
        <pre class={body}>{stages.transform_b ? bodyText("transform_b", stages.transform_b) : ""}</pre>
      </div>
    </div>
    <div class="flex items-center"><span class={arrow(activeStep > 1)}></span></div>

    <div class={cn(stageBox(stageState(2)), "w-[320px] shrink-0")}>
      <div class="flex items-center gap-2">
        <span class={dot(stageState(2))}></span>
        <span class="text-sm font-medium">2. 3-way Diff</span>
        {#if stages.timings?.diff_ms != null}
          <span class={timing(stageState(2))}>{fmtMs(stages.timings.diff_ms)}</span>
        {/if}
      </div>
      <pre class={body}>{stages.diff ? bodyText("diff", stages.diff) : ""}</pre>
    </div>
    <div class="flex items-center"><span class={arrow(activeStep > 2)}></span></div>

    <div class={cn(stageBox(stageState(3)), "w-[320px] shrink-0")}>
      <div class="flex items-center gap-2">
        <span class={dot(stageState(3))}></span>
        <span class="text-sm font-medium">3. Policy Resolution</span>
        {#if stages.timings?.policy_ms != null}
          <span class={timing(stageState(3))}>{fmtMs(stages.timings.policy_ms)}</span>
        {/if}
      </div>
      <pre class={body}>{stages.policy ? bodyText("policy", stages.policy) : ""}</pre>
    </div>
    <div class="flex items-center"><span class={arrow(activeStep > 3)}></span></div>

    <div class={cn(stageBox(stageState(4)), "w-[260px] shrink-0")}>
      <div class="flex items-center gap-2">
        <span class={dot(stageState(4))}></span>
        <span class="text-sm font-medium">4. Outcome</span>
        {#if stages.timings?.outcome_ms != null}
          <span class={timing(stageState(4))}>{fmtMs(stages.timings.outcome_ms)}</span>
        {/if}
      </div>
      <pre class={body}>{stages.outcome ? bodyText("outcome", stages.outcome) : ""}</pre>
    </div>
  </div>

  {#if stages.timings && stages.timings.total_ms > 0}
    <div class="flex justify-end gap-3 border-t border-dashed border-border pt-2 font-mono text-[11px]">
      <span>total · {fmtMs(stages.timings.total_ms)}</span>
      {#if stages.timings.transform_a_ms != null && stages.timings.transform_b_ms != null}
        <span class="text-muted-foreground">
          (parallel transforms: max {fmtMs(Math.max(stages.timings.transform_a_ms, stages.timings.transform_b_ms))})
        </span>
      {/if}
    </div>
  {/if}
</section>
