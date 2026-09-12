<script setup lang="ts">
import { cn } from "~/lib/utils";

defineProps<{ class?: string }>();
</script>

<template>
  <div data-slot="page-shell" :class="cn('flex min-w-0 flex-col gap-6', $props.class)">
    <slot />
  </div>
</template>
