<script setup lang="ts">
import { type Component } from "vue";

import { cn } from "~/lib/utils";

const props = withDefaults(
  defineProps<{
    class?: string;
    title: string;
    description?: string;
    count?: number | null;
    countLabel?: string;
    icon?: Component<{ class?: string }>;
  }>(),
  {
    count: null,
    countLabel: "个",
  },
);

const Icon = props.icon;
</script>

<template>
  <div
    data-slot="section-header"
    :class="cn('flex items-center justify-between gap-3', props.class)"
  >
    <div class="flex min-w-0 flex-col gap-1">
      <div class="flex items-center gap-2">
        <component v-if="Icon" :is="Icon" class="h-5 w-5 shrink-0 text-muted-foreground" />
        <h2 class="truncate font-mono text-xl font-bold text-foreground">{{ title }}</h2>
        <Badge
          v-if="count !== null && count !== undefined"
          variant="secondary"
          class="shrink-0 font-mono"
        >
          {{ count }} {{ countLabel }}
        </Badge>
      </div>
      <p v-if="description" class="text-sm text-muted-foreground">{{ description }}</p>
    </div>
    <div v-if="$slots.actions" class="flex shrink-0 items-center gap-2">
      <slot name="actions" />
    </div>
  </div>
</template>
