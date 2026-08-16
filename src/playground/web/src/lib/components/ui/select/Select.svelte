<script lang="ts">
  import type { HTMLSelectAttributes } from "svelte/elements";
  import type { Snippet } from "svelte";
  import { cn } from "../../../utils";

  interface Props extends HTMLSelectAttributes {
    class?: string;
    children?: Snippet;
    value?: string | number | null;
  }

  let { class: className, children, value = $bindable(), ...rest }: Props = $props();
</script>

<div class={cn("relative inline-flex w-full", className)}>
  <select
    bind:value
    class={cn(
      "flex h-9 w-full appearance-none rounded-md border border-input bg-transparent px-3 py-1 pr-9 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
    )}
    {...rest}
  >
    {@render children?.()}
  </select>
  <svg
    class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground"
    xmlns="http://www.w3.org/2000/svg"
    width="24"
    height="24"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path d="m6 9 6 6 6-6" />
  </svg>
</div>
