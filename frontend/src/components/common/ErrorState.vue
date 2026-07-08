<script setup lang="ts">
import { cn } from "~/lib/utils";
import { AlertCircle, RefreshCw } from "@lucide/vue";
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
      <AlertCircle class="h-4 w-4 text-destructive shrink-0" />
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-sm font-medium text-destructive">{{ formatted.title }}</span>
        <span
          v-if="formatted.detail && formatted.detail !== formatted.title"
          class="text-xs text-muted-foreground truncate"
          :title="formatted.detail"
        >
          {{ formatted.detail }}
        </span>
      </div>
      <Button
        v-if="onRetry"
        variant="outline"
        size="sm"
        class="ml-auto shrink-0 cursor-pointer text-xs h-7 gap-1"
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
          class="text-xs text-muted-foreground max-w-xs text-center"
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
