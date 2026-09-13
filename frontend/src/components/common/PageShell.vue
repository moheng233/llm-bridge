<script setup lang="ts">
import { cn } from "~/lib/utils";

const props = withDefaults(
  defineProps<{ class?: string; bodyClass?: string; scrollable?: boolean; resetKey?: unknown }>(),
  {
    scrollable: true,
  },
);
const body = ref<HTMLElement>();
watch(
  () => props.resetKey,
  async () => {
    await nextTick();
    if (!body.value) return;
    body.value.scrollTop = 0;
    for (const area of body.value.querySelectorAll<HTMLElement>("[data-scroll-area]"))
      area.scrollTop = 0;
  },
);
</script>

<template>
  <div
    data-slot="page-shell"
    :class="
      cn('page-shell flex h-full min-h-0 min-w-0 flex-col gap-4 overflow-hidden', $props.class)
    "
  >
    <header v-if="$slots.header" data-slot="page-header" class="min-w-0 shrink-0 empty:hidden">
      <slot name="header" />
    </header>
    <div v-if="$slots.toolbar" data-slot="page-toolbar" class="min-w-0 shrink-0">
      <slot name="toolbar" />
    </div>
    <div
      ref="body"
      data-slot="page-body"
      :data-scroll-area="scrollable ? '' : undefined"
      :tabindex="scrollable ? 0 : undefined"
      :class="
        cn(
          'min-h-0 min-w-0 flex-1',
          scrollable
            ? 'space-y-4 overflow-auto overscroll-contain p-1'
            : 'flex flex-col overflow-hidden',
          bodyClass,
        )
      "
    >
      <slot />
    </div>
    <footer v-if="$slots.footer" data-slot="page-footer" class="min-w-0 shrink-0 border-t pt-3">
      <slot name="footer" />
    </footer>
  </div>
</template>
