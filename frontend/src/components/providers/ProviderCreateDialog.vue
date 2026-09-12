<script setup lang="ts">
import { type ProviderResponse } from "@bindings/ProviderResponse";

import ProviderForm from "./ProviderForm.vue";
import { useApiCall } from "~/composables/useApiCall";
import { getApi } from "~/lib/api";
import { providerToDraft, validateProviderDraft } from "~/lib/utils/provider";
import { useConnectionTestsStore } from "~/stores/connection-tests";
const props = defineProps<{ provider?: ProviderResponse | null }>();
const open = defineModel<boolean>("open", { default: false });
const emit = defineEmits<{ created: []; updated: []; error: [message: string] }>();
const draft = ref(providerToDraft(props.provider ?? null));
const baseline = ref(JSON.stringify(draft.value));
const fieldErrors = ref<Record<string, string>>({});
const confirm = useConfirm();
const tests = useConnectionTestsStore();
const save = useApiCall(async () =>
  props.provider
    ? getApi().admin.updateProvider(String(props.provider.id), draft.value)
    : getApi().admin.createProvider(draft.value),
);
watch(
  open,
  (value) => {
    if (value) {
      draft.value = providerToDraft(props.provider ?? null);
      baseline.value = JSON.stringify(draft.value);
      fieldErrors.value = {};
      save.clearError();
    }
  },
  { immediate: true },
);
let closing = false;
async function close() {
  if (save.loading.value || closing) return;
  closing = true;
  try {
    if (
      JSON.stringify(draft.value) !== baseline.value &&
      !(await confirm({ title: "放弃未保存的提供者配置？", description: "已保存记录不受影响。" }))
    )
      return;
    draft.value.apiKeys = [];
    open.value = false;
  } finally {
    closing = false;
  }
}
async function submit() {
  if (save.loading.value) return;
  fieldErrors.value = validateProviderDraft(draft.value, props.provider ?? null);
  if (Object.keys(fieldErrors.value).length) return;
  const saved = await save.execute();
  if (!saved) return;
  tests.invalidateProvider(saved.id);
  draft.value = providerToDraft(saved);
  baseline.value = JSON.stringify(draft.value);
  if (props.provider) emit("updated");
  else emit("created");
  open.value = false;
}
onBeforeRouteLeave(async () => {
  if (!open.value) return;
  await close();
  return !open.value;
});
onBeforeUnmount(() => {
  draft.value.apiKeys = [];
});
</script>
<template>
  <Button v-if="!provider" @click="open = true">添加提供者</Button
  ><Dialog
    :open="open"
    @update:open="
      (value) => {
        if (!value) close();
      }
    "
    ><DialogContent
      class="flex max-h-[90svh] flex-col sm:max-w-3xl"
      @escape-key-down="
        (event) => {
          event.preventDefault();
          close();
        }
      "
      ><DialogHeader
        ><DialogTitle>{{ provider ? "编辑提供者" : "添加提供者" }}</DialogTitle
        ><DialogDescription
          >一次保存全部可见配置；目录不会覆盖本地记录。</DialogDescription
        ></DialogHeader
      >
      <div class="min-h-0 overflow-y-auto">
        <ProviderForm
          v-model="draft"
          :is-edit="!!provider"
          :saving="save.loading.value"
          submit-label="保存提供者"
          :error="save.error.value"
          :field-errors="fieldErrors"
          @submit="submit"
          @cancel="close"
        /></div></DialogContent
  ></Dialog>
</template>
