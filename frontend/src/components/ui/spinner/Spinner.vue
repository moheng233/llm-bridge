<script setup lang="ts">
import { cn } from "~/lib/utils";
import { Loader2 } from "@lucide/vue";

withDefaults(
  defineProps<{
    class?: string;
    role?: string;
    "aria-label"?: string;
  }>(),
  {
    role: "status",
    "aria-label": "Loading",
  },
);
</script>

<template>
  <Loader2
    :role="role"
    :aria-label="$props['aria-label'] || 'Loading'"
    :class="cn('size-4 animate-spin', $props.class)"
  />
</template>
