<script setup lang="ts">
import { useConnectionSetup } from "~/composables/useConnectionSetup";
import { useUnsavedChanges } from "~/composables/useUnsavedChanges";
const route = useRoute();
const router = useRouter();
const s = useConnectionSetup({
  providerId: route.query.providerId === undefined ? undefined : Number(route.query.providerId),
  modelId: route.query.modelId === undefined ? undefined : Number(route.query.modelId),
  templateProviderId:
    typeof route.query.templateProviderId === "string" ? route.query.templateProviderId : undefined,
});
const {
  step,
  catalog,
  savedProvider,
  providerDraft,
  providers,
  models,
  lockedModel,
  modelDrafts,
  selectedConnections,
  activeKey,
  fieldErrors,
  saving,
  loading,
  result,
  error,
  contextError,
  uncertain,
  referenceProvider,
  conflictingModel,
} = s;
const { confirmDiscard } = useUnsavedChanges(s.dirty, saving);
const providerMode = ref<"catalog" | "existing" | "manual">("catalog");
const modelMode = ref<"catalog" | "local" | "manual">("catalog");
const selectedTemplate = ref("");
const localSearch = ref("");
const localPage = ref(1);
const targetCatalog = computed(() => {
  if (!catalog.value) return null;
  const protocols = savedProvider.value?.protocols.filter((protocol) => protocol.enabled) ?? [];
  const protocolId = protocols.length === 1 ? protocols[0]!.id : null;
  const modelIds = new Map(models.value.map((model) => [model.modelName, model.id]));
  const existing = new Map<number, Set<string>>();
  for (const link of s.localLinks.value) {
    if (link.protocolId !== protocolId) continue;
    const aliases = existing.get(link.modelId) ?? new Set<string>();
    aliases.add(link.providerModelId);
    existing.set(link.modelId, aliases);
  }
  return {
    ...catalog.value,
    links: catalog.value.links
      .filter((row) => row.link.providerId === referenceProvider.value)
      .map((row) => ({
        ...row,
        exists:
          existing.get(modelIds.get(row.link.modelName) ?? -1)?.has(row.link.providerModelId) ??
          false,
      })),
  };
});
const localCandidates = computed(() =>
  models.value.filter(
    (model) =>
      (!lockedModel.value || lockedModel.value.id === model.id) &&
      `${model.modelName} ${model.displayName} ${model.description ?? ""}`
        .toLowerCase()
        .includes(localSearch.value.toLowerCase()),
  ),
);
const active = computed(() =>
  selectedConnections.value.find((item) => item.key === activeKey.value),
);
const activeIndex = computed(() =>
  selectedConnections.value.findIndex((item) => item.key === activeKey.value),
);
const activeErrors = computed(() =>
  Object.fromEntries(
    Object.entries(fieldErrors.value)
      .filter(([key]) => key.startsWith(`${activeIndex.value}.`) && !key.includes(".model."))
      .map(([key, value]) => [key.slice(`${activeIndex.value}.`.length), value]),
  ),
);
const nominalErrors = computed(() =>
  Object.fromEntries(
    Object.entries(fieldErrors.value)
      .filter(([key]) => key.startsWith(`${activeIndex.value}.model.`))
      .map(([key, value]) => [key.slice(`${activeIndex.value}.model.`.length), value]),
  ),
);
const resultEntries = computed(
  () =>
    result.value?.items.map((item, index) => {
      const selected = selectedConnections.value[index]!;
      return {
        ...item,
        modelName: selected.existingModelId
          ? (models.value.find((model) => model.id === selected.existingModelId)?.modelName ??
            selected.modelKey)
          : modelDrafts[selected.modelKey]!.modelName,
      };
    }) ?? [],
);
const duplicateProvider = computed(() =>
  providers.value.find((provider) => provider.providerId === selectedTemplate.value),
);
async function choose(value: number | string | null) {
  if (!(await confirmDiscard())) return;
  await s.selectProvider(value);
}
async function changeProvider() {
  if (!(await confirmDiscard())) return;
  s.reset();
  await router.replace({
    path: "/admin/setup",
    query: lockedModel.value ? { modelId: String(lockedModel.value.id) } : {},
  });
}
async function exit() {
  await router.push(
    lockedModel.value
      ? `/admin/models/${lockedModel.value.id}`
      : savedProvider.value
        ? `/providers/${savedProvider.value.id}`
        : "/providers",
  );
}
async function continueAdding() {
  if (loading.value || !(await s.reloadLocalRecords())) return;
  selectedConnections.value = [];
  for (const key of Object.keys(modelDrafts)) delete modelDrafts[key];
  result.value = null;
  error.value = "";
  step.value = "models";
}
async function referenceChanged(value: unknown) {
  if (selectedConnections.value.length && !(await confirmDiscard())) return;
  selectedConnections.value = [];
  referenceProvider.value = String(value);
}
watch([step, modelMode], () => {
  if (
    (step.value === "provider" || (step.value === "models" && modelMode.value === "catalog")) &&
    !catalog.value &&
    !s.catalogCall.loading.value
  )
    s.loadCatalog();
});
watch(
  () => active.value?.link.protocolId,
  () => {
    if (active.value?.templateProtocol)
      active.value.protocolConfirmed =
        savedProvider.value?.protocols.find((p) => p.id === active.value?.link.protocolId)
          ?.protocol === active.value.templateProtocol;
  },
);
watch(fieldErrors, async (errors) => {
  if (Object.keys(errors).length) {
    await nextTick();
    document.querySelector<HTMLElement>('#setup-content [aria-invalid="true"]')?.focus();
  }
});
onMounted(async () => {
  await s.load();
  if (!contextError.value && !catalog.value) await s.loadCatalog();
});
</script>
<template>
  <PageShell id="setup-content"
    ><div class="flex flex-wrap items-start justify-between gap-3">
      <div>
        <h1>
          接入模型<span v-if="savedProvider"> / {{ savedProvider.displayName }}</span>
        </h1>
        <p class="mt-2 text-muted-foreground">
          目录只在首次创建时预填；保存后独立维护，不同步、不覆盖。
        </p>
        <p v-if="lockedModel" class="mt-2 break-all">
          当前模型：<code>{{ lockedModel.modelName }}</code
          >（已锁定，复用本地定义）
        </p>
      </div>
      <Button variant="outline" :disabled="saving" @click="exit">返回入口</Button>
    </div>
    <ol class="grid grid-cols-2 gap-2 rounded border bg-card p-4 sm:grid-cols-4">
      <li
        v-for="(entry, index) in [
          { key: 'provider', label: '选择提供者' },
          { key: 'configuration', label: '连接设置' },
          { key: 'models', label: '添加模型' },
          { key: 'result', label: '完成' },
        ]"
        :key="entry.key"
        :aria-current="step === entry.key ? 'step' : undefined"
        :class="step === entry.key ? 'font-semibold text-primary' : 'text-muted-foreground'"
      >
        {{ index + 1 }}. {{ entry.label }}
      </li>
    </ol>
    <div v-if="contextError" role="alert" class="space-y-3 rounded border border-destructive p-6">
      <p>{{ contextError }}</p>
      <RouterLink to="/providers" class="text-primary underline">返回提供者</RouterLink>
    </div>
    <Skeleton v-else-if="loading" class="h-48" />
    <template v-else>
      <div v-if="step === 'provider'" class="space-y-4">
        <div class="flex flex-wrap gap-2">
          <Button
            v-for="mode in ['catalog', 'existing', 'manual'] as const"
            :key="mode"
            :variant="providerMode === mode ? 'default' : 'outline'"
            @click="
              providerMode = mode;
              if (mode === 'manual') choose(null);
            "
            >{{
              mode === "catalog"
                ? "从目录选择"
                : mode === "existing"
                  ? "使用已有提供者"
                  : "手动配置"
            }}</Button
          >
        </div>
        <CatalogPicker
          v-if="providerMode === 'catalog'"
          :preview="catalog"
          :loading="s.catalogCall.loading.value"
          :error="s.catalogCall.error.value"
          kind="providers"
          :selected-keys="selectedTemplate ? [selectedTemplate] : []"
          @select="(keys) => (selectedTemplate = keys[0] ?? '')"
          @retry="s.loadCatalog"
          @refresh="s.loadCatalog"
          @manual="choose(null)"
        />
        <div
          v-if="providerMode === 'catalog' && selectedTemplate"
          class="flex flex-wrap items-center gap-3 rounded border p-4"
        >
          <p v-if="duplicateProvider">此目录 ID 已有本地实例，目录不会合并到它。</p>
          <Button v-if="duplicateProvider" variant="outline" @click="choose(duplicateProvider.id)"
            >使用已有提供者</Button
          ><Button @click="choose(selectedTemplate)">{{
            duplicateProvider ? "新建独立实例" : "使用此预设"
          }}</Button>
        </div>
        <div v-if="providerMode === 'existing'" class="space-y-3">
          <p v-if="!providers.length">尚未配置提供者，请从目录选择或手动配置。</p>
          <button
            v-for="provider in providers"
            :key="provider.id"
            class="flex min-h-14 w-full flex-wrap justify-between gap-2 rounded border bg-card p-4 text-left"
            @click="choose(provider.id)"
          >
            <span
              >{{ provider.displayName }} · <code>{{ provider.providerId }}</code></span
            ><span>{{ provider.protocols.length }} 个协议 · {{ provider.modelCount }} 个连接</span>
          </button>
        </div>
      </div>
      <div v-if="step === 'configuration'" class="space-y-4">
        <p v-if="savedProvider" class="rounded border p-3">
          提供者已保存；此处修改将显式更新本地配置。
        </p>
        <p
          v-else-if="referenceProvider && providers.some((p) => p.providerId === referenceProvider)"
          class="rounded border p-3 text-warning"
        >
          新建独立实例：请输入新的实例 ID（例如
          {{ referenceProvider }}-backup），现有实例不会被覆盖。
        </p>
        <ProviderForm
          v-model="providerDraft"
          :is-edit="!!savedProvider"
          :saving="saving"
          submit-label="保存提供者并选择模型"
          :error="error"
          :field-errors="fieldErrors"
          @submit="s.saveProvider()"
          @cancel="exit"
          ><template #actions
            ><Button
              variant="outline"
              type="button"
              :disabled="saving"
              @click="s.saveProvider(false)"
              >只保存提供者</Button
            ></template
          ></ProviderForm
        >
      </div>
      <div v-if="step === 'models' && savedProvider" class="space-y-4">
        <div class="flex flex-wrap gap-3">
          <Button variant="outline" :disabled="saving" @click="step = 'configuration'"
            >编辑连接设置</Button
          ><Button variant="ghost" :disabled="saving" @click="changeProvider"
            >更换提供者（已保存记录保留）</Button
          >
        </div>
        <p v-if="!savedProvider.apiKeys.length" class="rounded border p-3 text-warning">
          已保存，但尚未配置 API Key，不能完成上游测试。
        </p>
        <fieldset :disabled="saving" class="grid min-w-0 gap-6 lg:grid-cols-2">
          <section class="min-w-0 space-y-4">
            <div class="flex flex-wrap gap-2">
              <Button
                type="button"
                v-for="mode in ['catalog', 'local', 'manual'] as const"
                :key="mode"
                :variant="modelMode === mode ? 'default' : 'outline'"
                @click="modelMode = mode"
                >{{
                  mode === "catalog" ? "目录推荐" : mode === "local" ? "本地模型" : "手动新建"
                }}</Button
              >
            </div>
            <template v-if="modelMode === 'catalog'"
              ><Label>选择参考目录提供者（仅用于本次添加模型）</Label
              ><Select
                :model-value="referenceProvider || undefined"
                @update:model-value="referenceChanged"
                ><SelectTrigger><SelectValue placeholder="选择参考目录提供者" /></SelectTrigger
                ><SelectContent
                  ><SelectItem
                    v-for="row in catalog?.providers ?? []"
                    :key="row.provider.providerId"
                    :value="row.provider.providerId"
                    >{{ row.provider.displayName }} · {{ row.provider.providerId }}</SelectItem
                  ></SelectContent
                ></Select
              ><CatalogPicker
                :preview="targetCatalog"
                :loading="s.catalogCall.loading.value"
                :error="s.catalogCall.error.value"
                kind="providerModels"
                :provider-key="referenceProvider || '__no-reference__'"
                :model-key="lockedModel?.modelName"
                :selected-keys="selectedConnections.map((item) => item.key)"
                multiple
                @select="s.selectCatalogKeys"
                @retry="s.loadCatalog"
                @refresh="s.loadCatalog"
                @manual="modelMode = 'manual'" /></template
            ><template v-else-if="modelMode === 'local'"
              ><Input
                v-model="localSearch"
                placeholder="搜索本地模型"
                aria-label="搜索本地模型"
                @update:model-value="localPage = 1"
              /><button
                v-for="model in localCandidates.slice((localPage - 1) * 50, localPage * 50)"
                :key="model.id"
                class="min-h-14 w-full rounded border bg-card p-3 text-left"
                @click="s.addLocal(model.id)"
              >
                {{ model.displayName }} · <code>{{ model.modelName }}</code> · 复用本地定义
              </button>
              <div class="flex justify-between">
                <Button variant="outline" :disabled="localPage <= 1" @click="localPage--"
                  >上一页</Button
                ><Button
                  variant="outline"
                  :disabled="localPage * 50 >= localCandidates.length"
                  @click="localPage++"
                  >下一页</Button
                >
              </div></template
            ><template v-else
              ><p>Token 上限为空且必填；能力默认不声明支持，不使用猜测规格。</p>
              <Button @click="s.addManual">{{
                lockedModel ? "为此模型添加连接" : "新建手动模型草稿"
              }}</Button></template
            >
            <details v-if="s.localLinks.value.length" class="rounded border p-3">
              <summary>已配置 {{ s.localLinks.value.length }} 个连接</summary>
              <p v-for="link in s.localLinks.value" :key="link.id" class="mt-2 break-all">
                {{ models.find((m) => m.id === link.modelId)?.modelName }} →
                {{ link.providerModelId }} · 已添加
                <RouterLink :to="`/admin/models/${link.modelId}`" class="text-primary underline"
                  >查看/编辑连接</RouterLink
                >
              </p>
            </details>
          </section>
          <section class="min-w-0 space-y-4 rounded border bg-card p-4">
            <h2>已选项配置</h2>
            <div class="flex flex-wrap gap-2">
              <Button
                v-for="item in selectedConnections"
                :key="item.key"
                :variant="activeKey === item.key ? 'default' : 'outline'"
                @click="activeKey = item.key"
                >{{ item.link.providerModelId || "新连接" }}</Button
              >
            </div>
            <p v-if="!active">从左侧选择连接；初始不会全选。</p>
            <template v-else
              ><div class="flex flex-wrap justify-between gap-2">
                <p class="break-all">
                  {{
                    active.existingModelId
                      ? active.modelKey
                      : modelDrafts[active.modelKey]?.modelName || "未填写网关 ID"
                  }}
                  → {{ active.link.providerModelId || "未填写上游 ID" }}
                </p>
                <Button
                  variant="ghost"
                  @click="
                    selectedConnections = selectedConnections.filter(
                      (item) => item.key !== active!.key,
                    );
                    activeKey = selectedConnections[0]?.key ?? '';
                  "
                  >取消此选择</Button
                >
              </div>
              <div v-if="active.existingModelId" class="space-y-2 rounded border p-3">
                <p>复用本地模型定义，不会覆盖共享标称信息。</p>
                <p class="text-sm">
                  最大输入
                  {{ models.find((m) => m.id === active!.existingModelId)?.maxInputTokens }} / 输出
                  {{ models.find((m) => m.id === active!.existingModelId)?.maxOutputTokens }}
                </p>
                <RouterLink
                  :to="`/admin/models/${active.existingModelId}?tab=definition`"
                  class="text-primary underline"
                  >查看/编辑本地模型定义</RouterLink
                >
              </div>
              <ModelForm
                v-else
                v-model="modelDrafts[active.modelKey]!"
                :field-errors="nominalErrors"
                :saving="saving"
              />
              <h2>本连接</h2>
              <ModelLinkFields
                v-model="active.link"
                :provider="savedProvider"
                :field-errors="activeErrors"
                :saving="saving"
              /><label
                v-if="
                  active.templateProtocol &&
                  savedProvider.protocols.find((p) => p.id === active!.link.protocolId)
                    ?.protocol !== active.templateProtocol
                "
                class="flex items-start gap-2 rounded border p-3 text-warning"
                ><Checkbox
                  v-model="active.protocolConfirmed"
                />目录协议与目标协议不同；我已确认兼容性并手动检查上游模型 ID。</label
              ></template
            >
          </section>
        </fieldset>
        <div v-if="conflictingModel" class="space-y-3 rounded border border-warning p-4">
          <h2>同名本地模型已存在</h2>
          <p>
            {{ conflictingModel.modelName }} · {{ conflictingModel.displayName }} · 输入
            {{ conflictingModel.maxInputTokens }} / 输出 {{ conflictingModel.maxOutputTokens }}
          </p>
          <p>
            当前草稿：{{ modelDrafts[s.conflictKey.value]?.displayName }} · 输入
            {{ modelDrafts[s.conflictKey.value]?.maxInput }} / 输出
            {{ modelDrafts[s.conflictKey.value]?.maxOutput }}
          </p>
          <Button @click="s.reuseConflict">复用本地定义继续</Button
          ><Button variant="outline" @click="conflictingModel = null">改用其他模型 ID</Button>
        </div>
        <p v-if="error" role="alert" class="rounded border border-destructive p-4 text-destructive">
          {{ error }}
        </p>
        <div v-if="uncertain" class="flex flex-wrap items-center gap-3 rounded border p-4">
          <RouterLink :to="`/providers/${savedProvider.id}`" class="text-primary underline"
            >查看已配置连接</RouterLink
          ><Button variant="outline" @click="s.reloadLocalRecords">重新加载本地记录</Button
          ><Button variant="outline" @click="uncertain = false"
            >已检查本地记录，允许我确认重试</Button
          >
        </div>
        <div class="flex flex-wrap items-center justify-between gap-3 border-t bg-background py-4">
          <p>
            已选 {{ selectedConnections.length }} 个连接 · 新建
            {{
              new Set(
                selectedConnections
                  .filter((item) => !item.existingModelId)
                  .map((item) => item.modelKey),
              ).size
            }}
            个模型定义
          </p>
          <Button
            :disabled="saving || uncertain || !selectedConnections.length"
            @click="s.submitConnections"
            >{{ saving ? "保存中…" : `保存 ${selectedConnections.length} 个连接` }}</Button
          >
        </div>
      </div>
      <div v-if="step === 'result' && result && savedProvider" class="space-y-6">
        <h2>模型连接已保存</h2>
        <p>本批事务已提交；退出不会撤销已保存配置。连接存在不代表上游实时可达。</p>
        <p v-if="error" role="alert" class="text-destructive">{{ error }}</p>
        <ConnectionList :entries="resultEntries" :providers="[savedProvider]" read-only />
        <div class="flex flex-wrap gap-3">
          <RouterLink :to="`/providers/${savedProvider.id}`" class="text-primary underline"
            >查看提供者</RouterLink
          ><Button variant="outline" :disabled="loading" @click="continueAdding"
            >继续添加模型</Button
          ><RouterLink
            :to="`/models?model=${encodeURIComponent(resultEntries[0]?.modelName ?? '')}`"
            class="text-primary underline"
            >查看模型使用方式</RouterLink
          >
        </div>
        <ClientConnectionInfo :model-name="resultEntries[0]?.modelName" />
      </div> </template
  ></PageShell>
</template>
<route lang="json">
{ "meta": { "requiresAdmin": true } }
</route>
