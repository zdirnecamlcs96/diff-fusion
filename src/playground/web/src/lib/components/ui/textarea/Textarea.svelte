<script lang="ts">
  import type { HTMLTextareaAttributes } from "svelte/elements";
  import { cn } from "../../../utils";

  interface Props extends HTMLTextareaAttributes {
    class?: string;
    value?: string | null;
  }

  let { class: className, value = $bindable(), ...rest }: Props = $props();
</script>

<textarea
  bind:value
  class={cn(
    "flex min-h-[140px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm font-mono shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
    className,
  )}
  {...rest}
></textarea>
