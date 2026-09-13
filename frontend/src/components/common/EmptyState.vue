<script setup lang="ts">
import { cn } from "~/lib/utils";

const props = withDefaults(
  defineProps<{
    class?: string;
    icon?: any;
    title: string;
    description?: string;
  }>(),
  {},
);

const Icon = props.icon;
</script>

<template>
  <div
    data-slot="empty-state"
    role="status"
    :class="
      cn(
        'flex min-h-40 items-center justify-center rounded-md border border-dashed p-6 text-muted-foreground',
        props.class,
      )
    "
  >
    <div class="flex min-w-0 flex-col items-center gap-3 text-center">
      <component v-if="Icon" :is="Icon" class="h-12 w-12 opacity-30" />
      <p class="text-sm font-medium">{{ title }}</p>
      <p v-if="description" class="max-w-xs text-center text-xs text-muted-foreground">
        {{ description }}
      </p>
      <div v-if="$slots.actions" class="mt-1 flex flex-wrap items-center justify-center gap-2">
        <slot name="actions" />
      </div>
    </div>
  </div>
</template>
