<script setup lang="ts">
import { RefreshCw, Search, X } from "@lucide/vue";

const query = defineModel<string>({ required: true });
withDefaults(
  defineProps<{
    label: string;
    placeholder: string;
    loading?: boolean;
    disabled?: boolean;
    filtered?: boolean;
    refreshable?: boolean;
    collapseFilters?: boolean;
  }>(),
  { refreshable: true },
);
defineEmits<{ refresh: []; clear: [] }>();
</script>

<template>
  <PageFilters :collapsed="collapseFilters">
    <template #primary>
      <div class="relative min-w-0 flex-1">
        <Search
          class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
          aria-hidden="true"
        />
        <Input
          v-model="query"
          :aria-label="label"
          :placeholder="placeholder"
          :disabled="disabled"
          class="pl-9"
        />
      </div>
      <slot name="primary" />
    </template>
    <template v-if="$slots.default" #default><slot /></template>
    <template #actions>
      <Button
        v-if="query || filtered"
        type="button"
        variant="ghost"
        size="icon"
        aria-label="清除筛选"
        title="清除筛选"
        :disabled="disabled"
        @click="$emit('clear')"
        ><X aria-hidden="true"
      /></Button>
      <Button
        v-if="refreshable"
        type="button"
        variant="outline"
        size="icon"
        aria-label="刷新列表"
        title="刷新列表"
        :disabled="loading || disabled"
        @click="$emit('refresh')"
        ><RefreshCw :class="loading ? 'animate-spin' : ''" aria-hidden="true"
      /></Button>
      <slot name="actions" />
    </template>
  </PageFilters>
</template>
