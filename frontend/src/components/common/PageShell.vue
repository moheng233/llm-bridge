<script setup lang="ts">
import { cn } from "~/lib/utils";

defineProps<{ class?: string }>();
</script>

<template>
  <div data-slot="page-shell" :class="cn('flex h-full min-h-0 flex-col gap-4', $props.class)">
    <slot />
  </div>
</template>
