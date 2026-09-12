<script setup lang="ts">
import { type CatalogPreview } from "@bindings/CatalogPreview";

import { protocolLabel } from "~/lib/constants";
const props = defineProps<{
  preview: CatalogPreview | null;
  loading: boolean;
  error: string;
  errorDetail?: string;
  kind: "providers" | "models" | "providerModels";
  providerKey?: string;
  modelKey?: string;
  selectedKeys: string[];
  multiple?: boolean;
}>();
const emit = defineEmits<{ select: [keys: string[]]; retry: []; refresh: []; manual: [] }>();
const query = ref("");
const page = ref(1);
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
      hint: `${row.provider.providerId} · ${protocolLabel(row.provider.compat)} · ${row.provider.baseUrl} · ${counts[row.provider.providerId] ?? 0} 个相关连接`,
      exists: row.exists,
    }));
  }
  if (props.kind === "models")
    return catalog.models
      .filter((row) => !props.modelKey || row.model.modelName === props.modelKey)
      .map((row) => ({
        key: row.model.modelName,
        title: row.model.displayName || row.model.modelName,
        hint: `${row.model.modelName} · ${row.model.description ?? ""} · 最大输入 ${row.model.maxInputTokens} / 输出 ${row.model.maxOutputTokens}`,
        exists: row.exists,
      }));
  return catalog.links
    .filter(
      (row) =>
        (!props.providerKey || row.link.providerId === props.providerKey) &&
        (!props.modelKey || row.link.modelName === props.modelKey),
    )
    .map((row) => ({
      key: row.key,
      title: `${row.link.modelName} → ${row.link.providerModelId}`,
      hint: `${row.link.providerId} · ${row.link.protocolKey}`,
      exists: row.exists,
    }));
});
const filtered = computed(() => {
  const text = query.value.trim().toLowerCase();
  return rows.value.filter(
    (row) => !text || `${row.title} ${row.hint}`.toLowerCase().includes(text),
  );
});
const pageCount = computed(() => Math.max(1, Math.ceil(filtered.value.length / 50)));
const visible = computed(() => filtered.value.slice((page.value - 1) * 50, page.value * 50));
const selected = computed(() => new Set(props.selectedKeys));
watch([query, () => props.kind, () => props.providerKey, () => props.modelKey], () => {
  page.value = 1;
});
watch(pageCount, (count) => {
  page.value = Math.min(page.value, count);
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
  <section class="min-w-0 space-y-4">
    <div class="flex flex-wrap gap-2">
      <Input
        v-model="query"
        aria-label="搜索目录"
        placeholder="搜索 ID、名称或上游别名"
        class="min-w-0 flex-1"
      /><Button variant="outline" :disabled="loading" @click="emit('refresh')">刷新目录</Button
      ><Button variant="outline" @click="emit('manual')">手动配置</Button>
    </div>
    <p class="text-sm text-muted-foreground">
      配置的目录仅用于首次预填，保存后由你维护；不会同步或覆盖本地修改。
    </p>
    <p v-if="preview" class="text-xs break-all text-muted-foreground">
      revision: {{ preview.sourceRev }} · 生成于 {{ preview.generatedAt }}
    </p>
    <div v-if="loading" class="space-y-3" role="status" aria-label="加载目录">
      <Skeleton v-for="index in 4" :key="index" class="h-16 w-full" />
    </div>
    <div v-else-if="error" role="alert" class="space-y-3 rounded border border-destructive p-4">
      <p class="text-destructive">目录读取或校验失败</p>
      <p class="text-sm break-all whitespace-pre-wrap">{{ errorDetail || error }}</p>
      <Button variant="outline" @click="emit('retry')">重试</Button>
      <p>本地模型使用和手动配置不依赖目录。</p>
    </div>
    <template v-else-if="preview"
      ><div v-if="multiple" class="flex flex-wrap items-center gap-3">
        <Button variant="outline" @click="selectFiltered"
          >全选筛选结果（{{ filtered.filter((row) => !row.exists).length }}）</Button
        ><Button variant="ghost" @click="emit('select', [])">清空选择</Button
        ><span class="text-sm">已选 {{ selectedKeys.length }} 项</span>
      </div>
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
      <div class="flex items-center justify-between gap-2">
        <Button variant="outline" :disabled="page <= 1" @click="page--">上一页</Button
        ><span class="text-xs">{{ page }} / {{ pageCount }} · {{ filtered.length }} 项</span
        ><Button variant="outline" :disabled="page >= pageCount" @click="page++">下一页</Button>
      </div></template
    >
  </section>
</template>
