<script setup lang="ts">
import { type ModelProviderSummary } from "@bindings/ModelProviderSummary";
import { type ModelResponse } from "@bindings/ModelResponse";
import { Search, Cpu, Eye, Wrench, Brain } from "@lucide/vue";

import { getApi, formatTokens, formatPrice } from "~/lib/api";
import { SKELETON_ROWS } from "~/lib/constants";

const api = getApi();

function cheapestInputPrice(providers: ModelProviderSummary[]): number | null {
  const prices = providers.map((p) => p.inputPricePer1m).filter((v): v is number => v != null);
  return prices.length > 0 ? Math.min(...prices) : null;
}
function cheapestOutputPrice(providers: ModelProviderSummary[]): number | null {
  const prices = providers.map((p) => p.outputPricePer1m).filter((v): v is number => v != null);
  return prices.length > 0 ? Math.min(...prices) : null;
}

const models = ref<ModelResponse[]>([]);
const loading = ref(true);
const error = ref("");
const search = ref("");
const onlyAvailable = ref(false);
const sortField = ref<"name" | "maxInputTokens" | "maxOutputTokens">("name");
const sortDir = ref<"asc" | "desc">("asc");

async function load() {
  loading.value = true;
  error.value = "";
  try {
    models.value = onlyAvailable.value
      ? await api.models.listAvailableModels()
      : await api.models.listAllModels();
  } catch (e: any) {
    error.value = e.message;
  } finally {
    loading.value = false;
  }
}

watchEffect(() => {
  onlyAvailable.value;
  load();
});

const filteredModels = computed(() => {
  let result = models.value;
  if (search.value.trim()) {
    const q = search.value.toLowerCase();
    result = result.filter(
      (m) => m.modelName.toLowerCase().includes(q) || m.description?.toLowerCase().includes(q),
    );
  }
  result = [...result].sort((a, b) => {
    const dir = sortDir.value === "asc" ? 1 : -1;
    if (sortField.value === "name") return dir * a.modelName.localeCompare(b.modelName);
    if (sortField.value === "maxInputTokens") return dir * (a.maxInputTokens - b.maxInputTokens);
    if (sortField.value === "maxOutputTokens") return dir * (a.maxOutputTokens - b.maxOutputTokens);
    return 0;
  });
  return result;
});

function toggleSort(field: typeof sortField.value) {
  if (sortField.value === field) {
    sortDir.value = sortDir.value === "asc" ? "desc" : "asc";
  } else {
    sortField.value = field;
    sortDir.value = "desc";
  }
}

function sortIndicator(field: typeof sortField.value): string {
  if (sortField.value !== field) return "";
  return sortDir.value === "asc" ? " ↑" : " ↓";
}
</script>

<template>
  <div class="flex h-full min-h-0 flex-col gap-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="font-mono text-xl font-bold text-foreground">模型目录</h2>
        <p class="mt-1 text-sm text-muted-foreground">浏览可用模型的能力与定价</p>
      </div>
      <Badge variant="secondary" class="font-mono">{{ models.length }} 个模型</Badge>
    </div>

    <Alert v-if="error" class="border-destructive/30 bg-destructive/10">
      <AlertDescription class="text-sm text-destructive">{{ error }}</AlertDescription>
    </Alert>

    <div class="flex items-center gap-3">
      <div class="relative max-w-md flex-1">
        <Search class="absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <Input v-model="search" class="pl-9" placeholder="搜索模型名称或描述..." />
      </div>
      <Label
        class="flex cursor-pointer items-center gap-2 text-sm whitespace-nowrap text-muted-foreground"
      >
        <Checkbox v-model:checked="onlyAvailable" class="cursor-pointer" /> 仅可用
      </Label>
    </div>

    <div v-if="loading" class="flex flex-col gap-2">
      <Skeleton v-for="i in SKELETON_ROWS.models" :key="i" class="h-12 w-full rounded-lg" />
    </div>
    <div v-else class="min-h-0 flex-1 overflow-auto rounded-lg border border-border">
      <table class="w-full text-sm">
        <thead class="sticky top-0 border-b border-border bg-card">
          <tr>
            <th
              class="cursor-pointer px-4 py-3 text-left font-mono text-xs text-muted-foreground hover:text-foreground"
              @click="toggleSort('name')"
            >
              模型名称{{ sortIndicator("name") }}
            </th>
            <th
              class="cursor-pointer px-4 py-3 text-right font-mono text-xs text-muted-foreground hover:text-foreground"
              @click="toggleSort('maxInputTokens')"
            >
              输入{{ sortIndicator("maxInputTokens") }}
            </th>
            <th
              class="cursor-pointer px-4 py-3 text-right font-mono text-xs text-muted-foreground hover:text-foreground"
              @click="toggleSort('maxOutputTokens')"
            >
              输出{{ sortIndicator("maxOutputTokens") }}
            </th>
            <th class="px-4 py-3 text-center font-mono text-xs text-muted-foreground">能力</th>
            <th class="px-4 py-3 text-right font-mono text-xs text-muted-foreground">输入价格</th>
            <th class="px-4 py-3 text-right font-mono text-xs text-muted-foreground">输出价格</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="model in filteredModels"
            :key="model.modelName"
            class="cursor-pointer border-b border-border/50 transition-colors hover:bg-accent/50"
          >
            <td class="px-4 py-3">
              <div class="flex flex-col gap-0.5">
                <span class="font-mono font-medium text-foreground">{{ model.modelName }}</span>
                <span v-if="model.description" class="line-clamp-1 text-xs text-muted-foreground">{{
                  model.description
                }}</span>
                <div v-if="model.providers.length > 0" class="mt-1 flex flex-wrap gap-1">
                  <Badge
                    v-for="p in model.providers"
                    :key="p.providerModelId"
                    variant="outline"
                    class="px-1.5 py-0 font-mono text-xs"
                    :title="p.providerModelId"
                    >{{ p.providerDisplayName }}</Badge
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
                  class="h-3.5 w-3.5 text-[#22C55E]"
                  title="工具调用"
                />
                <Eye v-if="model.vision" class="h-3.5 w-3.5 text-[#22C55E]" title="视觉" />
                <Brain v-if="model.thinking" class="h-3.5 w-3.5 text-[#22C55E]" title="推理" />
              </div>
            </td>
            <td class="px-4 py-3 text-right font-mono text-foreground tabular-nums">
              {{ formatPrice(cheapestInputPrice(model.providers)) }}
            </td>
            <td class="px-4 py-3 text-right font-mono text-foreground tabular-nums">
              {{ formatPrice(cheapestOutputPrice(model.providers)) }}
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
  </div>
</template>
