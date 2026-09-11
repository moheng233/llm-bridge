<script setup lang="ts">
import { type CatalogSelection } from "@bindings/CatalogSelection";

import { useApiCall } from "~/composables/useApiCall";
import { useReactiveSet } from "~/composables/useReactiveCollections";
import { getApi } from "~/lib/api";

const open = defineModel<boolean>("open", { default: false });
const emit = defineEmits<{ imported: [] }>();
const api = getApi();
const {
  data: preview,
  loading,
  error,
  execute: fetchPreview,
} = useApiCall(() => api.modelsImport.preview());
const {
  data: report,
  loading: importing,
  error: importError,
  execute: submit,
  clearError,
} = useApiCall((selection: CatalogSelection) => api.modelsImport.import(selection));
type Layer = "models" | "providers" | "links";
const layers: { key: Layer; label: string }[] = [
  { key: "models", label: "模型" },
  { key: "providers", label: "提供者" },
  { key: "links", label: "关联" },
];
const layer = ref<Layer>("models");
const search = ref("");
const filter = ref("all");
const page = ref(0);
const pageSize = 50;
const selected = {
  models: useReactiveSet<string>(),
  providers: useReactiveSet<string>(),
  links: useReactiveSet<string>(),
};
const selection = computed<CatalogSelection>(() => ({
  models: [...selected.models.raw.value],
  providers: [...selected.providers.raw.value],
  links: [...selected.links.raw.value],
}));
const selectedCount = computed(
  () =>
    selection.value.models.length + selection.value.providers.length + selection.value.links.length,
);
const rows = computed(() => {
  const catalog = preview.value;
  if (!catalog) return [];
  const all =
    layer.value === "models"
      ? catalog.models.map(({ model, exists }) => ({
          key: model.modelName,
          label: model.displayName || model.modelName,
          detail: model.modelName,
          exists,
        }))
      : layer.value === "providers"
        ? catalog.providers.map(({ provider, exists }) => ({
            key: provider.providerId,
            label: provider.displayName,
            detail: provider.baseUrl,
            exists,
          }))
        : catalog.links.map(({ key, link, exists }) => ({
            key,
            label: `${link.providerId} / ${link.providerModelId}`,
            detail: link.modelName,
            exists,
          }));
  const query = search.value.trim().toLowerCase();
  return all.filter(
    (row) =>
      (filter.value === "all" || row.exists === (filter.value === "update")) &&
      `${row.label} ${row.detail}`.toLowerCase().includes(query),
  );
});
const pages = computed(() => Math.max(1, Math.ceil(rows.value.length / pageSize)));
const visibleRows = computed(() =>
  rows.value.slice(page.value * pageSize, (page.value + 1) * pageSize),
);
watch([layer, search, filter], () => {
  page.value = 0;
});
function clearSelection() {
  for (const group of Object.values(selected)) group.clear();
}
async function load() {
  clearSelection();
  preview.value = null;
  report.value = null;
  clearError();
  page.value = 0;
  await fetchPreview();
}
watch(open, (value) => {
  if (value) void load();
});
function selectRows(keys: string[], checked: boolean) {
  if (!preview.value) return;
  const next = {
    models: new Set(selection.value.models),
    providers: new Set(selection.value.providers),
    links: new Set(selection.value.links),
  };
  const target = new Set(keys);
  for (const key of keys) {
    if (checked) next[layer.value].add(key);
    else next[layer.value].delete(key);
  }
  for (const { key, link } of preview.value.links) {
    const matches =
      layer.value === "links"
        ? target.has(key)
        : layer.value === "models"
          ? target.has(link.modelName)
          : target.has(link.providerId);
    if (!matches) continue;
    if (checked) {
      next.links.add(key);
      next.models.add(link.modelName);
      next.providers.add(link.providerId);
    } else next.links.delete(key);
  }
  selected.models.raw.value = next.models;
  selected.providers.raw.value = next.providers;
  selected.links.raw.value = next.links;
}
async function runImport() {
  if (importing.value || !selectedCount.value) return;
  const result = await submit(selection.value);
  if (result) {
    clearSelection();
    emit("imported");
    await fetchPreview();
  }
}
function setOpen(value: boolean) {
  if (!importing.value) open.value = value;
}
</script>

<template>
  <Dialog :open="open" @update:open="setOpen">
    <DialogContent class="flex max-h-[90dvh] flex-col sm:max-w-3xl">
      <DialogHeader class="shrink-0">
        <DialogTitle>从 models.dev 目录导入</DialogTitle>
        <DialogDescription
          >先预览，再选择导入。选择模型或提供者会带上其关联及依赖；清除父项会取消对应关联。已有 API
          Key 和手动优先级不会被覆盖。</DialogDescription
        >
      </DialogHeader>
      <div class="min-h-0 flex-1 space-y-4 overflow-y-auto">
        <p v-if="loading" role="status" class="py-8 text-center text-muted-foreground">
          正在拉取并比较目录…
        </p>
        <ErrorState v-if="error" :error="error" inline @retry="load" />
        <template v-if="preview">
          <p class="truncate text-xs text-muted-foreground">
            来源 {{ preview.sourceRev }} · {{ preview.generatedAt }}
          </p>
          <div class="flex flex-wrap gap-2" aria-label="目录层级">
            <Button
              v-for="item in layers"
              :key="item.key"
              :variant="layer === item.key ? 'default' : 'outline'"
              :disabled="importing"
              @click="layer = item.key"
              >{{ item.label }} ({{ preview[item.key].length }})</Button
            >
          </div>
          <div class="flex flex-wrap gap-2">
            <Input
              v-model="search"
              aria-label="搜索目录"
              placeholder="搜索模型、提供者或关联"
              class="min-w-40 flex-1"
              :disabled="importing"
            />
            <select
              v-model="filter"
              aria-label="导入差异筛选"
              class="rounded-md border border-input bg-background px-3 text-sm"
              :disabled="importing"
            >
              <option value="all">全部</option>
              <option value="create">仅新建</option>
              <option value="update">仅更新</option>
            </select>
            <Button
              variant="outline"
              :disabled="importing"
              @click="
                selectRows(
                  rows.map((row) => row.key),
                  true,
                )
              "
              >全选筛选结果</Button
            >
            <Button variant="ghost" :disabled="importing" @click="clearSelection">清空选择</Button>
          </div>
          <div
            class="min-h-24 overflow-y-auto rounded-md border border-border"
            :aria-busy="importing"
          >
            <label
              v-for="row in visibleRows"
              :key="row.key"
              class="flex cursor-pointer items-center gap-3 border-b border-border p-3 last:border-0 hover:bg-muted/40"
            >
              <input
                type="checkbox"
                class="size-4 shrink-0 accent-current"
                :aria-label="`选择 ${row.label}`"
                :checked="selected[layer].has(row.key)"
                :disabled="importing"
                @change="selectRows([row.key], ($event.target as HTMLInputElement).checked)"
              />
              <span class="min-w-0 flex-1"
                ><span class="block truncate text-sm">{{ row.label }}</span
                ><span class="block truncate font-mono text-xs text-muted-foreground">{{
                  row.detail
                }}</span></span
              >
              <Badge :variant="row.exists ? 'secondary' : 'outline'">{{
                row.exists ? "更新" : "新建"
              }}</Badge>
            </label>
            <p v-if="!rows.length" class="p-6 text-center text-sm text-muted-foreground">
              没有匹配项
            </p>
          </div>
          <div class="flex items-center justify-between gap-2 text-xs text-muted-foreground">
            <span>筛选 {{ rows.length }} 项 · 第 {{ page + 1 }} / {{ pages }} 页</span>
            <div class="flex gap-2">
              <Button
                size="sm"
                variant="outline"
                :disabled="page === 0 || importing"
                @click="page--"
                >上一页</Button
              ><Button
                size="sm"
                variant="outline"
                :disabled="page + 1 >= pages || importing"
                @click="page++"
                >下一页</Button
              >
            </div>
          </div>
        </template>
        <p v-if="importError" role="alert" class="text-sm text-destructive">{{ importError }}</p>
        <p v-if="report" role="status" class="text-sm">
          事务已提交：新建 {{ report.created }}、更新/复用 {{ report.updated }}、跳过
          {{ report.skipped }}。计数包含协议行。
        </p>
      </div>
      <DialogFooter class="shrink-0 gap-2">
        <span class="mr-auto text-xs text-muted-foreground"
          >已选 {{ selection.models.length }} 模型 / {{ selection.providers.length }} 提供者 /
          {{ selection.links.length }} 关联</span
        >
        <Button variant="outline" :disabled="importing" @click="setOpen(false)">关闭</Button>
        <Button :disabled="loading || importing || !selectedCount" @click="runImport">{{
          importing ? "事务导入中…" : "导入所选"
        }}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
