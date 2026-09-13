<script setup lang="ts">
import { ArrowLeft, Cable, Save, SlidersHorizontal } from "@lucide/vue";
import { useMediaQuery } from "@vueuse/core";
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from "reka-ui";

import { useConnectionSetup } from "~/composables/useConnectionSetup";
import { useUnsavedChanges } from "~/composables/useUnsavedChanges";
import { focusInScrollArea } from "~/lib/utils";
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
  providerRecovery,
  providerRecoveryChecked,
  referenceProvider,
  conflictingModel,
} = s;
const { confirmDiscard } = useUnsavedChanges(s.dirty, saving);
const providerMode = ref<"catalog" | "existing" | "manual">("catalog");
const modelMode = ref<"catalog" | "local" | "manual">("catalog");
const modelPanel = ref("selection");
const wide = useMediaQuery("(min-width: 1024px)");
const short = useMediaQuery("(max-height: 640px)");
const selectedTemplate = ref("");
const localSearch = ref("");
const localPage = ref(1);
const providerQuery = ref("");
const providerPage = ref(1);
const modelQuery = ref("");
const catalogPage = ref(1);
const existingOpen = ref(false);
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
    models: catalog.value.models.map((row) => ({
      ...row,
      exists:
        existing.get(modelIds.get(row.model.modelName) ?? -1)?.has(row.model.modelName) ?? false,
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
  modelPanel.value = "selection";
  step.value = "models";
}
function addManual() {
  s.addManual();
  modelPanel.value = "configuration";
}
watch(step, (value) => {
  if (value === "models" && lockedModel.value) modelPanel.value = "configuration";
});
watch([step, modelMode], () => {
  if (
    (step.value === "provider" || (step.value === "models" && modelMode.value === "catalog")) &&
    !catalog.value &&
    !s.catalogCall.loading.value
  )
    s.loadCatalog();
});
watch(fieldErrors, async (errors) => {
  if (Object.keys(errors).length && step.value === "models") {
    if (step.value === "models") modelPanel.value = "configuration";
    await nextTick();
    const invalid = document.querySelector<HTMLElement>('#setup-content [aria-invalid="true"]');
    focusInScrollArea(invalid);
  }
});
onMounted(async () => {
  await s.load();
  if (!contextError.value && !catalog.value) await s.loadCatalog();
});
</script>
<template>
  <PageShell id="setup-content" :scrollable="false">
    <template #header>
      <SectionHeader
        :title="savedProvider ? `接入模型 / ${savedProvider.displayName}` : '接入模型'"
        :subtitle="
          lockedModel ? `当前模型：${lockedModel.modelName}（已锁定，复用本地定义）` : undefined
        "
        :icon="Cable"
      >
        <template #actions
          ><Button variant="outline" :disabled="saving" @click="exit"
            ><ArrowLeft aria-hidden="true" />返回入口</Button
          ></template
        >
      </SectionHeader>
    </template>
    <template v-if="!short || step !== 'models'" #toolbar>
      <ol
        v-if="!short || step !== 'models'"
        class="setup-steps grid grid-cols-4 gap-2 border-b pb-2 text-xs sm:text-sm"
      >
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
      </ol></template
    >
    <div
      v-if="contextError"
      data-scroll-area
      role="alert"
      class="min-h-0 space-y-3 overflow-auto rounded border border-destructive p-6"
    >
      <p>{{ contextError }}</p>
      <RouterLink to="/providers" class="text-primary underline">返回提供者</RouterLink>
    </div>
    <div v-if="loading" role="status" class="shrink-0 text-sm">正在加载本地记录...</div>
    <template v-if="!contextError">
      <div v-if="step === 'provider'" class="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
        <div class="flex shrink-0 flex-wrap gap-2">
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
          v-show="providerMode === 'catalog'"
          fill
          v-model:query="providerQuery"
          v-model:page="providerPage"
          :preview="catalog"
          :loading="s.catalogCall.loading.value"
          :error="s.catalogCall.error.value"
          :error-detail="s.catalogCall.errorDetail.value"
          kind="providers"
          :selected-keys="selectedTemplate ? [selectedTemplate] : []"
          @select="(keys) => (selectedTemplate = keys[0] ?? '')"
          @retry="s.loadCatalog"
          @refresh="s.loadCatalog"
          @manual="choose(null)"
        />
        <div
          v-if="providerMode === 'existing'"
          data-scroll-area
          tabindex="0"
          class="min-h-0 flex-1 space-y-3 overflow-y-auto overscroll-contain p-1"
        >
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
      <ProviderForm
        v-if="step === 'configuration'"
        fill
        v-model="providerDraft"
        :is-edit="!!savedProvider"
        :saving="saving || loading"
        :blocked="uncertain"
        submit-label="保存提供者并选择模型"
        :error="error"
        :field-errors="fieldErrors"
        @submit="s.saveProvider()"
        @cancel="exit"
      >
        <template #before-fields>
          <p v-if="savedProvider" class="rounded border p-3">
            提供者已保存；此处修改将显式更新本地配置。
          </p>
          <p
            v-else-if="
              referenceProvider && providers.some((p) => p.providerId === referenceProvider)
            "
            class="rounded border p-3 text-warning"
          >
            新建独立实例：请输入新的实例 ID（例如
            {{ referenceProvider }}-backup），现有实例不会被覆盖。
          </p>
          <div v-if="uncertain" role="status" class="space-y-3 rounded border border-warning p-4">
            <p>提供者保存结果待确认</p>
            <template v-if="providerRecovery">
              <p>
                找到本地实例：{{ providerRecovery.displayName }} · {{ providerRecovery.providerId }}
              </p>
              <p>
                {{ providerRecovery.enabled ? "已启用" : "已停用" }} ·
                {{ providerRecovery.apiKeys.length }} 个 Key ·
                {{ providerRecovery.modelCount }} 个连接
              </p>
              <p
                v-for="protocol in providerRecovery.protocols"
                :key="protocol.id"
                class="break-all"
              >
                {{ protocol.protocol }} · {{ protocol.baseUrl }}
              </p>
              <p>采用本地配置将替换当前提供者草稿，不会再次写入；模型连接草稿保留。</p>
            </template>
            <p v-else-if="providerRecoveryChecked">
              {{
                savedProvider
                  ? "原提供者已不存在，请返回提供者列表。"
                  : "未找到此实例 ID 的本地记录，可以确认后重试。"
              }}
            </p>
            <div class="flex flex-wrap gap-3">
              <Button
                type="button"
                variant="outline"
                :disabled="saving || loading"
                @click="s.reloadLocalRecords"
                >重新加载本地记录</Button
              >
              <Button
                type="button"
                v-if="providerRecovery"
                :disabled="saving || loading"
                @click="s.useRecoveredProvider"
                >采用本地配置</Button
              >
              <Button
                type="button"
                v-else-if="providerRecoveryChecked && !savedProvider"
                variant="outline"
                :disabled="saving || loading"
                @click="s.allowProviderRetry"
                >确认未保存，允许重试</Button
              >
            </div>
          </div> </template
        ><template #actions
          ><Button
            variant="outline"
            type="button"
            :disabled="saving || uncertain"
            @click="s.saveProvider(false)"
            >只保存提供者</Button
          ></template
        ></ProviderForm
      >
      <div
        v-if="step === 'models' && savedProvider"
        class="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden"
      >
        <PageFilters v-if="short" class="shrink-0">
          <template #primary>
            <Select
              :model-value="modelPanel === 'configuration' ? 'configuration' : modelMode"
              @update:model-value="
                (value) => {
                  if (value === 'configuration') modelPanel = 'configuration';
                  else {
                    modelMode = value as typeof modelMode;
                    modelPanel = 'selection';
                  }
                }
              "
            >
              <SelectTrigger aria-label="接入工作区"><SelectValue /></SelectTrigger>
              <SelectContent
                ><SelectItem value="catalog">目录模型</SelectItem
                ><SelectItem value="local">本地模型</SelectItem
                ><SelectItem value="manual">手动新建</SelectItem
                ><SelectItem value="configuration"
                  >配置连接（{{ selectedConnections.length }}）</SelectItem
                ></SelectContent
              >
            </Select>
          </template>
          <Button variant="outline" :disabled="saving || uncertain" @click="step = 'configuration'"
            >编辑连接设置</Button
          >
          <Button variant="outline" :disabled="saving" @click="changeProvider">更换提供者</Button>
          <details v-if="s.localLinks.value.length" class="w-full">
            <summary>已配置 {{ s.localLinks.value.length }} 个连接</summary>
            <p v-for="link in s.localLinks.value" :key="link.id" class="break-all">
              {{ link.providerModelId }}
              <RouterLink :to="`/admin/models/${link.modelId}`" class="text-primary underline"
                >查看/编辑连接</RouterLink
              >
            </p>
          </details>
        </PageFilters>
        <div v-else class="flex shrink-0 flex-wrap gap-2">
          <Button variant="outline" :disabled="saving || uncertain" @click="step = 'configuration'"
            >编辑连接设置</Button
          ><Button
            variant="ghost"
            :disabled="saving"
            @click="changeProvider"
            title="更换提供者，已保存记录保留"
            >更换提供者</Button
          >
        </div>
        <TabsRoot
          v-model="modelPanel"
          :unmount-on-hide="false"
          class="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden"
        >
          <TabsList
            v-if="!short"
            aria-label="模型接入工作区"
            class="grid shrink-0 grid-cols-2 border-b lg:hidden"
          >
            <TabsTrigger
              value="selection"
              :disabled="saving"
              class="min-h-11 border-b-2 border-transparent px-2 text-sm data-[state=active]:border-primary data-[state=active]:font-semibold data-[state=active]:text-primary"
              >选择模型</TabsTrigger
            >
            <TabsTrigger
              value="configuration"
              :disabled="saving"
              class="min-h-11 border-b-2 border-transparent px-2 text-sm data-[state=active]:border-primary data-[state=active]:font-semibold data-[state=active]:text-primary"
              >配置连接（{{ selectedConnections.length }}）</TabsTrigger
            >
          </TabsList>
          <fieldset
            :disabled="saving || loading"
            class="grid min-h-0 min-w-0 flex-1 grid-rows-[minmax(0,1fr)] gap-4 overflow-hidden lg:grid-cols-2"
          >
            <TabsContent
              value="selection"
              :force-mount="wide"
              as="section"
              class="flex min-h-0 min-w-0 flex-col gap-2 overflow-hidden"
            >
              <div v-if="!short" class="flex shrink-0 flex-wrap gap-2">
                <Button
                  type="button"
                  v-for="mode in ['catalog', 'local', 'manual'] as const"
                  :key="mode"
                  :variant="modelMode === mode ? 'default' : 'outline'"
                  @click="modelMode = mode"
                  >{{
                    mode === "catalog" ? "目录模型" : mode === "local" ? "本地模型" : "手动新建"
                  }}</Button
                >
              </div>
              <CatalogPicker
                v-show="modelMode === 'catalog'"
                fill
                v-model:query="modelQuery"
                v-model:page="catalogPage"
                :preview="targetCatalog"
                :loading="s.catalogCall.loading.value"
                :error="s.catalogCall.error.value"
                :error-detail="s.catalogCall.errorDetail.value"
                kind="models"
                :model-key="lockedModel?.modelName"
                :selected-keys="
                  selectedConnections
                    .filter((item) => item.key.startsWith('catalog:'))
                    .map((item) => item.modelKey)
                "
                multiple
                @select="s.selectCatalogKeys"
                @retry="s.loadCatalog"
                @refresh="s.loadCatalog"
                @manual="modelMode = 'manual'"
              />
              <template v-if="modelMode === 'local'"
                ><Input
                  v-model="localSearch"
                  placeholder="搜索本地模型"
                  aria-label="搜索本地模型"
                  @update:model-value="localPage = 1"
                />
                <div
                  data-scroll-area
                  tabindex="0"
                  class="min-h-0 flex-1 space-y-2 overflow-y-auto overscroll-contain p-1"
                >
                  <button
                    v-for="model in localCandidates.slice((localPage - 1) * 50, localPage * 50)"
                    :key="model.id"
                    class="min-h-14 w-full rounded border bg-card p-3 text-left break-all"
                    @click="s.addLocal(model.id)"
                  >
                    {{ model.displayName }} · <code>{{ model.modelName }}</code> · 复用本地定义
                  </button>
                </div>
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
              ><template v-else-if="modelMode === 'manual'"
                ><p>Token 上限为空且必填；能力默认不声明支持，不使用猜测规格。</p>
                <Button @click="addManual">{{
                  lockedModel ? "为此模型添加连接" : "新建手动模型草稿"
                }}</Button></template
              >
              <section
                v-if="!short && s.localLinks.value.length"
                :class="
                  existingOpen
                    ? 'flex min-h-0 flex-1 flex-col overflow-hidden border-t pt-2'
                    : 'shrink-0 border-t pt-2'
                "
              >
                <button
                  type="button"
                  :aria-expanded="existingOpen"
                  class="min-h-9 shrink-0 text-left"
                  @click="existingOpen = !existingOpen"
                >
                  已配置 {{ s.localLinks.value.length }} 个连接
                </button>
                <div
                  v-if="existingOpen"
                  data-scroll-area
                  tabindex="0"
                  class="min-h-0 flex-1 overflow-y-auto overscroll-contain"
                >
                  <p v-for="link in s.localLinks.value" :key="link.id" class="mt-2 break-all">
                    {{ models.find((m) => m.id === link.modelId)?.modelName }} →
                    {{ link.providerModelId }} · 已添加
                    <RouterLink :to="`/admin/models/${link.modelId}`" class="text-primary underline"
                      >查看/编辑连接</RouterLink
                    >
                  </p>
                </div>
              </section>
            </TabsContent>
            <TabsContent
              value="configuration"
              :force-mount="wide"
              as="section"
              class="flex min-h-0 min-w-0 flex-col gap-2 overflow-hidden lg:border-l lg:pl-4"
            >
              <h2 class="shrink-0">已选项配置</h2>
              <div
                data-scroll-area
                class="flex max-h-[20%] shrink-0 flex-wrap gap-2 overflow-y-auto overscroll-contain p-1"
              >
                <Button
                  v-for="item in selectedConnections"
                  :key="item.key"
                  class="h-auto min-h-9 max-w-full text-left break-all whitespace-normal"
                  :variant="activeKey === item.key ? 'default' : 'outline'"
                  @click="activeKey = item.key"
                  >{{ item.link.providerModelId || "新连接" }}</Button
                >
              </div>
              <div
                data-scroll-area
                tabindex="0"
                class="min-h-0 flex-1 space-y-4 overflow-y-auto overscroll-contain p-1"
              >
                <p v-if="!savedProvider.apiKeys.length" class="text-warning">
                  已保存，但尚未配置 API Key，不能完成上游测试。
                </p>
                <p v-if="!active">尚未选择连接</p>
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
                      {{ models.find((m) => m.id === active!.existingModelId)?.maxInputTokens }} /
                      输出
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
                  <p class="text-sm text-muted-foreground">
                    请确认上游模型 ID 与当前提供者一致；价格留空表示未知，能力留空继承模型标称。
                  </p>
                  <ModelLinkFields
                    v-model="active.link"
                    :provider="savedProvider"
                    :field-errors="activeErrors"
                    :saving="saving"
                  />
                </template>
                <div v-if="conflictingModel" class="space-y-3 rounded border border-warning p-4">
                  <h2>同名本地模型已存在</h2>
                  <p>
                    {{ conflictingModel.modelName }} · {{ conflictingModel.displayName }} · 输入
                    {{ conflictingModel.maxInputTokens }} / 输出
                    {{ conflictingModel.maxOutputTokens }}
                  </p>
                  <p>
                    当前草稿：{{ modelDrafts[s.conflictKey.value]?.displayName }} · 输入
                    {{ modelDrafts[s.conflictKey.value]?.maxInput }} / 输出
                    {{ modelDrafts[s.conflictKey.value]?.maxOutput }}
                  </p>
                  <Button @click="s.reuseConflict">复用本地定义继续</Button
                  ><Button variant="outline" @click="conflictingModel = null"
                    >改用其他模型 ID</Button
                  >
                </div>
                <p
                  v-if="error"
                  role="alert"
                  class="rounded border border-destructive p-4 text-destructive"
                >
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
              </div>
            </TabsContent>
          </fieldset>
        </TabsRoot>
      </div>
      <div
        v-if="step === 'result' && result && savedProvider"
        data-scroll-area
        tabindex="0"
        class="min-h-0 flex-1 space-y-4 overflow-y-auto overscroll-contain p-1"
      >
        <h2>模型连接已保存</h2>
        <p>本批事务已提交；退出不会撤销已保存配置。连接存在不代表上游实时可达。</p>
        <p v-if="error" role="alert" class="text-destructive">{{ error }}</p>
        <ConnectionList :entries="resultEntries" :providers="[savedProvider]" read-only />
        <ClientConnectionInfo :model-name="resultEntries[0]?.modelName" />
      </div>
    </template>
    <template
      v-if="
        !contextError &&
        (step === 'models' ||
          step === 'result' ||
          (step === 'provider' && providerMode === 'catalog' && selectedTemplate))
      "
      #footer
    >
      <div v-if="step === 'provider'" class="flex flex-wrap items-center justify-end gap-2">
        <Button v-if="duplicateProvider" variant="outline" @click="choose(duplicateProvider.id)"
          >使用已有提供者</Button
        >
        <Button @click="choose(selectedTemplate)">{{
          duplicateProvider ? "新建独立实例" : "使用此预设"
        }}</Button>
      </div>
      <div v-if="step === 'models'" class="flex flex-wrap items-center justify-between gap-2">
        <p class="text-sm">
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
          v-if="modelPanel === 'selection'"
          class="lg:hidden"
          :disabled="saving || !selectedConnections.length"
          @click="modelPanel = 'configuration'"
        >
          <SlidersHorizontal />配置连接
        </Button>
        <Button
          :class="modelPanel === 'selection' ? 'hidden lg:inline-flex' : ''"
          :disabled="saving || uncertain || !selectedConnections.length"
          @click="s.submitConnections"
          ><Save />{{ saving ? "保存中…" : `保存 ${selectedConnections.length} 个连接` }}</Button
        >
      </div>
      <div v-if="step === 'result' && savedProvider" class="flex flex-wrap gap-3">
        <RouterLink :to="`/providers/${savedProvider.id}`" class="text-primary underline"
          >查看提供者</RouterLink
        ><Button variant="outline" :disabled="loading" @click="continueAdding">继续添加模型</Button
        ><RouterLink
          :to="`/models?model=${encodeURIComponent(resultEntries[0]?.modelName ?? '')}`"
          class="text-primary underline"
          >查看模型使用方式</RouterLink
        >
      </div>
    </template>
  </PageShell>
</template>
<route lang="json">
{ "meta": { "requiresAdmin": true } }
</route>
