<script setup lang="ts">
import { AlertCircle, RefreshCw } from "@lucide/vue";

import { cn } from "~/lib/utils";
import { formatApiError } from "~/lib/utils/error";

const props = withDefaults(
  defineProps<{
    class?: string;
    /** 错误对象（unknown，来自 catch）或字符串 */
    error: unknown;
    /** 重试回调；不传则不显示重试按钮 */
    onRetry?: () => void;
    /** 是否紧凑展示（用于内联） */
    inline?: boolean;
  }>(),
  {
    inline: false,
  },
);

const emit = defineEmits<{ retry: [] }>();

const formatted = computed(() => formatApiError(props.error));
</script>

<template>
  <div
    data-slot="error-state"
    :class="
      cn(
        inline
          ? 'flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-3'
          : 'flex flex-1 items-center justify-center',
        props.class,
      )
    "
  >
    <template v-if="inline">
      <AlertCircle class="h-4 w-4 shrink-0 text-destructive" />
      <div class="flex min-w-0 flex-col gap-0.5">
        <span class="text-sm font-medium text-destructive">{{ formatted.title }}</span>
        <span
          v-if="formatted.detail && formatted.detail !== formatted.title"
          class="truncate text-xs text-muted-foreground"
          :title="formatted.detail"
        >
          {{ formatted.detail }}
        </span>
      </div>
      <Button
        v-if="onRetry"
        variant="outline"
        size="sm"
        class="ml-auto h-7 shrink-0 cursor-pointer gap-1 text-xs"
        @click="onRetry"
      >
        <RefreshCw class="h-3 w-3" />
        重试
      </Button>
    </template>
    <template v-else>
      <div class="flex flex-col items-center gap-3">
        <AlertCircle class="h-12 w-12 text-destructive opacity-50" />
        <p class="text-sm font-medium">{{ formatted.title }}</p>
        <p
          v-if="formatted.detail && formatted.detail !== formatted.title"
          class="max-w-xs text-center text-xs text-muted-foreground"
        >
          {{ formatted.detail }}
        </p>
        <Button v-if="onRetry" variant="outline" size="sm" class="cursor-pointer" @click="onRetry">
          <RefreshCw class="h-4 w-4" />
          重试
        </Button>
      </div>
    </template>
  </div>
</template>
