<script setup lang="ts">
import { cn } from "~/lib/utils";
import { type Component } from "vue";

const props = withDefaults(
  defineProps<{
    class?: string;
    icon?: Component<{ class?: string }>;
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
    :class="cn('flex flex-1 items-center justify-center text-muted-foreground', props.class)"
  >
    <div class="flex flex-col items-center gap-3">
      <component v-if="Icon" :is="Icon" class="h-12 w-12 opacity-30" />
      <p class="text-sm font-medium">{{ title }}</p>
      <p v-if="description" class="text-xs text-muted-foreground max-w-xs text-center">
        {{ description }}
      </p>
      <div v-if="$slots.actions" class="flex items-center gap-2 mt-1">
        <slot name="actions" />
      </div>
    </div>
  </div>
</template>
