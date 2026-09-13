<script setup lang="ts">
import { type ModelProviderSummary } from "@bindings/ModelProviderSummary";
import { type ModelResponse } from "@bindings/ModelResponse";
import {
  ArrowDownWideNarrow,
  ArrowUpWideNarrow,
  Cable,
  Check,
  Cpu,
  Eye,
  Minus,
  Server,
} from "@lucide/vue";

import ModelCapabilities from "~/components/common/ModelCapabilities.vue";
import { getApi, formatTokens, formatPrice } from "~/lib/api";
import { protocolLabel } from "~/lib/constants";
const route = useRoute();
const onlyAvailable = ref(typeof route.query.model !== "string");
const search = ref("");
const capabilities = reactive({ toolCalling: false, vision: false, thinking: false });
const capabilityOptions = [
  { key: "toolCalling", label: "工具调用" },
  { key: "vision", label: "视觉" },
  { key: "thinking", label: "推理" },
] as const;
const connectionPriceFields = [
  { key: "inputPricePer1m", label: "输入" },
  { key: "outputPricePer1m", label: "输出" },
  { key: "cacheReadPricePer1m", label: "缓存读取" },
] as const;
const sortField = ref<"name" | "maxInputTokens" | "maxOutputTokens" | "inputPrice">("name");
const sortDir = ref<"asc" | "desc">("asc");
const columns = [
  { key: "name", label: "模型" },
  { key: "maxInputTokens", label: "最大输入 Token" },
  { key: "maxOutputTokens", label: "最大输出 Token" },
  { key: "inputPrice", label: "参考输入价格" },
] as const;
const call = useApiCall(() =>
  onlyAvailable.value ? getApi().models.listAvailableModels() : getApi().models.listAllModels(),
);
const sheetOpen = ref(false);
const selectedModel = ref<ModelResponse | null>(null);
function connectionNotice(model: ModelResponse) {
  if (!model.providers.length) return "尚未关联提供者，暂不可路由。";
  if (!model.providers.some((provider) => provider.enabled))
    return "暂无启用连接；请检查提供者、协议和连接的启用状态。";
  return "";
}
type PriceField = "inputPricePer1m" | "outputPricePer1m" | "cacheReadPricePer1m";
function priceRange(
  providers: ModelProviderSummary[],
  field: PriceField,
): { min: number; max: number } | null {
  let min = Infinity,
    max = -Infinity;
  for (const provider of providers) {
    const price = provider[field];
    if (provider.enabled && price !== null) {
      min = Math.min(min, price);
      max = Math.max(max, price);
    }
  }
  return min === Infinity ? null : { min, max };
}
function referencePrice(providers: ModelProviderSummary[], field: PriceField) {
  const range = priceRange(providers, field);
  if (!range) return "未知";
  return range.min === range.max
    ? formatPrice(range.min)
    : `${formatPrice(range.min)} – ${formatPrice(range.max)}`;
}
const filtered = computed(() => {
  const query = search.value.trim().toLowerCase();
  const rows = (call.data.value ?? []).filter(
    (model) =>
      `${model.modelName} ${model.displayName} ${model.description ?? ""}`
        .toLowerCase()
        .includes(query) &&
      (!capabilities.toolCalling || model.toolCalling) &&
      (!capabilities.vision || model.vision) &&
      (!capabilities.thinking || model.thinking || model.adaptiveThinking),
  );
  const price = new Map(
    rows.map((model) => [
      model.modelName,
      priceRange(model.providers, "inputPricePer1m")?.min ?? null,
    ]),
  );
  return rows.sort((a, b) => {
    const dir = sortDir.value === "asc" ? 1 : -1;
    if (sortField.value === "name") return dir * a.modelName.localeCompare(b.modelName);
    if (sortField.value === "inputPrice") {
      const x = price.get(a.modelName) ?? null,
        y = price.get(b.modelName) ?? null;
      if (x === null) return y === null ? a.modelName.localeCompare(b.modelName) : 1;
      if (y === null) return -1;
      return dir * (x - y) || a.modelName.localeCompare(b.modelName);
    }
    return dir * (a[sortField.value] - b[sortField.value]);
  });
});
const sortedConnections = computed(() =>
  [...(selectedModel.value?.providers ?? [])].sort(
    (a, b) =>
      Number(b.enabled) - Number(a.enabled) ||
      a.priority - b.priority ||
      a.providerDisplayName.localeCompare(b.providerDisplayName),
  ),
);
const paginationKey = computed(() =>
  JSON.stringify([search.value, capabilities, sortField.value, sortDir.value, onlyAvailable.value]),
);
const { page, visible } = useListPagination(filtered, paginationKey);
function sort(field: typeof sortField.value) {
  if (sortField.value === field) sortDir.value = sortDir.value === "asc" ? "desc" : "asc";
  else {
    sortField.value = field;
    sortDir.value = "asc";
  }
}
function clearFilters() {
  search.value = "";
  capabilities.toolCalling = false;
  capabilities.vision = false;
  capabilities.thinking = false;
}
function open(model: ModelResponse) {
  selectedModel.value = model;
  sheetOpen.value = true;
}
function declarations(model: ModelResponse) {
  return (
    [
      model.toolCalling && "工具调用",
      model.vision && "视觉",
      model.thinking && "推理",
      model.adaptiveThinking && "自适应推理",
    ]
      .filter(Boolean)
      .join(" · ") || "未声明支持特殊能力"
  );
}
watch(onlyAvailable, () => call.execute());
watch(
  () => route.query.model,
  (value) => {
    if (typeof value === "string") {
      search.value = value;
      onlyAvailable.value = false;
      capabilities.toolCalling = false;
      capabilities.vision = false;
      capabilities.thinking = false;
    }
  },
  { immediate: true },
);
onMounted(() => call.execute());
</script>

<template>
  <PageShell :reset-key="`${page}:${paginationKey}`">
    <template #header
      ><SectionHeader
        title="使用模型"
        :icon="Cpu"
        :count="call.data.value?.length ?? null"
        count-label="个"
    /></template>
    <template #toolbar>
      <ListToolbar
        v-model="search"
        collapse-filters
        label="搜索模型"
        placeholder="搜索模型 ID / 名称 / 描述"
        :loading="call.loading.value"
        :filtered="Object.values(capabilities).some(Boolean)"
        @refresh="call.execute"
        @clear="clearFilters"
      >
        <label
          v-for="option in capabilityOptions"
          :key="option.key"
          class="flex min-h-11 items-center gap-2"
          ><Checkbox v-model="capabilities[option.key]" />{{ option.label }}</label
        >
        <label class="flex min-h-11 items-center gap-2"
          ><Checkbox v-model="onlyAvailable" />仅可路由</label
        >
        <div class="flex items-center gap-2">
          <Select
            :model-value="sortField"
            @update:model-value="(value) => sort(value as typeof sortField)"
            ><SelectTrigger aria-label="模型排序字段" class="w-40"><SelectValue /></SelectTrigger
            ><SelectContent
              ><SelectItem v-for="column in columns" :key="column.key" :value="column.key">{{
                column.label
              }}</SelectItem></SelectContent
            ></Select
          >
          <Button
            variant="outline"
            size="icon"
            :aria-label="sortDir === 'asc' ? '升序，切换为降序' : '降序，切换为升序'"
            :title="sortDir === 'asc' ? '升序，切换为降序' : '降序，切换为升序'"
            @click="sortDir = sortDir === 'asc' ? 'desc' : 'asc'"
            ><ArrowUpWideNarrow v-if="sortDir === 'asc'" aria-hidden="true" /><ArrowDownWideNarrow
              v-else
              aria-hidden="true"
          /></Button>
        </div>
        <p class="text-xs text-muted-foreground">
          仅可路由基于本地配置，不替代个人 Token 范围与额度校验。价格单位 USD / 百万
          Token；未知不等于免费。
        </p>
      </ListToolbar>
    </template>
    <p class="text-muted-foreground">
      本地网关模型的标称能力与连接参考价格；配置状态不代表上游实时可达。
    </p>
    <ErrorState v-if="call.error.value" :error="call.error.value" @retry="call.execute" />
    <div v-else-if="call.loading.value" role="status" aria-label="加载列表" class="space-y-3">
      <Skeleton v-for="index in 4" :key="index" class="h-24 rounded-md" />
    </div>
    <EmptyState
      v-else-if="!filtered.length"
      :icon="Cpu"
      :title="
        search || Object.values(capabilities).some(Boolean)
          ? '筛选无结果'
          : onlyAvailable
            ? '暂无可路由模型'
            : '尚无本地模型定义'
      "
    >
      <template #actions>
        <Button
          v-if="search || Object.values(capabilities).some(Boolean)"
          variant="outline"
          @click="clearFilters"
          >清除筛选</Button
        ><Button v-if="onlyAvailable" variant="outline" @click="onlyAvailable = false"
          >查看全部本地定义</Button
        >
      </template>
    </EmptyState>
    <template v-else>
      <ListItem v-for="model in visible" :key="model.modelName">
        <template #title
          ><button type="button" @click="open(model)">
            {{ model.displayName || model.modelName }}
          </button></template
        >
        <template #subtitle>{{ model.modelName }}</template>
        <template #status
          ><Badge variant="outline">{{ model.providers.length }} 条连接</Badge></template
        >
        <p v-if="connectionNotice(model)" class="text-sm text-warning">
          {{ connectionNotice(model) }}
        </p>
        <p>
          最大输入 {{ formatTokens(model.maxInputTokens) }} / 最大输出
          {{ formatTokens(model.maxOutputTokens) }} Token
        </p>
        <p>{{ declarations(model) }}</p>
        <p>
          参考输入 / 输出：{{ referencePrice(model.providers, "inputPricePer1m") }} /
          {{ referencePrice(model.providers, "outputPricePer1m") }}（USD / 百万 Token）
        </p>
        <template #actions
          ><Button variant="outline" @click="open(model)"
            ><Eye aria-hidden="true" />查看</Button
          ></template
        >
      </ListItem>
    </template>
    <template v-if="call.data.value && !call.error.value" #footer
      ><ListPagination v-model="page" :total="filtered.length" :disabled="call.loading.value"
    /></template>
    <Sheet v-model:open="sheetOpen"
      ><SheetContent class="w-full max-w-full gap-0 sm:max-w-2xl"
        ><SheetHeader class="shrink-0 border-b p-4 pr-12"
          ><SheetTitle class="text-lg font-semibold break-words">{{
            selectedModel?.displayName || selectedModel?.modelName
          }}</SheetTitle
          ><SheetDescription class="text-xs leading-5"
            >使用网关模型 ID 调用；以下配置与能力声明不代表实时连通性。</SheetDescription
          ></SheetHeader
        >
        <div
          v-if="selectedModel"
          data-scroll-area
          class="min-h-0 flex-1 space-y-4 overflow-y-auto overscroll-contain p-4"
        >
          <p v-if="connectionNotice(selectedModel)" role="status" class="text-sm text-warning">
            {{ connectionNotice(selectedModel) }}
          </p>
          <ClientConnectionInfo :model-name="selectedModel.modelName" />
          <section class="space-y-4 border-t pt-5">
            <div class="space-y-2">
              <h2 class="text-base font-semibold">标称能力</h2>
              <p class="text-sm leading-6 break-words whitespace-pre-wrap text-muted-foreground">
                {{ selectedModel.description || "暂无描述" }}
              </p>
            </div>
            <ModelCapabilities :model="selectedModel" />
          </section>
          <section class="space-y-4 border-t pt-5">
            <div class="flex items-center justify-between gap-3">
              <h2 class="text-base font-semibold">提供者连接</h2>
              <span class="text-xs text-muted-foreground tabular-nums">
                {{ selectedModel.providers.length }} 条
              </span>
            </div>
            <p class="text-xs leading-5 text-muted-foreground">
              未覆盖的能力与 Token 上限继承标称值；覆盖仅作用于本连接。优先级数字越小越优先。
            </p>
            <div
              v-if="!sortedConnections.length"
              class="flex items-center gap-2 py-6 text-sm text-muted-foreground"
            >
              <Cable class="size-5 shrink-0" aria-hidden="true" />
              <p>尚未关联提供者。</p>
            </div>
            <article
              v-for="(connection, index) in sortedConnections"
              :key="index"
              class="min-w-0 overflow-hidden rounded-lg border bg-card text-sm"
            >
              <div class="space-y-3 p-4">
                <div class="flex items-start justify-between gap-3">
                  <div class="flex min-w-0 items-start gap-2">
                    <Server
                      class="mt-0.5 size-4 shrink-0 text-muted-foreground"
                      aria-hidden="true"
                    />
                    <h3 class="min-w-0 font-semibold break-all">
                      {{ connection.providerDisplayName || connection.providerId }}
                    </h3>
                  </div>
                  <span
                    class="inline-flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-xs font-medium"
                    :class="
                      connection.enabled
                        ? 'bg-primary/10 text-primary'
                        : 'bg-muted text-muted-foreground'
                    "
                  >
                    <Check v-if="connection.enabled" class="size-3 shrink-0" aria-hidden="true" />
                    <Minus v-else class="size-3 shrink-0" aria-hidden="true" />
                    {{ connection.enabled ? "配置启用" : "配置停用" }}
                  </span>
                </div>
                <code class="block font-mono text-xs break-all text-muted-foreground select-text">{{
                  connection.providerModelId
                }}</code>
                <div
                  class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 text-xs text-muted-foreground"
                >
                  <span class="inline-flex min-w-0 items-center gap-1.5">
                    <Cable class="size-3.5 shrink-0" aria-hidden="true" />
                    <span class="min-w-0 break-words">{{
                      protocolLabel(connection.compatibility)
                    }}</span>
                  </span>
                  <span class="inline-flex items-center gap-1.5">
                    <ArrowDownWideNarrow class="size-3.5 shrink-0" aria-hidden="true" />
                    优先级
                    <span class="font-medium text-foreground tabular-nums">{{
                      connection.priority
                    }}</span>
                  </span>
                </div>
              </div>
              <div class="space-y-3 border-y bg-muted/30 px-4 py-3">
                <div
                  class="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"
                >
                  <h4 class="font-medium">参考价格</h4>
                  <span>USD / 百万 Token</span>
                </div>
                <dl class="grid grid-cols-3 gap-3">
                  <div
                    v-for="field in connectionPriceFields"
                    :key="field.key"
                    class="min-w-0 space-y-1"
                  >
                    <dt class="text-xs text-muted-foreground">{{ field.label }}</dt>
                    <dd
                      class="text-base font-medium break-all tabular-nums"
                      :class="
                        connection[field.key] === null ? 'text-muted-foreground' : 'text-foreground'
                      "
                    >
                      {{
                        connection[field.key] === null ? "未知" : formatPrice(connection[field.key])
                      }}
                    </dd>
                  </div>
                </dl>
              </div>
              <ModelCapabilities :model="selectedModel" :overrides="connection" class="p-4" />
            </article>
          </section></div></SheetContent
    ></Sheet>
  </PageShell>
</template>
