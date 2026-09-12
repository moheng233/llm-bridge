<script setup lang="ts">
import { type ProtocolInput } from "@bindings/ProtocolInput";

import { PROTOCOL_OPTIONS } from "~/lib/constants";
const draft = defineModel<ProtocolInput>({ required: true });
const props = withDefaults(
  defineProps<{ prefix?: string; fieldErrors?: Record<string, string> }>(),
  { prefix: "protocol", fieldErrors: () => ({}) },
);
</script>
<template>
  <div class="grid min-w-0 gap-4 sm:grid-cols-2">
    <div class="space-y-2">
      <Label :for="`${prefix}.protocol`">协议类型</Label
      ><Select v-model="draft.protocol"
        ><SelectTrigger :id="`${prefix}.protocol`"><SelectValue /></SelectTrigger
        ><SelectContent
          ><SelectItem
            v-for="option in PROTOCOL_OPTIONS"
            :key="option.value"
            :value="option.value"
            >{{ option.label }}</SelectItem
          ></SelectContent
        ></Select
      >
    </div>
    <div class="space-y-2">
      <Label :for="`${prefix}.baseUrl`">Base URL</Label
      ><Input
        :id="`${prefix}.baseUrl`"
        v-model="draft.baseUrl"
        :aria-invalid="!!fieldErrors[`${prefix}.baseUrl`]"
        class="font-mono"
        placeholder="http://localhost:8000/v1"
      />
      <p v-if="fieldErrors[`${prefix}.baseUrl`]" class="text-sm text-destructive">
        {{ fieldErrors[`${prefix}.baseUrl`] }}
      </p>
    </div>
    <details class="space-y-3 sm:col-span-2">
      <summary class="cursor-pointer py-2">协议高级配置</summary>
      <div class="space-y-2">
        <Label :for="`${prefix}.priority`">协议优先级（本提供者内，数字越小越优先）</Label
        ><Input
          :id="`${prefix}.priority`"
          v-model.number="draft.priority"
          type="number"
          min="0"
          max="4294967295"
          :aria-invalid="!!fieldErrors[`${prefix}.priority`]"
        />
        <p class="text-sm text-destructive">{{ fieldErrors[`${prefix}.priority`] }}</p>
      </div>
      <div class="space-y-2">
        <Label :for="`${prefix}.compatSettings`">兼容配置 JSON（可选）</Label
        ><Input
          :id="`${prefix}.compatSettings`"
          :model-value="draft.compatSettings ?? ''"
          @update:model-value="(value) => (draft.compatSettings = String(value) || null)"
          :aria-invalid="!!fieldErrors[`${prefix}.compatSettings`]"
          class="font-mono"
        />
        <p class="text-sm text-destructive">{{ fieldErrors[`${prefix}.compatSettings`] }}</p>
      </div>
    </details>
    <label class="flex min-h-9 items-center gap-2"
      ><Checkbox
        :model-value="draft.enabled"
        @update:model-value="(value) => (draft.enabled = value === true)"
      />启用此协议</label
    >
  </div>
</template>
