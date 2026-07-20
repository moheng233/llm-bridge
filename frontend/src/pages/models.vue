<script setup lang="ts">
import { type ModelProviderSummary } from "@bindings/ModelProviderSummary";
import { type ModelResponse } from "@bindings/ModelResponse";
import {
  Search,
  Cpu,
  Eye,
  Wrench,
  Brain,
  ArrowUpDown,
  ArrowUp,
  ArrowDown,
  Sparkles,
} from "@lucide/vue";

import { getApi, formatTokens, formatPrice } from "~/lib/api";
import { SKELETON_ROWS } from "~/lib/constants";
import { useApiCall } from "~/composables/useApiCall";

const api = getApi();

// ── 价格聚合 ──

function cheapestPrice(
  providers: ModelProviderSummary[],
  key: keyof ModelProviderSummary,
): number | null {
  const prices = providers
    .map((p) => p[key])
    .filter((v): v is number => typeof v === "number");
  return prices.length > 0 ? Math.min(...prices) : null;
}

function priceRange(
  providers: ModelProviderSummary[],
  key: keyof ModelProviderSummary,
): { min: number; max: number } | null {
  const prices = providers
    .map((p) => p[key])
    .filter((v): v is number => typeof v === "number");
  if (prices.length === 0) return null;
  return { min: Math.min(...prices), max: Math.max(...prices) };
}

function formatPriceRange(range: { min: number; max: number } | null): string {
  if (!range) return "—";
  if (range.min === range.max) return formatPrice(range.min);
  return `${formatPrice(range.min)} – ${formatPrice(range.max)}`;
}

function availableProviderCount(providers: ModelProviderSummary[]): number {
  return providers.filter((p) => p.enabled).length;
}

// ── 数据加载 ──

const models = ref<ModelResponse[]>([]);
const search = ref("");
const onlyAvailable = ref(false);
const sortField = ref<"name" | "maxInputTokens" | "maxOutputTokens" | "inputPrice">("name");
const sortDir = ref<"asc" | "desc">("asc");

const { loading, error, execute: fetchModels } = useApiCall(() =>
  onlyAvailable.value ? api.models.listAvailableModels() : api.models.listAllModels(),
);

async function load() {
  const result = await fetchModels();
  if (result) models.value = result;
}

watchEffect(() => {
  onlyAvailable.value;
  load();
});

// ── 能力筛选 ──

const capabilityFilter = ref<{ vision: boolean; tool: boolean; thinking: boolean }>({
  vision: false,
  tool: false,
  thinking: false,
});

function toggleCapability(key: "vision" | "tool" | "thinking") {
  capabilityFilter.value[key] = !capabilityFilter.value[key];
}

const capabilityChips = computed(() => [
  { key: "tool" as const, label: "工具调用", icon: Wrench, active: capabilityFilter.value.tool },
  { key: "vision" as const, label: "视觉", icon: Eye, active: capabilityFilter.value.vision },
  { key: "thinking" as const, label: "推理", icon: Brain, active: capabilityFilter.value.thinking },
]);

// ── 列表过滤/排序 ──

const filteredModels = computed(() => {
  let result = models.value;
  if (search.value.trim()) {
    const q = search.value.toLowerCase();
    result = result.filter(
      (m) => m.modelName.toLowerCase().includes(q) || m.description?.toLowerCase().includes(q),
    );
  }
  if (capabilityFilter.value.vision) result = result.filter((m) => m.vision);
  if (capabilityFilter.value.tool) result = result.filter((m) => m.toolCalling);
  if (capabilityFilter.value.thinking)
    result = result.filter((m) => m.thinking === true || m.adaptiveThinking === true);

  result = [...result].sort((a, b) => {
    const dir = sortDir.value === "asc" ? 1 : -1;
    if (sortField.value === "name") return dir * a.modelName.localeCompare(b.modelName);
    if (sortField.value === "maxInputTokens") return dir * (a.maxInputTokens - b.maxInputTokens);
    if (sortField.value === "maxOutputTokens") return dir * (a.maxOutputTokens - b.maxOutputTokens);
    if (sortField.value === "inputPrice") {
      const pa = cheapestPrice(a.providers, "inputPricePer1m") ?? Number.POSITIVE_INFINITY;
      const pb = cheapestPrice(b.providers, "inputPricePer1m") ?? Number.POSITIVE_INFINITY;
      return dir * (pa - pb);
    }
    return 0;
  });
  return result;
});

function toggleSort(field: typeof sortField.value) {
  if (sortField.value === field) {
    sortDir.value = sortDir.value === "asc" ? "desc" : "asc";
  } else {
    sortField.value = field;
    sortDir.value = "asc";
  }
}

function sortIcon(field: typeof sortField.value) {
  if (sortField.value !== field) return ArrowUpDown;
  return sortDir.value === "asc" ? ArrowUp : ArrowDown;
}

// ── 表头定义 ──

interface ColumnDef {
  key: typeof sortField.value | null;
  label: string;
  align: "left" | "right" | "center";
  sortable: boolean;
}

const columns: ColumnDef[] = [
  { key: "name", label: "模型", align: "left", sortable: true },
  { key: "maxInputTokens", label: "上下文", align: "right", sortable: true },
  { key: "maxOutputTokens", label: "输出上限", align: "right", sortable: true },
  { key: null, label: "能力", align: "center", sortable: false },
  { key: "inputPrice", label: "输入价格", align: "right", sortable: true },
  { key: null, label: "输出价格", align: "right", sortable: false },
];

// ── 详情抽屉 ──

const selectedModel = ref<ModelResponse | null>(null);
const sheetOpen = ref(false);

function openDetail(model: ModelResponse) {
  selectedModel.value = model;
  sheetOpen.value = true;
}

function capabilityList(m: ModelResponse) {
  return [
    { label: "工具调用", icon: Wrench, active: m.toolCalling },
    { label: "视觉", icon: Eye, active: m.vision },
    {
      label: "推理",
      icon: Brain,
      active: m.thinking === true || m.adaptiveThinking === true,
    },
  ];
}

// Provider 详情排序：可用优先，其次按优先级数字（小优先），再按名称
function sortedProviders(providers: ModelProviderSummary[]): ModelProviderSummary[] {
  return [...providers].sort((a, b) => {
    if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;
    if (a.priority !== b.priority) return a.priority - b.priority;
    return a.providerDisplayName.localeCompare(b.providerDisplayName);
  });
}
</script>

<template>
  <div class="flex h-full min-h-0 flex-col gap-4">
    <!-- 页眉 -->
    <div class="flex items-center justify-between">
      <div>
        <h2 class="font-mono text-xl font-bold text-foreground">模型目录</h2>
        <p class="mt-1 text-sm text-muted-foreground">
          浏览可用模型的能力、上下文与定价 · 支持多 Provider 路由
        </p>
      </div>
      <Badge variant="secondary" class="font-mono">{{ models.length }} 个模型</Badge>
    </div>

    <Alert v-if="error" class="border-destructive/30 bg-destructive/10">
      <AlertDescription class="text-sm text-destructive">{{ error }}</AlertDescription>
    </Alert>

    <!-- 过滤栏 -->
    <div class="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
      <div class="relative max-w-md flex-1">
        <Search class="absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <Input v-model="search" class="pl-9" placeholder="搜索模型名称或描述..." />
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <button
          v-for="chip in capabilityChips"
          :key="chip.key"
          type="button"
          class="flex cursor-pointer items-center gap-1.5 rounded-full border px-3 py-1.5 text-xs font-medium transition-colors"
          :class="
            chip.active
              ? 'border-cta/40 bg-cta/10 text-cta'
              : 'border-border bg-transparent text-muted-foreground hover:bg-accent hover:text-foreground'
          "
          @click="toggleCapability(chip.key)"
        >
          <component :is="chip.icon" class="h-3.5 w-3.5" />
          {{ chip.label }}
        </button>
        <Label
          class="flex cursor-pointer items-center gap-2 whitespace-nowrap text-sm text-muted-foreground"
        >
          <Checkbox v-model="onlyAvailable" class="cursor-pointer" /> 仅可用
        </Label>
      </div>
    </div>

    <!-- 加载骨架 -->
    <div v-if="loading" class="flex flex-col gap-2">
      <Skeleton v-for="i in SKELETON_ROWS.models" :key="i" class="h-12 w-full rounded-lg" />
    </div>

    <!-- 模型表格（桌面端 md+） -->
    <div
      v-else
      class="hidden min-h-0 flex-1 overflow-auto rounded-lg border border-border md:block"
    >
      <table class="w-full text-sm">
        <thead class="sticky top-0 z-10 border-b border-border bg-card">
          <tr>
            <th
              v-for="col in columns"
              :key="col.label"
              class="px-4 py-3 font-mono text-xs text-muted-foreground"
              :class="[
                col.align === 'left' && 'text-left',
                col.align === 'right' && 'text-right',
                col.align === 'center' && 'text-center',
              ]"
            >
              <button
                v-if="col.sortable"
                type="button"
                class="inline-flex cursor-pointer items-center gap-1 hover:text-foreground"
                @click="toggleSort(col.key as any)"
              >
                {{ col.label }}
                <component :is="sortIcon(col.key as any)" class="h-3 w-3 opacity-60" />
              </button>
              <span v-else>{{ col.label }}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="model in filteredModels"
            :key="model.modelName"
            class="cursor-pointer border-b border-border/50 transition-colors hover:bg-accent/50"
            @click="openDetail(model)"
          >
            <td class="px-4 py-3">
              <div class="flex flex-col gap-0.5">
                <span class="font-mono font-medium text-foreground">{{ model.modelName }}</span>
                <span v-if="model.description" class="line-clamp-1 text-xs text-muted-foreground">{{
                  model.description
                }}</span>
                <div class="mt-1 flex flex-wrap items-center gap-1">
                  <Badge variant="secondary" class="px-1.5 py-0 font-mono text-[10px]">
                    {{ availableProviderCount(model.providers) }}/{{ model.providers.length }} 渠道
                  </Badge>
                  <Badge
                    v-for="p in model.providers.slice(0, 2)"
                    :key="p.providerModelId"
                    variant="outline"
                    class="px-1.5 py-0 font-mono text-[10px]"
                    :class="!p.enabled && 'opacity-40 line-through'"
                    :title="p.providerModelId"
                    >{{ p.providerDisplayName }}</Badge
                  >
                  <span
                    v-if="model.providers.length > 2"
                    class="text-[10px] text-muted-foreground"
                    >+{{ model.providers.length - 2 }}</span
                  >
                </div>
              </div>
            </td>
            <td class="px-4 py-3 text-right font-mono text-foreground tabular-nums">
              {{ formatTokens(model.maxInputTokens) }}
            </td>
            <td class="px-4 py-3 text-right font-mono text-foreground tabular-nums">
              {{ formatTokens(model.maxOutputTokens) }}
            </td>
            <td class="px-4 py-3">
              <div class="flex items-center justify-center gap-1.5">
                <Wrench
                  v-if="model.toolCalling"
                  class="h-3.5 w-3.5 text-cta"
                  title="工具调用"
                />
                <Eye v-if="model.vision" class="h-3.5 w-3.5 text-cta" title="视觉" />
                <Brain
                  v-if="model.thinking || model.adaptiveThinking"
                  class="h-3.5 w-3.5 text-cta"
                  title="推理"
                />
                <span
                  v-if="!model.toolCalling && !model.vision && !model.thinking && !model.adaptiveThinking"
                  class="text-[10px] text-muted-foreground"
                  >—</span
                >
              </div>
            </td>
            <td class="px-4 py-3 text-right font-mono text-foreground tabular-nums">
              <div class="flex flex-col items-end leading-tight">
                <span>{{ formatPrice(cheapestPrice(model.providers, "inputPricePer1m")) }}</span>
                <span class="text-[10px] text-muted-foreground">起 / 1M</span>
              </div>
            </td>
            <td class="px-4 py-3 text-right font-mono text-foreground tabular-nums">
              {{ formatPrice(cheapestPrice(model.providers, "outputPricePer1m")) }}
            </td>
          </tr>
        </tbody>
      </table>
      <div
        v-if="filteredModels.length === 0"
        class="flex items-center justify-center py-12 text-muted-foreground"
      >
        <div class="flex flex-col items-center gap-2">
          <Cpu class="h-8 w-8 opacity-30" />
          <p class="text-sm">未找到匹配的模型</p>
        </div>
      </div>
    </div>

    <!-- 模型卡片（移动端 <md） -->
    <div v-if="!loading" class="flex flex-col gap-2 md:hidden">
      <button
        v-for="model in filteredModels"
        :key="model.modelName"
        type="button"
        class="flex cursor-pointer flex-col gap-2 rounded-lg border border-border bg-card p-3 text-left transition-colors hover:bg-accent/50"
        @click="openDetail(model)"
      >
        <div class="flex items-start justify-between gap-2">
          <div class="min-w-0 flex-1">
            <div class="font-mono font-medium text-foreground">{{ model.modelName }}</div>
            <div v-if="model.description" class="mt-0.5 line-clamp-1 text-xs text-muted-foreground">
              {{ model.description }}
            </div>
          </div>
          <Badge variant="secondary" class="shrink-0 font-mono text-[10px]">
            {{ availableProviderCount(model.providers) }}/{{ model.providers.length }}
          </Badge>
        </div>
        <div class="flex flex-wrap items-center gap-1.5 text-[11px]">
          <span class="rounded bg-accent px-1.5 py-0.5 font-mono">{{
            formatTokens(model.maxInputTokens)
          }}</span>
          <span
            v-if="model.toolCalling"
            class="inline-flex items-center gap-0.5 rounded bg-cta/10 px-1.5 py-0.5 text-cta"
          >
            <Wrench class="h-3 w-3" /> 工具
          </span>
          <span
            v-if="model.vision"
            class="inline-flex items-center gap-0.5 rounded bg-cta/10 px-1.5 py-0.5 text-cta"
          >
            <Eye class="h-3 w-3" /> 视觉
          </span>
          <span
            v-if="model.thinking || model.adaptiveThinking"
            class="inline-flex items-center gap-0.5 rounded bg-cta/10 px-1.5 py-0.5 text-cta"
          >
            <Brain class="h-3 w-3" /> 推理
          </span>
        </div>
        <div class="flex items-center justify-between text-[11px] text-muted-foreground">
          <span
            >输入 <span class="font-mono text-foreground">{{
              formatPrice(cheapestPrice(model.providers, "inputPricePer1m"))
            }}</span></span
          >
          <span
            >输出 <span class="font-mono text-foreground">{{
              formatPrice(cheapestPrice(model.providers, "outputPricePer1m"))
            }}</span></span
          >
        </div>
      </button>
      <div
        v-if="filteredModels.length === 0"
        class="flex items-center justify-center py-12 text-muted-foreground"
      >
        <div class="flex flex-col items-center gap-2">
          <Cpu class="h-8 w-8 opacity-30" />
          <p class="text-sm">未找到匹配的模型</p>
        </div>
      </div>
    </div>

    <!-- 模型详情抽屉 -->
    <Sheet v-model:open="sheetOpen">
      <SheetContent side="right" class="flex w-full flex-col gap-0 p-0 sm:max-w-lg">
        <SheetHeader class="flex flex-col gap-2 border-b border-border p-4 pr-10">
          <div class="flex items-center gap-2">
            <Sparkles class="h-4 w-4 text-cta" />
            <SheetTitle class="font-mono text-lg font-bold">{{
              selectedModel?.modelName
            }}</SheetTitle>
          </div>
          <SheetDescription
            v-if="selectedModel?.description"
            class="text-sm text-muted-foreground"
          >
            {{ selectedModel.description }}
          </SheetDescription>
          <SheetDescription v-else class="text-xs text-muted-foreground">暂无描述</SheetDescription>
          <div v-if="selectedModel" class="flex flex-wrap gap-1.5 pt-1">
            <Badge
              v-for="cap in capabilityList(selectedModel)"
              :key="cap.label"
              :variant="cap.active ? 'default' : 'outline'"
              class="gap-1 text-[10px]"
              :class="cap.active && 'bg-cta/15 text-cta hover:bg-cta/20'"
            >
              <component :is="cap.icon" class="h-3 w-3" />
              {{ cap.label }}
            </Badge>
          </div>
        </SheetHeader>

        <div v-if="selectedModel" class="min-h-0 flex-1 overflow-auto p-4">
          <!-- 概览 -->
          <div class="grid grid-cols-3 gap-2 text-center">
            <div class="rounded-lg border border-border bg-card p-2">
              <div class="text-[10px] text-muted-foreground">上下文</div>
              <div class="font-mono text-sm font-medium text-foreground">
                {{ formatTokens(selectedModel.maxInputTokens) }}
              </div>
            </div>
            <div class="rounded-lg border border-border bg-card p-2">
              <div class="text-[10px] text-muted-foreground">输出上限</div>
              <div class="font-mono text-sm font-medium text-foreground">
                {{ formatTokens(selectedModel.maxOutputTokens) }}
              </div>
            </div>
            <div class="rounded-lg border border-border bg-card p-2">
              <div class="text-[10px] text-muted-foreground">可用渠道</div>
              <div class="font-mono text-sm font-medium text-foreground">
                {{ availableProviderCount(selectedModel.providers) }}/{{
                  selectedModel.providers.length
                }}
              </div>
            </div>
          </div>

          <!-- 价格区间 -->
          <div class="mt-4 rounded-lg border border-border bg-card p-3">
            <div class="mb-2 font-mono text-xs text-muted-foreground">价格区间（每 1M tokens）</div>
            <div class="grid grid-cols-3 gap-2 text-sm">
              <div>
                <div class="text-[10px] text-muted-foreground">输入</div>
                <div class="font-mono text-foreground tabular-nums">
                  {{ formatPriceRange(priceRange(selectedModel.providers, "inputPricePer1m")) }}
                </div>
              </div>
              <div>
                <div class="text-[10px] text-muted-foreground">输出</div>
                <div class="font-mono text-foreground tabular-nums">
                  {{ formatPriceRange(priceRange(selectedModel.providers, "outputPricePer1m")) }}
                </div>
              </div>
              <div>
                <div class="text-[10px] text-muted-foreground">缓存读</div>
                <div class="font-mono text-foreground tabular-nums">
                  {{
                    formatPriceRange(priceRange(selectedModel.providers, "cacheReadPricePer1m"))
                  }}
                </div>
              </div>
            </div>
          </div>

          <!-- Provider 列表 -->
          <div class="mt-4">
            <div class="mb-2 font-mono text-xs text-muted-foreground">
              Provider 渠道（{{ selectedModel.providers.length }}）
            </div>
            <div class="flex flex-col gap-2">
              <div
                v-for="p in sortedProviders(selectedModel.providers)"
                :key="p.providerModelId"
                class="rounded-lg border border-border bg-card p-3"
                :class="!p.enabled && 'opacity-60'"
              >
                <div class="flex items-center justify-between gap-2">
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-1.5">
                      <span class="truncate font-mono text-sm font-medium text-foreground">{{
                        p.providerDisplayName
                      }}</span>
                      <span
                        v-if="p.enabled"
                        class="inline-flex items-center gap-0.5 rounded bg-cta/15 px-1.5 py-0 text-[9px] font-medium text-cta"
                        >可用</span
                      >
                      <span
                        v-else
                        class="inline-flex items-center gap-0.5 rounded bg-muted px-1.5 py-0 text-[9px] font-medium text-muted-foreground"
                        >停用</span
                      >
                    </div>
                    <div class="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
                      {{ p.providerModelId }}
                    </div>
                  </div>
                  <Badge variant="outline" class="font-mono text-[10px]">P{{ p.priority }}</Badge>
                </div>
                <div class="mt-2 grid grid-cols-3 gap-2 text-[11px]">
                  <div>
                    <span class="text-muted-foreground">输入 </span>
                    <span class="font-mono text-foreground tabular-nums">{{
                      formatPrice(p.inputPricePer1m)
                    }}</span>
                  </div>
                  <div>
                    <span class="text-muted-foreground">输出 </span>
                    <span class="font-mono text-foreground tabular-nums">{{
                      formatPrice(p.outputPricePer1m)
                    }}</span>
                  </div>
                  <div>
                    <span class="text-muted-foreground">缓存 </span>
                    <span class="font-mono text-foreground tabular-nums">{{
                      formatPrice(p.cacheReadPricePer1m)
                    }}</span>
                  </div>
                </div>
                <div class="mt-1.5 flex flex-wrap gap-1 text-[10px]">
                  <span
                    v-if="p.toolCalling"
                    class="rounded bg-[#22C55E]/10 px-1 py-0 text-[#22C55E]">工具</span
                  >
                  <span
                    v-if="p.vision"
                    class="rounded bg-[#22C55E]/10 px-1 py-0 text-[#22C55E]">视觉</span
                  >
                  <span
                    v-if="p.thinking"
                    class="rounded bg-[#22C55E]/10 px-1 py-0 text-[#22C55E]">推理</span
                  >
                </div>
              </div>
            </div>
          </div>
        </div>
      </SheetContent>
    </Sheet>
  </div>
</template>
