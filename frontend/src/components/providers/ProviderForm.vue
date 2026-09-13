<script setup lang="ts">
import { type CreateProviderRequest } from "@bindings/CreateProviderRequest";
import { Plus, Save } from "@lucide/vue";

import ProtocolEditForm from "./ProtocolEditForm.vue";
import {
  QUOTA_ADAPTER_OPTIONS,
  quotaAdapterFromSelect,
  quotaAdapterToSelect,
} from "~/lib/constants";
import { focusInScrollArea } from "~/lib/utils";
import { emptyProtocol } from "~/lib/utils/provider";
const draft = defineModel<CreateProviderRequest>({ required: true });
const props = defineProps<{
  isEdit: boolean;
  saving: boolean;
  blocked?: boolean;
  fill?: boolean;
  submitLabel: string;
  error: string;
  fieldErrors: Record<string, string>;
}>();
const emit = defineEmits<{ submit: []; cancel: [] }>();
const form = ref<HTMLFormElement>();
watch(
  () => props.fieldErrors,
  async (errors) => {
    if (Object.keys(errors).length) {
      await nextTick();
      const invalid = form.value?.querySelector<HTMLElement>('[aria-invalid="true"]');
      focusInScrollArea(invalid);
    }
  },
);
function addKey() {
  let index = 1;
  while (draft.value.apiKeys.some((k) => k.label === `key-${index}`)) index++;
  draft.value.apiKeys.push({ label: `key-${index}`, weight: 1, key: "" });
}
</script>
<template>
  <form
    ref="form"
    :class="fill ? 'flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden' : 'space-y-6'"
    @submit.prevent="emit('submit')"
  >
    <div
      :data-scroll-area="fill ? '' : undefined"
      :class="
        fill ? 'min-h-0 flex-1 space-y-4 overflow-y-auto overscroll-contain p-1' : 'space-y-4'
      "
    >
      <slot name="before-fields" />
      <fieldset :disabled="saving || blocked" class="min-w-0 space-y-6">
        <div class="grid gap-4 sm:grid-cols-2">
          <div
            v-for="field in ['displayName', 'providerId'] as const"
            :key="field"
            class="space-y-2"
          >
            <Label :for="field">{{ field === "providerId" ? "实例 ID" : "显示名称" }}</Label
            ><Input
              :id="field"
              v-model="draft[field]"
              :readonly="field === 'providerId' && isEdit"
              :aria-invalid="!!fieldErrors[field]"
              :class="field === 'providerId' ? 'font-mono' : ''"
            />
            <p v-if="fieldErrors[field]" class="text-sm text-destructive">
              {{ fieldErrors[field] }}
            </p>
          </div>
        </div>
        <p v-if="isEdit" class="text-sm text-muted-foreground">
          同 label 的已有 Key 留空表示保留原值；这里只显示空输入，不回传掩码。删除条目将移除该 Key。
        </p>
        <section class="space-y-4">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <h2>上游 API Key</h2>
            <Button type="button" variant="outline" @click="addKey">添加 Key</Button>
          </div>
          <p v-if="!draft.apiKeys.length" class="rounded border p-3 text-warning">
            尚未配置 API Key，不能完成上游测试；可以只保存提供者。
          </p>
          <div
            v-for="(key, index) in draft.apiKeys"
            :key="index"
            class="space-y-3 rounded border p-4"
          >
            <Label :for="`apiKeys.${index}.key`">{{
              index === 0 ? "主 API Key" : `API Key ${index + 1}`
            }}</Label
            ><Input
              :id="`apiKeys.${index}.key`"
              v-model="key.key"
              type="password"
              autocomplete="new-password"
              :placeholder="isEdit ? '已有同 label Key 留空保留' : '填写上游 Key'"
              :aria-invalid="!!fieldErrors[`apiKeys.${index}.key`]"
            />
            <p class="text-sm text-destructive">{{ fieldErrors[`apiKeys.${index}.key`] }}</p>
            <details>
              <summary class="cursor-pointer py-2">Key label 与调度权重</summary>
              <div class="grid gap-3 sm:grid-cols-2">
                <div v-for="field in ['label', 'weight'] as const" :key="field" class="space-y-2">
                  <Label :for="`apiKeys.${index}.${field}`">{{
                    field === "label" ? "label" : "权重（正整数）"
                  }}</Label
                  ><Input
                    v-if="field === 'label'"
                    :id="`apiKeys.${index}.${field}`"
                    v-model="key.label"
                    :aria-invalid="!!fieldErrors[`apiKeys.${index}.${field}`]"
                  /><Input
                    v-else
                    :id="`apiKeys.${index}.${field}`"
                    v-model.number="key.weight"
                    type="number"
                    min="1"
                    :aria-invalid="!!fieldErrors[`apiKeys.${index}.${field}`]"
                  />
                  <p class="text-sm text-destructive">
                    {{ fieldErrors[`apiKeys.${index}.${field}`] }}
                  </p>
                </div>
              </div>
            </details>
            <Button type="button" variant="ghost" @click="draft.apiKeys.splice(index, 1)"
              >移除此 Key 条目</Button
            >
          </div>
        </section>
        <section
          tabindex="-1"
          :aria-invalid="!!fieldErrors.protocols"
          :aria-describedby="fieldErrors.protocols ? 'provider-protocols-error' : undefined"
          class="space-y-4"
        >
          <div class="flex flex-wrap items-center justify-between gap-2">
            <h2>连接协议</h2>
            <Button type="button" variant="outline" @click="draft.protocols.push(emptyProtocol())"
              ><Plus />添加协议</Button
            >
          </div>
          <p
            v-if="fieldErrors.protocols"
            id="provider-protocols-error"
            role="alert"
            class="text-sm text-destructive"
          >
            {{ fieldErrors.protocols }}
          </p>
          <p v-if="!draft.protocols.length" class="text-warning">
            缺少协议；可只保存提供者，添加模型前必须补齐。
          </p>
          <div
            v-for="(protocol, index) in draft.protocols"
            :key="protocol.id ?? `new-${index}`"
            class="space-y-3 rounded border p-4"
          >
            <ProtocolEditForm
              v-model="draft.protocols[index]!"
              :prefix="`protocols.${index}`"
              :field-errors="fieldErrors"
            /><Button type="button" variant="ghost" @click="draft.protocols.splice(index, 1)"
              >移除此协议</Button
            >
          </div>
        </section>
        <details class="space-y-4 rounded border p-4">
          <summary class="cursor-pointer">高级配置</summary>
          <div class="space-y-2">
            <Label for="priority">提供者优先级（跨提供者，数字越小越优先）</Label
            ><Input
              id="priority"
              v-model.number="draft.priority"
              type="number"
              min="0"
              :aria-invalid="!!fieldErrors.priority"
            />
            <p class="text-sm text-destructive">{{ fieldErrors.priority }}</p>
          </div>
          <div class="space-y-2">
            <Label>额度适配器</Label
            ><Select
              :model-value="quotaAdapterToSelect(draft.quotaAdapter)"
              @update:model-value="
                (value) => (draft.quotaAdapter = quotaAdapterFromSelect(String(value)))
              "
              ><SelectTrigger><SelectValue /></SelectTrigger
              ><SelectContent
                ><SelectItem
                  v-for="option in QUOTA_ADAPTER_OPTIONS"
                  :key="option.value"
                  :value="option.value"
                  >{{ option.label }}</SelectItem
                ></SelectContent
              ></Select
            >
          </div>
          <div class="space-y-2">
            <Label for="quotaAdapterConfig">额度适配器配置 JSON（可选）</Label
            ><Input
              id="quotaAdapterConfig"
              :model-value="draft.quotaAdapterConfig ?? ''"
              @update:model-value="(value) => (draft.quotaAdapterConfig = String(value) || null)"
              :aria-invalid="!!fieldErrors.quotaAdapterConfig"
            />
            <p class="text-sm text-destructive">{{ fieldErrors.quotaAdapterConfig }}</p>
          </div>
          <label class="flex min-h-9 items-center gap-2"
            ><Checkbox v-model="draft.enabled" />启用提供者</label
          >
        </details>
      </fieldset>
      <ErrorState v-if="error" :error="error" inline />
      <slot name="after-fields" />
    </div>
    <div class="flex shrink-0 flex-wrap justify-end gap-2 border-t bg-background pt-3">
      <slot name="actions" /><Button
        type="button"
        variant="outline"
        :disabled="saving"
        @click="emit('cancel')"
        >取消</Button
      ><Button type="submit" :disabled="saving || blocked"
        ><Save aria-hidden="true" />{{ saving ? "保存中…" : submitLabel }}</Button
      >
    </div>
  </form>
</template>
