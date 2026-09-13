<script setup lang="ts">
import { SlidersHorizontal, X } from "@lucide/vue";
import { useMediaQuery } from "@vueuse/core";
import { PopoverRoot, PopoverTrigger, PopoverPortal, PopoverContent, PopoverClose } from "reka-ui";

const props = defineProps<{ collapsed?: boolean }>();
const compactViewport = useMediaQuery("(max-width: 767px), (max-height: 640px)");
const compact = computed(() => props.collapsed || compactViewport.value);
const open = ref(false);
watch(compact, () => {
  open.value = false;
});
</script>

<template>
  <PopoverRoot v-model:open="open">
    <div class="flex min-w-0 flex-wrap items-center gap-2">
      <div class="flex min-w-0 flex-1 items-center gap-2"><slot name="primary" /></div>
      <PopoverTrigger v-if="compact && $slots.default" as-child>
        <Button variant="outline" size="icon" aria-label="筛选与排序" title="筛选与排序"
          ><SlidersHorizontal
        /></Button>
      </PopoverTrigger>
      <div v-else-if="$slots.default" class="flex flex-wrap items-center gap-2"><slot /></div>
      <slot name="actions" />
    </div>
    <PopoverPortal v-if="compact && $slots.default">
      <PopoverContent
        align="end"
        :side-offset="8"
        :collision-padding="12"
        class="z-50 flex max-h-[min(70dvh,var(--reka-popover-content-available-height))] w-80 max-w-[calc(100vw-24px)] flex-col overflow-hidden rounded-md border bg-popover text-popover-foreground shadow-md"
      >
        <div class="flex shrink-0 items-center justify-between border-b px-3 py-1">
          <h2 class="text-base">筛选与排序</h2>
          <PopoverClose as-child
            ><Button variant="ghost" size="icon" aria-label="关闭筛选"><X /></Button
          ></PopoverClose>
        </div>
        <div
          data-scroll-area
          tabindex="0"
          class="flex min-h-0 flex-wrap items-center gap-3 overflow-y-auto overscroll-contain p-3"
        >
          <slot />
        </div>
      </PopoverContent>
    </PopoverPortal>
  </PopoverRoot>
</template>
