<script setup lang="ts">
import { MODEL_STATUS_OPTIONS } from "~/lib/constants";
import { type ModelDraft } from "~/lib/model-form";
const draft = defineModel<ModelDraft>({ required: true });
withDefaults(
  defineProps<{ isEdit?: boolean; saving?: boolean; fieldErrors?: Record<string, string> }>(),
  { isEdit: false, saving: false, fieldErrors: () => ({}) },
);
const fields = [
  { key: "modelName", label: "网关模型 ID" },
  { key: "displayName", label: "显示名称" },
  { key: "description", label: "描述" },
  { key: "maxInput", label: "最大输入 Token" },
  { key: "maxOutput", label: "最大输出 Token" },
] as const;
const capabilities = [
  { key: "toolCalling", label: "工具调用" },
  { key: "vision", label: "视觉" },
  { key: "thinking", label: "推理" },
  { key: "adaptiveThinking", label: "自适应推理" },
] as const;
</script>
<template>
  <fieldset :disabled="saving" class="min-w-0 space-y-4">
    <p class="text-sm text-muted-foreground">
      标称信息由所有连接共享。创建后网关模型 ID 不可修改；生命周期不表示上游连通性。
    </p>
    <div class="grid gap-4 sm:grid-cols-2">
      <div
        v-for="field in fields"
        :key="field.key"
        class="space-y-2"
        :class="field.key === 'description' ? 'sm:col-span-2' : ''"
      >
        <Label :for="`model-${field.key}`">{{ field.label }}</Label
        ><Input
          :id="`model-${field.key}`"
          v-model="draft[field.key]"
          :readonly="isEdit && field.key === 'modelName'"
          :aria-invalid="!!fieldErrors[field.key]"
          :class="field.key === 'modelName' ? 'font-mono' : ''"
          :placeholder="field.key.startsWith('max') ? '必填，精确整数或 K / M 单位' : ''"
        />
        <p v-if="fieldErrors[field.key]" class="text-sm text-destructive">
          {{ fieldErrors[field.key] }}
        </p>
      </div>
    </div>
    <div class="space-y-2">
      <Label>生命周期状态</Label
      ><Select
        :model-value="draft.status || 'unspecified'"
        @update:model-value="
          (value) => (draft.status = value === 'unspecified' ? '' : String(value))
        "
        ><SelectTrigger><SelectValue /></SelectTrigger
        ><SelectContent
          ><SelectItem value="unspecified">未声明</SelectItem
          ><SelectItem
            v-for="option in MODEL_STATUS_OPTIONS"
            :key="option.value"
            :value="option.value"
            >{{ option.label }}</SelectItem
          ></SelectContent
        ></Select
      >
    </div>
    <div class="flex flex-wrap gap-4">
      <label
        v-for="capability in capabilities"
        :key="capability.key"
        class="flex min-h-9 items-center gap-2"
        ><Checkbox v-model="draft[capability.key]" />声明支持{{ capability.label }}</label
      >
    </div>
  </fieldset>
</template>
