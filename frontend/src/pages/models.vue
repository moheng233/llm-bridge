<script setup lang="ts">
import { type ModelProviderSummary } from "@bindings/ModelProviderSummary";
import { type ModelResponse } from "@bindings/ModelResponse";

import { getApi, formatTokens, formatPrice } from "~/lib/api";
import { protocolLabel } from "~/lib/constants";
const route = useRoute();
const onlyAvailable = ref(true);
const search = ref("");
const capabilities = reactive({ toolCalling: false, vision: false, thinking: false });
const capabilityOptions = [
  { key: "toolCalling", label: "工具调用" },
  { key: "vision", label: "视觉" },
  { key: "thinking", label: "推理" },
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
const copyError = ref("");
const copied = ref(false);
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
  copied.value = false;
  copyError.value = "";
  sheetOpen.value = true;
}
async function copyModel() {
  if (!selectedModel.value) return;
  try {
    if (!navigator.clipboard) throw Error();
    await navigator.clipboard.writeText(selectedModel.value.modelName);
    copied.value = true;
  } catch {
    copyError.value = "复制失败，请手动选择下方模型 ID 复制。";
  }
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
function override(value: boolean | null, nominal: boolean | null) {
  return value === null
    ? `继承（${nominal ? "支持" : "不声明支持"}）`
    : value
      ? "支持（覆盖）"
      : "不支持（覆盖）";
}
watch(onlyAvailable, () => call.execute());
watch(
  () => route.query.model,
  (value) => {
    if (typeof value === "string") search.value = value;
  },
  { immediate: true },
);
onMounted(() => call.execute());
</script>

<template>
  <PageShell>
    <div>
      <h1>使用模型</h1>
      <p class="mt-2 text-muted-foreground">
        本地网关模型的标称能力与连接参考价格；不是外部目录或实时健康监控。
      </p>
    </div>
    <div class="flex flex-wrap items-center gap-3">
      <Input
        v-model="search"
        class="min-w-0 md:max-w-md"
        placeholder="搜索模型 ID / 名称 / 描述"
        aria-label="搜索模型"
      /><Button
        v-for="option in capabilityOptions"
        :key="option.key"
        :variant="capabilities[option.key] ? 'default' : 'outline'"
        :aria-pressed="capabilities[option.key]"
        @click="capabilities[option.key] = !capabilities[option.key]"
        >{{ option.label }}</Button
      ><label class="flex min-h-11 items-center gap-2"
        ><Checkbox v-model="onlyAvailable" />仅可路由</label
      >
    </div>
    <p class="text-sm text-muted-foreground">
      仅可路由基于本地配置，不代表上游实时健康，也不替代个人 Token
      范围与额度校验。参考价格只比较已启用连接，单位 USD / 百万 Token；未知不等于免费。
    </p>
    <div class="flex flex-wrap items-center gap-2">
      <span>排序：</span
      ><Button
        v-for="column in columns"
        :key="column.key"
        variant="outline"
        :aria-pressed="sortField === column.key"
        @click="sort(column.key)"
        >{{ column.label
        }}{{ sortField === column.key ? (sortDir === "asc" ? " ↑" : " ↓") : "" }}</Button
      >
    </div>
    <ErrorState v-if="call.error.value" :error="call.error.value" @retry="call.execute" />
    <div v-else-if="call.loading.value" class="space-y-3">
      <Skeleton v-for="i in 6" :key="i" class="h-14" />
    </div>
    <div v-else-if="!filtered.length" class="space-y-3 rounded border bg-card p-6">
      <p>
        {{
          search || Object.values(capabilities).some(Boolean)
            ? "筛选无结果"
            : onlyAvailable
              ? "暂无可路由模型，请联系管理员配置或查看全部本地定义。"
              : "尚无本地模型定义，请联系管理员。"
        }}
      </p>
      <Button
        v-if="search || Object.values(capabilities).some(Boolean)"
        variant="outline"
        @click="clearFilters"
        >清除筛选</Button
      ><Button v-else-if="onlyAvailable" variant="outline" @click="onlyAvailable = false"
        >查看全部本地定义</Button
      >
    </div>
    <template v-else>
      <div class="hidden overflow-x-auto rounded border bg-card md:block">
        <table class="w-full text-left text-sm">
          <thead>
            <tr class="border-b">
              <th class="p-3">模型 / 网关 ID</th>
              <th class="p-3">最大输入</th>
              <th class="p-3">最大输出</th>
              <th class="p-3">声明能力</th>
              <th class="p-3">参考输入 / 输出价格</th>
              <th class="p-3">配置连接</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="model in filtered"
              :key="model.modelName"
              class="min-h-14 border-b last:border-0"
            >
              <td class="p-3">
                <Button
                  variant="link"
                  class="h-auto justify-start px-0 text-left whitespace-normal"
                  @click="open(model)"
                  >{{ model.displayName || model.modelName }}</Button
                >
                <p class="font-mono text-xs break-all">{{ model.modelName }}</p>
              </td>
              <td class="p-3 tabular-nums">{{ formatTokens(model.maxInputTokens) }}</td>
              <td class="p-3 tabular-nums">{{ formatTokens(model.maxOutputTokens) }}</td>
              <td class="p-3">{{ declarations(model) }}</td>
              <td class="p-3 tabular-nums">
                {{ referencePrice(model.providers, "inputPricePer1m") }} /
                {{ referencePrice(model.providers, "outputPricePer1m") }}
              </td>
              <td class="p-3">{{ model.providers.length }} 条</td>
            </tr>
          </tbody>
        </table>
      </div>
      <article
        v-for="model in filtered"
        :key="model.modelName"
        class="space-y-3 rounded border bg-card p-4 md:hidden"
      >
        <Button
          variant="link"
          class="h-auto justify-start p-0 text-left text-lg whitespace-normal"
          @click="open(model)"
          >{{ model.displayName || model.modelName }}</Button
        >
        <p class="font-mono break-all">{{ model.modelName }}</p>
        <p>
          最大输入 {{ formatTokens(model.maxInputTokens) }} / 最大输出
          {{ formatTokens(model.maxOutputTokens) }} Token
        </p>
        <p>{{ declarations(model) }}</p>
        <p>
          参考输入 / 输出：{{ referencePrice(model.providers, "inputPricePer1m") }} /
          {{ referencePrice(model.providers, "outputPricePer1m") }}
        </p>
        <p>{{ model.providers.length }} 条配置连接</p>
      </article>
    </template>
    <Sheet v-model:open="sheetOpen"
      ><SheetContent class="w-full max-w-full sm:max-w-2xl"
        ><SheetHeader class="border-b p-4 pr-10"
          ><SheetTitle class="break-words">{{
            selectedModel?.displayName || selectedModel?.modelName
          }}</SheetTitle
          ><SheetDescription
            >使用网关模型 ID 调用；以下配置与能力声明不代表实时连通性。</SheetDescription
          ></SheetHeader
        >
        <div v-if="selectedModel" class="min-h-0 flex-1 space-y-6 overflow-y-auto p-4">
          <div class="space-y-3">
            <code class="block break-all select-all">{{ selectedModel.modelName }}</code
            ><Button variant="outline" @click="copyModel">{{
              copied ? "已复制" : "复制模型 ID"
            }}</Button>
            <p v-if="copyError" role="alert" class="text-destructive">{{ copyError }}</p>
            <ClientConnectionInfo :model-name="selectedModel.modelName" />
          </div>
          <section class="space-y-3">
            <h2>标称能力</h2>
            <p class="break-words whitespace-pre-wrap">
              {{ selectedModel.description || "暂无描述" }}
            </p>
            <p class="tabular-nums">
              最大输入 {{ selectedModel.maxInputTokens }} / 最大输出
              {{ selectedModel.maxOutputTokens }} Token
            </p>
            <p>{{ declarations(selectedModel) }}</p>
          </section>
          <section class="space-y-3">
            <h2>提供者连接 · {{ selectedModel.providers.length }} 条</h2>
            <p class="text-sm text-muted-foreground">
              价格 USD / 百万 Token；null
              保留继承，显式覆盖只作用于本连接。连接优先级数字越小越优先。
            </p>
            <p v-if="!sortedConnections.length">尚未关联提供者。</p>
            <article
              v-for="(connection, index) in sortedConnections"
              :key="index"
              class="space-y-2 rounded border p-4"
            >
              <h3 class="font-semibold break-words">
                {{ connection.providerDisplayName || connection.providerId }}
              </h3>
              <p class="font-mono break-all">{{ connection.providerModelId }}</p>
              <p>
                {{ protocolLabel(connection.compatibility) }} ·
                {{ connection.enabled ? "✓ 配置启用" : "− 配置停用" }} · 优先级
                {{ connection.priority }}
              </p>
              <p>
                输入 / 输出 / 缓存价格：{{
                  connection.inputPricePer1m === null
                    ? "未知"
                    : formatPrice(connection.inputPricePer1m)
                }}
                /
                {{
                  connection.outputPricePer1m === null
                    ? "未知"
                    : formatPrice(connection.outputPricePer1m)
                }}
                /
                {{
                  connection.cacheReadPricePer1m === null
                    ? "未知"
                    : formatPrice(connection.cacheReadPricePer1m)
                }}
              </p>
              <p>
                最大输入：{{
                  connection.maxInputTokens === null
                    ? `继承（${selectedModel.maxInputTokens}）`
                    : `${connection.maxInputTokens}（覆盖）`
                }}；最大输出：{{
                  connection.maxOutputTokens === null
                    ? `继承（${selectedModel.maxOutputTokens}）`
                    : `${connection.maxOutputTokens}（覆盖）`
                }}
              </p>
              <p>
                工具调用：{{
                  override(connection.toolCalling, selectedModel.toolCalling)
                }}；视觉：{{ override(connection.vision, selectedModel.vision) }}
              </p>
              <p>
                推理：{{ override(connection.thinking, selectedModel.thinking) }}；自适应推理：{{
                  override(connection.adaptiveThinking, selectedModel.adaptiveThinking)
                }}
              </p>
            </article>
          </section>
        </div></SheetContent
      ></Sheet
    >
  </PageShell>
</template>
