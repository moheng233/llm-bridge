<script setup lang="ts">
import { type AdminModelResponse } from "@bindings/AdminModelResponse";
import { type ModelLinkView } from "@bindings/ModelLinkView";
import { type ProviderResponse } from "@bindings/ProviderResponse";
import { Save } from "@lucide/vue";

import ModelLinkFields from "./ModelLinkFields.vue";
import { getApi } from "~/lib/api";
import {
  connectionToDraft,
  connectionDraftToInput,
  validateConnectionDraft,
} from "~/lib/connection-draft";
import { focusInScrollArea } from "~/lib/utils";
import { useConnectionTestsStore } from "~/stores/connection-tests";
const props = defineProps<{
  modelId: number;
  modelName: string;
  providers: ProviderResponse[];
  currentModel?: AdminModelResponse | null;
  editingLink: ModelLinkView | null;
}>();
const open = defineModel<boolean>("open", { default: false });
const emit = defineEmits<{ saved: []; error: [message: string] }>();
const draft = ref(props.editingLink ? connectionToDraft(props.editingLink) : null);
const baseline = ref(JSON.stringify(draft.value));
const fieldErrors = ref<Record<string, string>>({});
const provider = computed(() => props.providers.find((p) => p.id === draft.value?.providerId));
const confirm = useConfirm();
const tests = useConnectionTestsStore();
const save = useApiCall(async () =>
  getApi().admin.updateModelProvider(
    String(props.modelId),
    String(props.editingLink!.id),
    connectionDraftToInput(draft.value!),
  ),
);
watch(open, (value) => {
  if (value && props.editingLink) {
    draft.value = connectionToDraft(props.editingLink);
    baseline.value = JSON.stringify(draft.value);
    fieldErrors.value = {};
    save.clearError();
  }
});
let closing = false;
async function close() {
  if (save.loading.value || closing) return;
  closing = true;
  try {
    if (
      JSON.stringify(draft.value) !== baseline.value &&
      !(await confirm({
        title: "放弃未保存的修改？",
        description: "尚未保存的连接修改将丢失。",
        confirmText: "放弃修改",
      }))
    )
      return;
    open.value = false;
  } finally {
    closing = false;
  }
}
async function submit() {
  if (!draft.value || !provider.value || !props.editingLink || save.loading.value) return;
  fieldErrors.value = validateConnectionDraft(draft.value, provider.value);
  if (Object.keys(fieldErrors.value).length) {
    await nextTick();
    focusInScrollArea(
      document.querySelector<HTMLElement>('[data-slot="sheet-content"] [aria-invalid="true"]'),
    );
    return;
  }
  if (await save.execute()) {
    tests.invalidateLink(props.editingLink.id);
    baseline.value = JSON.stringify(draft.value);
    emit("saved");
    open.value = false;
  }
}
onBeforeRouteLeave(async () => {
  if (!open.value) return;
  await close();
  return !open.value;
});
</script>
<template>
  <Sheet
    :open="open"
    @update:open="
      (value) => {
        if (!value) close();
      }
    "
    ><SheetContent
      class="w-full max-w-full gap-0 sm:max-w-2xl"
      @escape-key-down="
        (event) => {
          event.preventDefault();
          close();
        }
      "
      ><SheetHeader class="shrink-0 border-b p-4 pr-12"
        ><SheetTitle>编辑连接 · {{ modelName }}</SheetTitle
        ><SheetDescription>只修改此连接，不覆盖共享模型定义。</SheetDescription></SheetHeader
      >
      <form class="flex min-h-0 flex-1 flex-col" @submit.prevent="submit">
        <div data-scroll-area class="min-h-0 flex-1 overflow-y-auto overscroll-contain p-4">
          <ModelLinkFields
            v-if="draft && provider"
            v-model="draft"
            :provider="provider"
            :field-errors="fieldErrors"
            :saving="save.loading.value"
          />
          <p v-else role="alert">连接或提供者不存在，请重新加载本地记录。</p>
          <ErrorState v-if="save.error.value" :error="save.error.value" class="mt-4" inline />
        </div>
        <div class="flex shrink-0 justify-end gap-2 border-t p-4">
          <Button type="button" variant="outline" :disabled="save.loading.value" @click="close"
            >取消</Button
          ><Button type="submit" :disabled="save.loading.value || !draft || !provider"
            ><Save aria-hidden="true" />{{ save.loading.value ? "保存中…" : "保存修改" }}</Button
          >
        </div>
      </form></SheetContent
    ></Sheet
  >
</template>
