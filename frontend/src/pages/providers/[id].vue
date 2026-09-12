<script setup lang="ts">
import { type ModelLinkView } from "@bindings/ModelLinkView";
import { type ProviderResponse } from "@bindings/ProviderResponse";

import { getApi } from "~/lib/api";
import { providerToDraft, validateProviderDraft } from "~/lib/utils/provider";
import { useConnectionTestsStore } from "~/stores/connection-tests";
const route = useRoute();
const router = useRouter();
const confirm = useConfirm();
const tests = useConnectionTestsStore();
const provider = ref<ProviderResponse | null>(null);
const providers = ref<ProviderResponse[]>([]);
const entries = ref<{ modelId: number; modelName: string; link: ModelLinkView }[]>([]);
const draft = ref(providerToDraft(null));
const baseline = ref("");
const fieldErrors = ref<Record<string, string>>({});
const error = ref("");
const loadCall = useApiCall(async () => {
  const id = Number(route.params.id);
  if (!Number.isSafeInteger(id) || id < 1) throw Error("记录不存在或已删除");
  const [p, all, models, rows] = await Promise.all([
    getApi().admin.getProvider(String(id)),
    getApi().admin.listProviders(),
    getApi().admin.listAdminModels(),
    getApi().admin.listProviderModels(String(id)),
  ]);
  const relevant = models.filter((model) => rows.some((row) => row.modelName === model.modelName));
  const lists = await Promise.all(
    relevant.map((model) => getApi().admin.listModelProviders(String(model.id))),
  );
  return {
    p,
    all,
    entries: lists.flatMap((links, index) =>
      links
        .filter((link) => link.providerId === id)
        .map((link) => ({
          modelId: relevant[index]!.id,
          modelName: relevant[index]!.modelName,
          link,
        })),
    ),
  };
});
const saveCall = useApiCall(() =>
  getApi().admin.updateProvider(String(provider.value!.id), draft.value),
);
const deleteCall = useApiCall(() => getApi().admin.deleteProvider(String(provider.value!.id)));
const busy = computed(() => saveCall.loading.value || deleteCall.loading.value);
const dirty = computed(() => !!provider.value && JSON.stringify(draft.value) !== baseline.value);
const { confirmDiscard } = useUnsavedChanges(dirty, busy);
const tab = computed(() => (route.query.tab === "connection" ? "connection" : "models"));
async function load() {
  const result = await loadCall.execute();
  if (result) {
    provider.value = result.p;
    providers.value = result.all;
    entries.value = result.entries;
    draft.value = providerToDraft(result.p);
    baseline.value = JSON.stringify(draft.value);
  }
}
async function save() {
  if (busy.value || !provider.value) return;
  fieldErrors.value = validateProviderDraft(draft.value, provider.value);
  for (const protocol of provider.value.protocols) {
    const count = entries.value.filter((entry) => entry.link.protocolId === protocol.id).length;
    if (count && !draft.value.protocols.some((p) => p.id === protocol.id))
      fieldErrors.value.protocols = `协议仍有 ${count} 条连接，请先移除或调整连接后再删除协议。`;
  }
  if (Object.keys(fieldErrors.value).length) {
    error.value = fieldErrors.value.protocols ?? "";
    return;
  }
  if (await saveCall.execute()) {
    tests.invalidateProvider(provider.value.id);
    await load();
    error.value = "";
  } else error.value = saveCall.error.value + " " + saveCall.errorDetail.value;
}
async function selectTab(value: string) {
  if (!(await confirmDiscard())) return;
  if (dirty.value && provider.value) draft.value = providerToDraft(provider.value);
  await router.replace({ path: route.path, query: { tab: value } });
}
async function remove() {
  if (
    !provider.value ||
    busy.value ||
    !(await confirm({
      title: "删除提供者？",
      description: `将删除它的协议与 ${entries.value.length} 个连接，但不会删除共享模型定义。此操作不可撤销。`,
      destructive: true,
    }))
  )
    return;
  if (dirty.value && !(await confirmDiscard())) return;
  const id = provider.value.id;
  await deleteCall.execute();
  if (!deleteCall.error.value) {
    tests.invalidateProvider(id);
    baseline.value = JSON.stringify(draft.value);
    await router.push("/providers");
  } else error.value = deleteCall.error.value;
}
watch(() => route.params.id, load, { immediate: true });
onBeforeRouteUpdate(async (to, from) =>
  to.path !== from.path || to.query.tab !== from.query.tab ? await confirmDiscard() : true,
);
</script>
<template>
  <PageShell
    ><ErrorState v-if="loadCall.error.value" :error="loadCall.error.value" @retry="load" /><Skeleton
      v-else-if="loadCall.loading.value"
      class="h-48"
    /><template v-else-if="provider"
      ><div class="flex flex-wrap justify-between gap-4">
        <div>
          <h1>{{ provider.displayName || provider.providerId }}</h1>
          <p class="mt-2 font-mono break-all">{{ provider.providerId }}</p>
          <p class="mt-2">
            {{ provider.enabled ? "✓ 已启用" : "− 已停用" }} ·
            {{
              !provider.protocols.some((p) => p.enabled)
                ? "缺已启用协议"
                : !provider.apiKeys.length
                  ? "缺 API Key"
                  : !entries.length
                    ? "未关联模型"
                    : "配置项齐备（非实时健康状态）"
            }}
          </p>
        </div>
        <div class="flex flex-wrap gap-2">
          <Button as-child
            ><RouterLink :to="`/admin/setup?providerId=${provider.id}`"
              >添加模型</RouterLink
            ></Button
          ><Button variant="outline" @click="selectTab('connection')">编辑配置</Button>
        </div>
      </div>
      <nav class="flex gap-2" aria-label="提供者详情">
        <Button :variant="tab === 'models' ? 'default' : 'outline'" @click="selectTab('models')"
          >模型连接</Button
        ><Button
          :variant="tab === 'connection' ? 'default' : 'outline'"
          @click="selectTab('connection')"
          >连接设置</Button
        >
      </nav>
      <ConnectionList
        v-if="tab === 'models'"
        :entries="entries"
        :providers="providers"
        @changed="load"
      />
      <div v-else class="space-y-6">
        <ProviderForm
          v-model="draft"
          is-edit
          :saving="busy"
          submit-label="保存提供者配置"
          :field-errors="fieldErrors"
          :error="error"
          @submit="save"
          @cancel="selectTab('models')"
        />
        <div class="rounded border p-4">
          <h2>删除提供者</h2>
          <p class="my-3 text-muted-foreground">
            删除协议和 {{ entries.length }} 个连接，不删除共享模型定义。
          </p>
          <Button variant="destructive" :disabled="busy" @click="remove">删除提供者</Button>
        </div>
      </div></template
    ></PageShell
  >
</template>
<route lang="json">
{ "meta": { "requiresAdmin": true } }
</route>
