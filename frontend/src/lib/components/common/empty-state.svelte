<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Component } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { cn, type WithElementRef } from "$lib/utils.js";

  type Props = WithElementRef<HTMLAttributes<HTMLDivElement>> & {
    icon?: Component<{ class?: string }>;
    title: string;
    description?: string;
    /** 可选的 action 区（如"创建"按钮） */
    actions?: Snippet;
  };

  let {
    ref = $bindable(null),
    class: className,
    icon: Icon,
    title,
    description,
    actions,
    ...restProps
  }: Props = $props();
</script>

<div
  bind:this={ref}
  data-slot="empty-state"
  class={cn(
    "flex flex-1 items-center justify-center text-muted-foreground",
    className,
  )}
  {...restProps}
>
  <div class="flex flex-col items-center gap-3">
    {#if Icon}
      <Icon class="h-12 w-12 opacity-30" />
    {/if}
    <p class="text-sm font-medium">{title}</p>
    {#if description}
      <p class="text-xs text-muted-foreground max-w-xs text-center">{description}</p>
    {/if}
    {#if actions}
      <div class="flex items-center gap-2 mt-1">
        {@render actions()}
      </div>
    {/if}
  </div>
</div>
