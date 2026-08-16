<script lang="ts">
  import { cn } from "../lib/utils";

  interface Step {
    key: string;
    label: string;
  }
  let {
    steps,
    current,
    ongoto,
  }: {
    steps: readonly Step[];
    current: number;
    ongoto: (i: number) => void;
  } = $props();
</script>

<nav aria-label="New test progress">
  <ol class="flex items-center">
    {#each steps as s, i (s.key)}
      {@const state = i < current ? "done" : i === current ? "active" : "todo"}
      <li class={cn("flex items-center", i < steps.length - 1 ? "flex-1" : "flex-none")}>
        <button
          type="button"
          aria-current={i === current ? "step" : undefined}
          aria-label={`Go to step ${i + 1}: ${s.label}`}
          onclick={() => ongoto(i)}
          class={cn(
            "group flex items-center gap-2 transition-colors",
            "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring rounded-md",
          )}
        >
          <span
            class={cn(
              "flex h-7 w-7 shrink-0 items-center justify-center rounded-full border text-xs font-mono font-semibold transition-colors",
              state === "active" &&
                "border-primary bg-primary text-primary-foreground ring-4 ring-primary/20",
              state === "done" && "border-ok/40 bg-ok/15 text-ok",
              state === "todo" && "border-border bg-card text-muted-foreground",
            )}
          >
            {#if state === "done"}✓{:else}{i + 1}{/if}
          </span>
          <span
            class={cn(
              "whitespace-nowrap text-sm",
              state === "active" && "font-medium text-foreground",
              state === "done" && "text-foreground",
              state === "todo" && "text-muted-foreground",
            )}
          >{s.label}</span>
        </button>
        {#if i < steps.length - 1}
          <span
            aria-hidden="true"
            class={cn(
              "mx-3 h-px flex-1",
              state === "done" || state === "active" ? "bg-primary/40" : "bg-border",
            )}
          ></span>
        {/if}
      </li>
    {/each}
  </ol>
</nav>
