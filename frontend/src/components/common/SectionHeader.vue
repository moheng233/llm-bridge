<script setup lang="ts">
import { cn } from "~/lib/utils";
import { type Component } from "vue";

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
    <div class="flex flex-col gap-1 min-w-0">
      <div class="flex items-center gap-2">
        <component v-if="Icon" :is="Icon" class="h-5 w-5 text-muted-foreground shrink-0" />
        <h2 class="text-xl font-bold font-mono text-foreground truncate">{{ title }}</h2>
        <Badge
          v-if="count !== null && count !== undefined"
          variant="secondary"
          class="font-mono shrink-0"
        >
          {{ count }} {{ countLabel }}
        </Badge>
      </div>
      <p v-if="description" class="text-sm text-muted-foreground">{{ description }}</p>
    </div>
    <div v-if="$slots.actions" class="flex items-center gap-2 shrink-0">
      <slot name="actions" />
    </div>
  </div>
</template>
