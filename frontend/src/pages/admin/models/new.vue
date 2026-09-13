<script setup lang="ts">
import { Layers, Save } from "@lucide/vue";
import { useMediaQuery } from "@vueuse/core";
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from "reka-ui";

import { getApi } from "~/lib/api";
import { modelToDraft, modelDraftToInput, validateModelDraft } from "~/lib/model-form";
import { type ModelDraft } from "~/lib/model-form";
import { focusInScrollArea } from "~/lib/utils";
const router = useRouter();
const preview = useApiCall(() => getApi().modelsImport.preview());
const local = useApiCall(() => getApi().admin.listAdminModels());
const mode = ref<"catalog" | "manual">("catalog");
const panel = ref("selection");
const wide = useMediaQuery("(min-width: 1024px)");
watch(mode, (value) => {
  if (value === "manual") panel.value = "configuration";
});
const selected = ref("");
const existingId = ref<number | null>(null);
const draft = ref<ModelDraft>({
  modelName: "",
  displayName: "",
  description: "",
  maxInput: "",
  maxOutput: "",
  status: "",
  toolCalling: false,
  vision: false,
  thinking: false,
  adaptiveThinking: false,
});
const baseline = JSON.stringify(draft.value);
const fieldErrors = ref<Record<string, string>>({});
const created = ref(false);
const save = useApiCall(() => getApi().admin.createAdminModel(modelDraftToInput(draft.value)));
const dirty = computed(() => !created.value && JSON.stringify(draft.value) !== baseline);
useUnsavedChanges(dirty, save.loading);
function pick(keys: string[]) {
  selected.value = keys[0] ?? "";
  const model = preview.data.value?.models.find((row) => row.model.modelName === selected.value);
  if (model) {
    existingId.value =
      local.data.value?.find((row) => row.modelName === selected.value)?.id ?? null;
    if (!existingId.value) draft.value = modelToDraft(model.model);
  }
}
async function submit() {
  if (save.loading.value) return;
  fieldErrors.value = validateModelDraft(draft.value);
  if (Object.keys(fieldErrors.value).length) {
    panel.value = "configuration";
    await nextTick();
    focusInScrollArea(document.querySelector<HTMLElement>('[aria-invalid="true"]'));
    return;
  }
  const model = await save.execute();
  if (model) {
    created.value = true;
    await router.push(`/admin/models/${model.id}?created=1`);
  } else {
    await local.execute();
    existingId.value =
      local.data.value?.find((row) => row.modelName === draft.value.modelName)?.id ?? null;
  }
}
onMounted(() => Promise.all([preview.execute(), local.execute()]));
</script>
<template>
  <PageShell :scrollable="false">
    <template #header
      ><SectionHeader
        title="添加模型定义"
        :icon="Layers"
        back-to="/admin/models"
        back-label="返回模型定义列表"
    /></template>
    <template #toolbar>
      <div class="flex gap-2">
        <Button
          :aria-pressed="mode === 'catalog'"
          :variant="mode === 'catalog' ? 'default' : 'outline'"
          @click="mode = 'catalog'"
          >从目录选择</Button
        ><Button
          :aria-pressed="mode === 'manual'"
          :variant="mode === 'manual' ? 'default' : 'outline'"
          @click="
            mode = 'manual';
            existingId = null;
          "
          >手动新建</Button
        >
      </div>
    </template>
    <TabsRoot
      v-model="panel"
      :unmount-on-hide="false"
      class="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden"
    >
      <TabsList class="grid shrink-0 grid-cols-2 border-b lg:hidden" aria-label="新建模型工作区">
        <TabsTrigger
          value="selection"
          class="min-h-11 border-b-2 border-transparent data-[state=active]:border-primary"
          >选择模型</TabsTrigger
        >
        <TabsTrigger
          value="configuration"
          class="min-h-11 border-b-2 border-transparent data-[state=active]:border-primary"
          >模型定义</TabsTrigger
        >
      </TabsList>
      <div
        class="grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)] gap-4 overflow-hidden lg:grid-cols-2"
      >
        <TabsContent
          value="selection"
          :force-mount="wide"
          class="flex min-h-0 flex-col overflow-hidden"
        >
          <CatalogPicker
            v-show="mode === 'catalog'"
            fill
            :preview="preview.data.value"
            :loading="preview.loading.value"
            :error="preview.error.value"
            :error-detail="preview.errorDetail.value"
            kind="models"
            :selected-keys="selected ? [selected] : []"
            @select="pick"
            @retry="preview.execute"
            @refresh="preview.execute"
            @manual="mode = 'manual'"
          />
          <p v-if="mode === 'manual'" class="text-muted-foreground">手动模型定义</p>
        </TabsContent>
        <TabsContent
          value="configuration"
          :force-mount="wide"
          class="flex min-h-0 flex-col overflow-hidden"
        >
          <div
            data-scroll-area
            tabindex="0"
            class="min-h-0 flex-1 space-y-4 overflow-auto overscroll-contain p-1"
          >
            <p class="text-muted-foreground">
              只创建共享标称定义，不隐式创建提供者。目录只用于首次预填。
            </p>
            <div v-if="existingId" class="space-y-3 rounded border p-4">
              <p>本地已有此模型定义，不会用目录覆盖它。</p>
              <RouterLink :to="`/admin/models/${existingId}`" class="text-primary underline"
                >查看已有模型</RouterLink
              ><Button
                variant="outline"
                @click="
                  existingId = null;
                  mode = 'manual';
                "
                >改用其他模型 ID</Button
              >
            </div>
            <form
              id="new-model-form"
              v-else-if="mode === 'manual' || selected"
              class="space-y-4"
              @submit.prevent="submit"
            >
              <ModelForm v-model="draft" :saving="save.loading.value" :field-errors="fieldErrors" />
              <ErrorState
                v-if="save.error.value"
                :error="save.errorDetail.value || save.error.value"
                inline
              />
            </form></div
        ></TabsContent></div
    ></TabsRoot>
    <template #footer
      ><div class="flex justify-end gap-2">
        <Button as-child variant="outline"><RouterLink to="/admin/models">取消</RouterLink></Button
        ><Button
          v-if="!wide && panel === 'selection'"
          :disabled="!selected && mode !== 'manual'"
          @click="panel = 'configuration'"
          >配置模型定义</Button
        >
        <Button
          v-else
          type="submit"
          form="new-model-form"
          :disabled="save.loading.value || !!existingId || (mode !== 'manual' && !selected)"
          ><Save aria-hidden="true" />{{ save.loading.value ? "保存中…" : "添加模型定义" }}</Button
        >
      </div>
    </template></PageShell
  >
</template>
<route lang="json">
{ "meta": { "requiresAdmin": true } }
</route>
