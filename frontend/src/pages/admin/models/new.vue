<script setup lang="ts">
import { getApi } from "~/lib/api";
import { modelToDraft, modelDraftToInput, validateModelDraft } from "~/lib/model-form";
import { type ModelDraft } from "~/lib/model-form";
const router = useRouter();
const preview = useApiCall(() => getApi().modelsImport.preview());
const local = useApiCall(() => getApi().admin.listAdminModels());
const mode = ref<"catalog" | "manual">("catalog");
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
    await nextTick();
    document.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
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
  <PageShell
    ><h1>添加模型定义</h1>
    <p class="text-muted-foreground">只创建共享标称定义，不隐式创建提供者。目录只用于首次预填。</p>
    <div class="flex gap-2">
      <Button :variant="mode === 'catalog' ? 'default' : 'outline'" @click="mode = 'catalog'"
        >从目录选择</Button
      ><Button
        :variant="mode === 'manual' ? 'default' : 'outline'"
        @click="
          mode = 'manual';
          existingId = null;
        "
        >手动新建</Button
      >
    </div>
    <CatalogPicker
      v-if="mode === 'catalog'"
      :preview="preview.data.value"
      :loading="preview.loading.value"
      :error="preview.error.value"
      kind="models"
      :selected-keys="selected ? [selected] : []"
      @select="pick"
      @retry="preview.execute"
      @refresh="preview.execute"
      @manual="mode = 'manual'"
    />
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
      v-else-if="mode === 'manual' || selected"
      class="space-y-4 rounded border bg-card p-4"
      @submit.prevent="submit"
    >
      <ModelForm v-model="draft" :saving="save.loading.value" :field-errors="fieldErrors" />
      <p v-if="save.error.value" role="alert" class="text-destructive">
        {{ save.error.value }} {{ save.errorDetail.value }}
      </p>
      <div class="flex justify-end gap-3">
        <Button as-child variant="outline"><RouterLink to="/admin/models">取消</RouterLink></Button
        ><Button type="submit" :disabled="save.loading.value">{{
          save.loading.value ? "保存中…" : "保存模型定义"
        }}</Button>
      </div>
    </form></PageShell
  >
</template>
<route lang="json">
{ "meta": { "requiresAdmin": true } }
</route>
