<script setup lang="ts">
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight } from "@lucide/vue";

const page = defineModel<number>({ required: true });
const props = withDefaults(
  defineProps<{ total: number; pageSize?: number; disabled?: boolean }>(),
  { pageSize: 50 },
);
const pages = computed(() => Math.max(1, Math.ceil(props.total / props.pageSize)));
const first = computed(() => (props.total === 0 ? 0 : (page.value - 1) * props.pageSize + 1));
const last = computed(() => Math.min(page.value * props.pageSize, props.total));
function go(value: number) {
  if (!props.disabled) page.value = Math.min(pages.value, Math.max(1, value));
}
</script>

<template>
  <nav
    aria-label="列表分页"
    class="flex min-w-0 flex-wrap items-center justify-between gap-2 text-sm"
  >
    <span class="text-xs text-muted-foreground tabular-nums" role="status"
      >第 {{ first }}–{{ last }} 条，共 {{ total }} 条</span
    >
    <div class="flex shrink-0 items-center gap-1">
      <Button
        variant="outline"
        size="icon"
        aria-label="第一页"
        title="第一页"
        :disabled="disabled || page <= 1"
        @click="go(1)"
        ><ChevronsLeft aria-hidden="true"
      /></Button>
      <Button
        variant="outline"
        size="icon"
        aria-label="上一页"
        title="上一页"
        :disabled="disabled || page <= 1"
        @click="go(page - 1)"
        ><ChevronLeft aria-hidden="true"
      /></Button>
      <span class="min-w-14 px-1 text-center text-xs tabular-nums">{{ page }} / {{ pages }}</span>
      <Button
        variant="outline"
        size="icon"
        aria-label="下一页"
        title="下一页"
        :disabled="disabled || page >= pages"
        @click="go(page + 1)"
        ><ChevronRight aria-hidden="true"
      /></Button>
      <Button
        variant="outline"
        size="icon"
        aria-label="最后一页"
        title="最后一页"
        :disabled="disabled || page >= pages"
        @click="go(pages)"
        ><ChevronsRight aria-hidden="true"
      /></Button>
    </div>
  </nav>
</template>
