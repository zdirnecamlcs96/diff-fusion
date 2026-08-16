<script lang="ts">
  import type { OutcomeDto, RunContext } from "../lib/types";
  import { buildFieldChangelogRows } from "../lib/render";
  import { cn } from "../lib/utils";

  let { outcome, context }: { outcome: OutcomeDto; context: RunContext } = $props();

  let rows = $derived.by(() => {
    if (outcome.kind !== "Synced" || !context.would_write) return [];
    return buildFieldChangelogRows(
      context.ancestor,
      context.cif_a,
      context.cif_b,
      context.would_write,
      context.policy_per_field as any,
      context.system_a_name,
      context.system_b_name,
    );
  });

  function diffLineClass(type: string): string {
    if (type === "add") return "text-ok";
    if (type === "rm") return "text-destructive";
    return "text-muted-foreground";
  }

  function winnerClass(w: string): string {
    if (w === "A") return "text-primary";
    if (w === "B") return "text-primary";
    if (w === "ancestor") return "text-muted-foreground";
    if (w === "merged") return "text-ok";
    return "text-foreground";
  }
</script>

{#if rows.length > 0}
  <div class="space-y-2">
    <h4 class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
      Field changelog <span class="ml-1 normal-case text-muted-foreground/80">(from → to per field)</span>
    </h4>
    <div class="overflow-x-auto rounded-md border border-border">
      <table class="w-full border-collapse text-xs">
        <thead class="bg-muted/30">
          <tr class="text-left">
            <th class="px-3 py-2 font-medium">Path</th>
            <th class="px-3 py-2 font-medium">Policy</th>
            <th class="px-3 py-2 font-medium">Ancestor</th>
            <th class="px-3 py-2 font-medium">System A ({context.system_a_name})</th>
            <th class="px-3 py-2 font-medium">System B ({context.system_b_name})</th>
            <th class="px-3 py-2 font-medium">Written</th>
            <th class="px-3 py-2 font-medium">Winner</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-border">
          {#each rows as r (r.path)}
            <tr class="align-top">
              <td class="px-3 py-2 font-mono">{r.path}</td>
              <td class="px-3 py-2">
                <div class={cn("font-mono", r.policy.kind === "(none)" ? "text-muted-foreground" : "text-foreground")}>
                  {r.policy.kind}
                </div>
                {#if r.policy.detailLines.length}
                  <div class="mt-0.5 space-y-0.5 font-mono text-[11px] text-muted-foreground">
                    {#each r.policy.detailLines as line}<div>{line}</div>{/each}
                  </div>
                {/if}
              </td>
              <td class="px-3 py-2 font-mono text-muted-foreground">
                {#if r.ancestor.cell.asPlain !== null}{r.ancestor.cell.asPlain}{:else}
                  {#each r.ancestor.cell.lines as line}
                    <div class={cn("flex gap-1", diffLineClass(line.type))}>
                      <span>{line.type === "rm" ? "-" : " "}</span>
                      <span>{line.text}</span>
                    </div>
                  {/each}
                {/if}
              </td>
              <td class={cn("px-3 py-2 font-mono", r.systemA.isWinner && "bg-primary/5")}>
                {#if r.systemA.cell.asPlain !== null}{r.systemA.cell.asPlain}{:else}
                  {#each r.systemA.cell.lines as line}
                    <div class={cn("flex gap-1", diffLineClass(line.type))}>
                      <span>{line.type === "add" ? "+" : " "}</span>
                      <span>{line.text}</span>
                    </div>
                  {/each}
                {/if}
              </td>
              <td class={cn("px-3 py-2 font-mono", r.systemB.isWinner && "bg-primary/5")}>
                {#if r.systemB.cell.asPlain !== null}{r.systemB.cell.asPlain}{:else}
                  {#each r.systemB.cell.lines as line}
                    <div class={cn("flex gap-1", diffLineClass(line.type))}>
                      <span>{line.type === "add" ? "+" : " "}</span>
                      <span>{line.text}</span>
                    </div>
                  {/each}
                {/if}
              </td>
              <td class="px-3 py-2 font-mono">
                {#if r.written.cell.asPlain !== null}{r.written.cell.asPlain}{:else}
                  {#each r.written.cell.lines as line}
                    <div class={cn("flex gap-1", diffLineClass(line.type))}>
                      <span>{line.type === "add" ? "+" : " "}</span>
                      <span>{line.text}</span>
                    </div>
                  {/each}
                {/if}
                {#if r.sbkDigest}
                  <div class="mt-1 space-y-0.5 text-[10px]">
                    {#each r.sbkDigest as d}
                      <div class={cn("font-mono", diffLineClass(d.status === "added" ? "add" : d.status === "removed" ? "rm" : ""))}>
                        [{d.status}] {d.label}{d.note}
                      </div>
                    {/each}
                  </div>
                {/if}
              </td>
              <td class={cn("px-3 py-2 font-mono", winnerClass(r.winner))}>
                <div>{r.winner}</div>
                {#if r.hint}<div class="text-[10px] text-muted-foreground">{r.hint}</div>{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
{/if}
