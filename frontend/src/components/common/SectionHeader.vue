<script setup lang="ts">
import { type Component } from "vue";

import { usePageHeader } from "~/composables/usePageHeader";
import { cn } from "~/lib/utils";

const props = withDefaults(
  defineProps<{
    class?: string;
    title: string;
    description?: string;
    subtitle?: string;
    backTo?: string;
    backLabel?: string;
    count?: number | null;
    countLabel?: string;
    icon?: Component;
  }>(),
  {
    count: null,
    countLabel: "个",
  },
);
usePageHeader(() => props.title);
</script>

<template>
  <div
    v-if="subtitle || description || $slots.status || $slots.actions"
    data-slot="section-header"
    :class="cn('flex min-w-0 flex-wrap items-center justify-between gap-3', props.class)"
  >
    <div class="flex min-w-0 flex-1 items-center gap-2">
      <div class="min-w-0 space-y-1">
        <div v-if="$slots.status" class="flex min-w-0 flex-wrap items-center gap-2">
          <slot name="status" />
        </div>
        <p
          v-if="subtitle"
          class="truncate font-mono text-xs text-muted-foreground"
          :title="subtitle"
        >
          {{ subtitle }}
        </p>
        <p v-if="description" class="text-sm text-muted-foreground">{{ description }}</p>
      </div>
    </div>
    <div v-if="$slots.actions" class="flex shrink-0 flex-wrap items-center gap-2">
      <slot name="actions" />
    </div>
  </div>
</template>
