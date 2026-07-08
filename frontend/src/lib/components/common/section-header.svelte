<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Component } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { cn, type WithElementRef } from "$lib/utils.js";
  import { Badge } from "$lib/components/ui/badge/index.js";

  type Props = WithElementRef<HTMLAttributes<HTMLDivElement>> & {
    title: string;
    description?: string;
    /** 右上角计数 badge，传数字则显示 */
    count?: number | null;
    countLabel?: string;
    /** 标题前图标组件 */
    icon?: Component<{ class?: string }>;
    /** 右侧操作区（按钮） */
    actions?: Snippet;
  };

  let {
    ref = $bindable(null),
    class: className,
    title,
    description,
    count = null,
    countLabel = "个",
    icon: Icon,
    actions,
    ...restProps
  }: Props = $props();
</script>

<div
  bind:this={ref}
  data-slot="section-header"
  class={cn("flex items-center justify-between gap-3", className)}
  {...restProps}
>
  <div class="flex flex-col gap-1 min-w-0">
    <div class="flex items-center gap-2">
      {#if Icon}
        <Icon class="h-5 w-5 text-muted-foreground shrink-0" />
      {/if}
      <h2 class="text-xl font-bold font-mono text-foreground truncate">{title}</h2>
      {#if count !== null && count !== undefined}
        <Badge variant="secondary" class="font-mono shrink-0">
          {count} {countLabel}
        </Badge>
      {/if}
    </div>
    {#if description}
      <p class="text-sm text-muted-foreground">{description}</p>
    {/if}
  </div>
  {#if actions}
    <div class="flex items-center gap-2 shrink-0">
      {@render actions()}
    </div>
  {/if}
</div>
