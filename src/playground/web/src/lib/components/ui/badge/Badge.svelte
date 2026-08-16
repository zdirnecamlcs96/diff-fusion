<script lang="ts" module>
  export type BadgeVariant = "default" | "secondary" | "destructive" | "outline" | "success" | "warning";
</script>

<script lang="ts">
  import type { HTMLAttributes } from "svelte/elements";
  import type { Snippet } from "svelte";
  import { cn } from "../../../utils";

  interface Props extends HTMLAttributes<HTMLSpanElement> {
    variant?: BadgeVariant;
    class?: string;
    children?: Snippet;
  }

  let { variant = "default", class: className, children, ...rest }: Props = $props();

  const variants: Record<BadgeVariant, string> = {
    default:
      "border-transparent bg-primary text-primary-foreground shadow hover:bg-primary/80",
    secondary:
      "border-transparent bg-secondary text-secondary-foreground hover:bg-secondary/80",
    destructive:
      "border-transparent bg-destructive text-destructive-foreground shadow hover:bg-destructive/80",
    outline: "text-foreground",
    success:
      "border-transparent bg-ok/15 text-ok border-ok/30",
    warning:
      "border-transparent bg-warn/15 text-warn border-warn/30",
  };

  const base =
    "inline-flex items-center rounded-md border px-2 py-0.5 text-xs font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2";
</script>

<span class={cn(base, variants[variant], className)} {...rest}>
  {@render children?.()}
</span>
