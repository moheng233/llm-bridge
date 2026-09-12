<script setup lang="ts">
import { type AdminModelResponse } from "@bindings/AdminModelResponse";
import { type ModelLinkView } from "@bindings/ModelLinkView";
import { type ProviderResponse } from "@bindings/ProviderResponse";

import { getApi } from "~/lib/api";
import {
  modelToDraft,
  modelDraftToInput,
  validateModelDraft,
  type ModelDraft,
} from "~/lib/model-form";
import { useConnectionTestsStore } from "~/stores/connection-tests";
const route = useRoute();
const router = useRouter();
const confirm = useConfirm();
const tests = useConnectionTestsStore();
const model = ref<AdminModelResponse | null>(null);
const links = ref<ModelLinkView[]>([]);
const providers = ref<ProviderResponse[]>([]);
const draft = ref<ModelDraft | null>(null);
const baseline = ref("");
const fieldErrors = ref<Record<string, string>>({});
const error = ref("");
const loadCall = useApiCall(async () => {
  const id = Number(route.params.id);
  if (!Number.isSafeInteger(id) || id < 1) throw Error("记录不存在或已删除");
  return Promise.all([
    getApi().admin.getAdminModel(String(id)),
    getApi().admin.listModelProviders(String(id)),
    getApi().admin.listProviders(),
  ]);
});
const saveCall = useApiCall(() =>
  getApi().admin.updateAdminModel(String(model.value!.id), modelDraftToInput(draft.value!)),
);
const deleteCall = useApiCall(() => getApi().admin.deleteAdminModel(String(model.value!.id)));
const busy = computed(() => saveCall.loading.value || deleteCall.loading.value);
const dirty = computed(() => !!draft.value && JSON.stringify(draft.value) !== baseline.value);
const { confirmDiscard } = useUnsavedChanges(dirty, busy);
const tab = computed(() => (route.query.tab === "definition" ? "definition" : "connections"));
const entries = computed(() =>
  links.value.map((link) => ({
    modelId: model.value!.id,
    modelName: model.value!.modelName,
    link,
  })),
);
async function load() {
  const data = await loadCall.execute();
  if (data) {
    [model.value, links.value, providers.value] = data;
    draft.value = modelToDraft(model.value);
    baseline.value = JSON.stringify(draft.value);
  }
}
async function save() {
  if (!model.value || !draft.value || busy.value) return;
  fieldErrors.value = validateModelDraft(draft.value);
  if (Object.keys(fieldErrors.value).length) {
    await nextTick();
    document.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
    return;
  }
  const input = modelDraftToInput(draft.value);
  const changed = (
    [
      "maxInputTokens",
      "maxOutputTokens",
      "toolCalling",
      "vision",
      "thinking",
      "adaptiveThinking",
    ] as const
  ).filter((field) => input[field] !== model.value![field]);
  const affected = links.value.filter((link) =>
    changed.some((field) => link[field] === null),
  ).length;
  if (
    affected &&
    !(await confirm({
      title: "更新共享标称能力？",
      description: `${affected} 条连接继承了本次修改的字段，将随模型定义变化；显式覆盖值不变。`,
      confirmText: "保存标称信息",
    }))
  )
    return;
  if (await saveCall.execute()) {
    if (changed.length) tests.invalidateModel(model.value.id);
    await load();
    error.value = "";
  } else error.value = saveCall.error.value;
}
async function selectTab(value: string) {
  if (!(await confirmDiscard())) return;
  if (dirty.value && model.value) draft.value = modelToDraft(model.value);
  await router.replace({ path: route.path, query: { tab: value } });
}
async function remove() {
  if (
    !model.value ||
    busy.value ||
    !(await confirm({
      title: "删除模型定义？",
      description: `将移除 ${links.value.length} 条连接，客户端将不能再使用 ${model.value.modelName}。此操作不可撤销。`,
      destructive: true,
    }))
  )
    return;
  const id = model.value.id;
  await deleteCall.execute();
  if (!deleteCall.error.value) {
    tests.invalidateModel(id);
    baseline.value = JSON.stringify(draft.value);
    await router.push("/admin/models");
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
    /><template v-else-if="model && draft"
      ><div class="flex flex-wrap justify-between gap-4">
        <div>
          <h1>{{ model.displayName || model.modelName }}</h1>
          <p class="mt-2 font-mono break-all">{{ model.modelName }}</p>
          <p class="mt-2">
            {{
              links.length
                ? `${links.length} 条连接（非实时健康状态）`
                : "模型定义已保存，尚未关联提供者"
            }}
          </p>
        </div>
        <Button as-child
          ><RouterLink :to="`/admin/setup?modelId=${model.id}`">关联提供者</RouterLink></Button
        >
      </div>
      <nav class="flex gap-2" aria-label="模型定义详情">
        <Button
          :variant="tab === 'connections' ? 'default' : 'outline'"
          @click="selectTab('connections')"
          >提供者连接</Button
        ><Button
          :variant="tab === 'definition' ? 'default' : 'outline'"
          @click="selectTab('definition')"
          >模型定义</Button
        >
      </nav>
      <ConnectionList
        v-if="tab === 'connections'"
        :entries="entries"
        :providers="providers"
        @changed="load"
      />
      <form v-else class="space-y-6" @submit.prevent="save">
        <ModelForm v-model="draft" is-edit :saving="busy" :field-errors="fieldErrors" />
        <p v-if="error" role="alert" class="text-destructive">{{ error }}</p>
        <div class="flex justify-end gap-3">
          <Button type="button" variant="outline" :disabled="busy" @click="selectTab('connections')"
            >取消</Button
          ><Button type="submit" :disabled="busy">{{ busy ? "保存中…" : "保存模型定义" }}</Button>
        </div>
        <div class="rounded border p-4">
          <h2>删除模型定义</h2>
          <p class="my-3 text-muted-foreground">所有连接和客户端对该 ID 的使用都会受到影响。</p>
          <Button type="button" variant="destructive" :disabled="busy" @click="remove"
            >删除模型定义</Button
          >
        </div>
      </form></template
    ></PageShell
  >
</template>
<route lang="json">
{ "meta": { "requiresAdmin": true } }
</route>
