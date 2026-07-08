<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { cn, type WithElementRef } from "$lib/utils.js";

  let {
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> & {
    children: Snippet;
  } = $props();
</script>

<div
  data-slot="page-shell"
  class={cn("flex h-full min-h-0 flex-col gap-4", className)}
  {...restProps}
>
  {@render children?.()}
</div>
