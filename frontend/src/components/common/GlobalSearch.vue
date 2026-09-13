<script setup lang="ts">
import { type Component } from "vue";

import { type ModelResponse } from "@bindings/ModelResponse";
// 全局搜索（Ctrl+K）— 复用项目导航与模型数据，不引入外部搜索服务。
// 结果：导航页 + 已登录用户可见模型（listAvailableModels），键盘上下选择 + Enter 跳转。
import { Cpu, Search } from "@lucide/vue";

import { useApiCall } from "~/composables/useApiCall";
import { getApi } from "~/lib/api";
import { NAV_ITEMS } from "~/lib/navigation";
import { useAuthStore } from "~/stores/auth";

const open = defineModel<boolean>("open", { default: false });

const router = useRouter();
const authStore = useAuthStore();
const { isAdmin } = storeToRefs(authStore);

interface SearchItem {
  key: string;
  label: string;
  hint: string;
  icon: Component;
  to: string;
}

const models = ref<ModelResponse[]>([]);
const { execute: fetchModels } = useApiCall(() =>
  isAdmin.value ? getApi().models.listAllModels() : getApi().models.listAvailableModels(),
);
watchEffect(async () => {
  if (!open.value || !authStore.isAuthenticated) return;
  const list = await fetchModels();
  models.value = list ?? [];
});

const query = ref("");
const activeIndex = ref(0);
const searchInput = ref<HTMLInputElement | null>(null);
const results = ref<HTMLElement | null>(null);
watch(activeIndex, async () => {
  await nextTick();
  const selected = results.value?.querySelector<HTMLElement>('[data-active="true"]');
  if (!selected || !results.value) return;
  const bounds = results.value.getBoundingClientRect();
  const item = selected.getBoundingClientRect();
  if (item.bottom > bounds.bottom) results.value.scrollTop += item.bottom - bounds.bottom;
  else if (item.top < bounds.top) results.value.scrollTop += item.top - bounds.top;
});

const items = computed<SearchItem[]>(() => {
  const q = query.value.trim().toLowerCase();
  const nav = NAV_ITEMS.filter((n) => isAdmin.value || n.group === "use")
    .map((n) => ({ ...n, to: n.path, hint: n.group === "admin" ? "管理" : "导航" }))
    .filter((n) => !q || n.label.toLowerCase().includes(q) || n.hint.includes(q));
  const modelItems: SearchItem[] = models.value
    .filter(
      (m) => !q || m.modelName.toLowerCase().includes(q) || m.displayName.toLowerCase().includes(q),
    )
    .slice(0, 12)
    .map((m) => ({
      key: `model-${m.modelName}`,
      label: m.displayName || m.modelName,
      hint: m.modelName,
      icon: Cpu,
      to: `/models?model=${encodeURIComponent(m.modelName)}`,
    }));
  return [...nav, ...modelItems];
});

watch(open, (v) => {
  if (v) {
    query.value = "";
    activeIndex.value = 0;
    nextTick(() => searchInput.value?.focus());
  }
});

watch(
  () => items.value.length,
  (n) => {
    if (activeIndex.value >= n) activeIndex.value = Math.max(0, n - 1);
  },
);

function pick(item: SearchItem) {
  open.value = false;
  router.push(item.to);
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    activeIndex.value = Math.min(activeIndex.value + 1, items.value.length - 1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    activeIndex.value = Math.max(activeIndex.value - 1, 0);
  } else if (e.key === "Enter") {
    e.preventDefault();
    const item = items.value[activeIndex.value];
    if (item) pick(item);
  } else if (e.key === "Escape") {
    open.value = false;
  }
}
</script>

<template>
  <Dialog :open="open" @update:open="(v: boolean) => (open = v)">
    <DialogContent
      class="flex max-w-lg flex-col overflow-hidden p-0 sm:max-w-lg"
      :aria-describedby="undefined"
      @keydown="onKeydown"
    >
      <DialogHeader class="sr-only">
        <DialogTitle>全局搜索</DialogTitle>
      </DialogHeader>
      <div class="flex shrink-0 items-center gap-2 border-b border-border px-3 pr-12">
        <Search class="h-4 w-4 shrink-0 text-muted-foreground" />
        <input
          ref="searchInput"
          aria-label="搜索页面与模型"
          v-model="query"
          class="h-11 w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          placeholder="搜索页面与模型…"
        />
        <kbd
          class="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-xs text-muted-foreground"
        >
          Esc
        </kbd>
      </div>
      <div
        ref="results"
        data-scroll-area
        class="max-h-72 min-h-0 flex-1 overflow-y-auto overscroll-contain p-1.5"
      >
        <div v-if="items.length === 0" class="px-3 py-8 text-center text-sm text-muted-foreground">
          没有匹配的结果
        </div>
        <button
          v-for="(item, i) in items"
          :key="item.key"
          :data-active="i === activeIndex"
          class="flex w-full cursor-pointer items-center gap-2.5 rounded-md px-3 py-2 text-left text-sm transition-colors"
          :class="
            i === activeIndex
              ? 'bg-accent text-accent-foreground'
              : 'text-foreground hover:bg-accent/50'
          "
          @mouseenter="activeIndex = i"
          @click="pick(item)"
        >
          <component :is="item.icon" class="h-4 w-4 shrink-0 text-muted-foreground" />
          <span class="truncate">{{ item.label }}</span>
          <span
            v-if="item.hint !== '导航' && item.hint !== '管理'"
            class="ml-auto truncate font-mono text-xs text-muted-foreground"
          >
            {{ item.hint }}
          </span>
          <Badge v-else variant="secondary" class="ml-auto shrink-0 text-xs">{{ item.hint }}</Badge>
        </button>
      </div>
    </DialogContent>
  </Dialog>
</template>
