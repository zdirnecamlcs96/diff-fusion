<script lang="ts">
  /** Confirmation dialog modeled on shadcn's alert-dialog. Backed by a
   *  native <dialog> for focus trap + Esc handling; styled with Tailwind
   *  shadcn tokens. The action button uses the `actionVariant` prop so
   *  callers can pick a destructive treatment when appropriate. */
  import type { Snippet } from "svelte";
  import { Button, type ButtonVariant } from "../button";
  import { cn } from "../../../utils";

  interface Props {
    open: boolean;
    title: string;
    description?: string;
    cancelLabel?: string;
    actionLabel?: string;
    actionVariant?: ButtonVariant;
    onCancel: () => void;
    onAction: () => void;
    children?: Snippet;
    class?: string;
  }

  let {
    open,
    title,
    description,
    cancelLabel = "Cancel",
    actionLabel = "Continue",
    actionVariant = "default",
    onCancel,
    onAction,
    children,
    class: className,
  }: Props = $props();

  let dialogEl: HTMLDialogElement;

  $effect(() => {
    if (!dialogEl) return;
    if (open && !dialogEl.open) dialogEl.showModal();
    else if (!open && dialogEl.open) dialogEl.close();
  });

  function onClose() {
    // Triggered by Esc / backdrop / dialogEl.close().
    if (open) onCancel();
  }

  function onBackdropClick(e: MouseEvent) {
    if (e.target === dialogEl) onCancel();
  }
</script>

<dialog
  bind:this={dialogEl}
  onclose={onClose}
  onclick={onBackdropClick}
  class={cn(
    "max-w-lg rounded-lg border border-border bg-card p-0 text-card-foreground shadow-lg backdrop:bg-black/60 backdrop:backdrop-blur-sm",
    className,
  )}
  style="color-scheme: dark"
>
  <div class="flex flex-col space-y-2 p-6 text-left">
    <h2 class="text-lg font-semibold">{title}</h2>
    {#if description}
      <p class="text-sm text-muted-foreground">{description}</p>
    {/if}
    {#if children}
      <div class="pt-2">{@render children()}</div>
    {/if}
  </div>
  <div class="flex justify-end gap-2 border-t border-border bg-muted/30 px-6 py-4">
    <Button variant="outline" onclick={onCancel}>{cancelLabel}</Button>
    <Button variant={actionVariant} onclick={onAction}>{actionLabel}</Button>
  </div>
</dialog>
