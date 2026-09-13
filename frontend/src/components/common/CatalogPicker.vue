<script setup lang="ts">
import { type CatalogPreview } from "@bindings/CatalogPreview";
import { useMediaQuery } from "@vueuse/core";

import { protocolLabel } from "~/lib/constants";
const props = defineProps<{
  preview: CatalogPreview | null;
  loading: boolean;
  error: string;
  errorDetail?: string;
  kind: "providers" | "models";
  modelKey?: string;
  selectedKeys: string[];
  multiple?: boolean;
  fill?: boolean;
}>();
const emit = defineEmits<{ select: [keys: string[]]; retry: []; refresh: []; manual: [] }>();
const query = defineModel<string>("query", { default: "" });
const page = defineModel<number>("page", { default: 1 });
const results = ref<HTMLElement>();
const short = useMediaQuery("(max-height: 640px)");
const rows = computed(() => {
  const catalog = props.preview;
  if (!catalog) return [];
  if (props.kind === "providers") {
    const counts: Record<string, number> = {};
    for (const row of catalog.links)
      counts[row.link.providerId] = (counts[row.link.providerId] ?? 0) + 1;
    return catalog.providers.map((row) => ({
      key: row.provider.providerId,
      title: row.provider.displayName || row.provider.providerId,
      searchText: `${row.provider.providerId} ${row.provider.displayName}`.toLowerCase(),
      hint: `${row.provider.providerId} · ${protocolLabel(row.provider.compat)} · ${row.provider.baseUrl} · ${counts[row.provider.providerId] ?? 0} 个相关连接`,
      exists: row.exists,
    }));
  }
  const aliases = new Map<string, Set<string>>();
  for (const { link } of catalog.links) {
    const names = aliases.get(link.modelName) ?? new Set<string>();
    names.add(link.providerModelId);
    aliases.set(link.modelName, names);
  }
  return catalog.models
    .filter((row) => !props.modelKey || row.model.modelName === props.modelKey)
    .map((row) => ({
      key: row.model.modelName,
      title: row.model.displayName || row.model.modelName,
      searchText:
        `${row.model.modelName} ${row.model.displayName} ${[...(aliases.get(row.model.modelName) ?? [])].join(" ")}`.toLowerCase(),
      hint: `${row.model.modelName} · ${row.model.description ?? ""} · 最大输入 ${row.model.maxInputTokens} / 输出 ${row.model.maxOutputTokens}`,
      exists: row.exists,
    }));
});
const filtered = computed(() => {
  const text = query.value.trim().toLowerCase();
  return rows.value.filter((row) => !text || row.searchText.includes(text));
});
const pageCount = computed(() => Math.max(1, Math.ceil(filtered.value.length / 50)));
const visible = computed(() => filtered.value.slice((page.value - 1) * 50, page.value * 50));
const selected = computed(() => new Set(props.selectedKeys));
watch([query, () => props.kind, () => props.modelKey], () => {
  page.value = 1;
});
watch(pageCount, (count) => {
  page.value = Math.min(page.value, count);
});
watch([page, query, () => props.kind, () => props.modelKey], async () => {
  await nextTick();
  if (results.value) results.value.scrollTop = 0;
});
function toggle(key: string) {
  if (!props.multiple) {
    emit("select", [key]);
    return;
  }
  const keys = new Set(props.selectedKeys);
  if (keys.has(key)) keys.delete(key);
  else keys.add(key);
  emit("select", [...keys]);
}
function selectFiltered() {
  const keys = new Set(props.selectedKeys);
  for (const row of filtered.value) if (!row.exists) keys.add(row.key);
  emit("select", [...keys]);
}
</script>
<template>
  <section
    :class="
      fill ? 'flex min-h-0 min-w-0 flex-1 flex-col gap-2 overflow-hidden' : 'min-w-0 space-y-4'
    "
  >
    <PageFilters v-if="short" class="shrink-0"
      ><template #primary>
        <Input
          v-model="query"
          aria-label="搜索目录"
          :placeholder="kind === 'providers' ? '搜索提供者' : '搜索模型'"
          class="min-w-0 flex-1"
        />
      </template>
      <Button variant="outline" :disabled="loading" @click="emit('refresh')">刷新目录</Button>
      <Button variant="outline" @click="emit('manual')">手动配置</Button>
      <template v-if="multiple && preview && !loading && !error">
        <Button variant="outline" @click="selectFiltered"
          >全选筛选结果（{{ filtered.filter((row) => !row.exists).length }}）</Button
        >
        <Button variant="ghost" @click="emit('select', [])">清空选择</Button>
        <span>已选 {{ selectedKeys.length }} 项</span>
      </template>
    </PageFilters>
    <div v-else class="flex shrink-0 flex-wrap gap-2">
      <Input
        v-model="query"
        aria-label="搜索目录"
        :placeholder="kind === 'providers' ? '搜索提供者 ID 或名称' : '搜索模型 ID、名称或上游别名'"
        class="min-w-0 flex-1"
      /><Button variant="outline" :disabled="loading" @click="emit('refresh')">刷新目录</Button
      ><Button variant="outline" @click="emit('manual')">手动配置</Button>
    </div>
    <div
      v-if="!short && multiple && preview && !loading && !error"
      class="flex shrink-0 flex-wrap items-center gap-2"
    >
      <Button variant="outline" @click="selectFiltered"
        >全选筛选结果（{{ filtered.filter((row) => !row.exists).length }}）</Button
      >
      <Button variant="ghost" @click="emit('select', [])">清空选择</Button>
      <span class="text-sm">已选 {{ selectedKeys.length }} 项</span>
    </div>
    <div
      ref="results"
      data-scroll-area
      role="region"
      :aria-label="kind === 'providers' ? '目录提供者结果' : '目录模型结果'"
      tabindex="0"
      :class="
        fill
          ? 'min-h-0 flex-1 space-y-3 overflow-y-auto overscroll-contain p-1'
          : 'max-h-[min(48svh,28rem)] space-y-3 overflow-y-auto overscroll-contain p-1'
      "
    >
      <details class="text-xs text-muted-foreground">
        <summary class="cursor-pointer py-1">目录来源</summary>
        <p class="text-sm text-muted-foreground">
          配置的目录仅用于首次预填，保存后由你维护；不会同步或覆盖本地修改。
        </p>
        <p v-if="preview" class="text-xs break-all text-muted-foreground">
          revision: {{ preview.sourceRev }} · 生成于 {{ preview.generatedAt }}
        </p>
      </details>
      <div v-if="loading" class="space-y-3" role="status" aria-label="加载目录">
        <Skeleton v-for="index in 4" :key="index" class="h-16 w-full" />
      </div>
      <div v-else-if="error" role="alert" class="space-y-3 rounded border border-destructive p-4">
        <p class="text-destructive">目录读取或校验失败</p>
        <p class="text-sm break-all whitespace-pre-wrap">{{ errorDetail || error }}</p>
        <Button variant="outline" @click="emit('retry')">重试</Button>
        <p>本地模型使用和手动配置不依赖目录。</p>
      </div>
      <template v-else-if="preview">
        <div v-if="!filtered.length" class="space-y-3 rounded border p-6">
          <p>{{ query ? "筛选无结果" : "没有对应的目录条目" }}</p>
          <Button v-if="query" variant="outline" @click="query = ''">清除筛选</Button
          ><Button v-else variant="outline" @click="emit('manual')">手动配置</Button>
        </div>
        <div v-else class="space-y-2">
          <button
            v-for="row in visible"
            :key="row.key"
            type="button"
            class="flex min-h-14 w-full items-start gap-3 rounded border bg-card p-3 text-left hover:bg-accent"
            :class="selected.has(row.key) ? 'border-primary ring-1 ring-primary' : ''"
            :aria-pressed="selected.has(row.key)"
            @click="toggle(row.key)"
          >
            <span class="shrink-0 text-sm">{{ selected.has(row.key) ? "✓ 已选" : "选择" }}</span
            ><span class="min-w-0 flex-1"
              ><span class="block font-medium break-all">{{ row.title }}</span
              ><span class="block text-xs leading-[18px] break-all text-muted-foreground">{{
                row.hint
              }}</span></span
            ><span v-if="row.exists" class="shrink-0 text-xs">已添加</span>
          </button>
        </div>
      </template>
    </div>
    <div
      v-if="preview && !loading && !error"
      class="flex shrink-0 items-center justify-between gap-2"
    >
      <Button variant="outline" :disabled="page <= 1" @click="page--">上一页</Button
      ><span class="text-xs">{{ page }} / {{ pageCount }} · {{ filtered.length }} 项</span
      ><Button variant="outline" :disabled="page >= pageCount" @click="page++">下一页</Button>
    </div>
  </section>
</template>
