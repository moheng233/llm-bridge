<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { cn, type WithElementRef } from "$lib/utils.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { RefreshCw, AlertCircle } from "@lucide/svelte";
  import { formatApiError } from "$lib/utils/error";

  type Props = WithElementRef<HTMLAttributes<HTMLDivElement>> & {
    /** 错误对象（unknown，来自 catch）或字符串 */
    error: unknown;
    /** 重试回调；不传则不显示重试按钮 */
    onRetry?: () => void;
    /** 是否紧凑展示（用于内联，默认占据剩余空间居中） */
    inline?: boolean;
  };

  let {
    ref = $bindable(null),
    class: className,
    error,
    onRetry,
    inline = false,
    ...restProps
  }: Props = $props();

  let formatted = $derived(formatApiError(error));
</script>

<div
  bind:this={ref}
  data-slot="error-state"
  class={cn(
    inline
      ? "flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3"
      : "flex flex-1 items-center justify-center",
    className,
  )}
  {...restProps}
>
  {#if inline}
    <AlertCircle class="h-4 w-4 text-destructive shrink-0" />
    <div class="flex flex-col gap-0.5 min-w-0">
      <span class="text-sm font-medium text-destructive">{formatted.title}</span>
      {#if formatted.detail && formatted.detail !== formatted.title}
        <span class="text-xs text-muted-foreground truncate" title={formatted.detail}>
          {formatted.detail}
        </span>
      {/if}
    </div>
    {#if onRetry}
      <Button size="sm" variant="ghost" class="ml-auto shrink-0 cursor-pointer" onclick={onRetry}>
        <RefreshCw class="h-3.5 w-3.5" />
        重试
      </Button>
    {/if}
  {:else}
    <div class="flex flex-col items-center gap-3 text-center">
      <AlertCircle class="h-12 w-12 text-destructive/40" />
      <div class="flex flex-col gap-1">
        <p class="text-sm font-medium text-destructive">{formatted.title}</p>
        {#if formatted.detail && formatted.detail !== formatted.title}
          <p class="text-xs text-muted-foreground max-w-md" title={formatted.detail}>
            {formatted.detail}
          </p>
        {/if}
      </div>
      {#if onRetry}
        <Button size="sm" variant="outline" class="cursor-pointer" onclick={onRetry}>
          <RefreshCw class="h-3.5 w-3.5" />
          重试
        </Button>
      {/if}
    </div>
  {/if}
</div>
