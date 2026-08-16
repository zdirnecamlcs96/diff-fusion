<script lang="ts">
  import { runState } from "./lib/runState.svelte";
  import Dashboard from "./views/Dashboard.svelte";
  import NewTest from "./views/NewTest.svelte";
  import Observers from "./views/Observers.svelte";
  import RunDialog from "./components/RunDialog.svelte";
  import { Button } from "./lib/components/ui/button";
  import { cn } from "./lib/utils";

  type View = "dashboard" | "new-test" | "observers";
  let view: View = $state("dashboard");

  const navItems: { key: View; label: string }[] = [
    { key: "dashboard", label: "Dashboard" },
    { key: "new-test", label: "New test" },
    { key: "observers", label: "Observers" },
  ];
</script>

<div class="min-h-screen bg-background text-foreground">
  <header class="sticky top-0 z-30 w-full border-b border-border/40 bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
    <div class="mx-auto flex h-14 max-w-screen-xl items-center px-6">
      <div class="mr-6 flex items-center space-x-2">
        <span class="font-semibold tracking-tight">diff-fusion</span>
        <span class="text-muted-foreground">·</span>
        <span class="text-sm text-muted-foreground">playground</span>
      </div>
      <nav class="flex items-center gap-1 text-sm">
        {#each navItems as item (item.key)}
          <Button
            variant="ghost"
            size="sm"
            class={cn(
              "h-8 px-3 text-sm font-medium transition-colors",
              view === item.key
                ? "bg-accent text-accent-foreground"
                : "text-muted-foreground hover:text-foreground",
            )}
            onclick={() => (view = item.key)}
          >
            {item.label}
          </Button>
        {/each}
      </nav>
    </div>
  </header>

  <main class="mx-auto max-w-screen-xl px-6 py-8">
    {#if view === "dashboard"}
      <Dashboard onstart={() => (view = "new-test")} />
    {:else if view === "new-test"}
      <NewTest onback={() => (view = "dashboard")} />
    {:else}
      <Observers />
    {/if}
  </main>
</div>

<RunDialog open={runState.dialogOpen} ondismiss={() => (runState.dialogOpen = false)} />
