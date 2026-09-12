<script setup lang="ts">
import { type ProviderResponse } from "@bindings/ProviderResponse";

import { type ConnectionDraft } from "~/lib/connection-draft";
import { protocolLabel } from "~/lib/constants";
const draft = defineModel<ConnectionDraft>({ required: true });
withDefaults(
  defineProps<{
    provider: ProviderResponse;
    fieldErrors?: Record<string, string>;
    saving?: boolean;
  }>(),
  { fieldErrors: () => ({}), saving: false },
);
const fields = [
  { key: "providerModelId", label: "上游模型 ID" },
  { key: "displayName", label: "本连接显示名" },
  { key: "inputPrice", label: "输入价格" },
  { key: "outputPrice", label: "输出价格" },
  { key: "cachePrice", label: "缓存读取价格" },
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
    <div class="space-y-2">
      <Label for="connection-protocol">连接协议</Label
      ><Select
        :model-value="draft.protocolId ?? undefined"
        @update:model-value="(value) => (draft.protocolId = Number(value))"
        ><SelectTrigger id="connection-protocol" :aria-invalid="!!fieldErrors.protocolId"
          ><SelectValue placeholder="选择本提供者协议" /></SelectTrigger
        ><SelectContent
          ><SelectItem
            v-for="protocol in provider.protocols"
            :key="protocol.id"
            :value="protocol.id"
            >{{ protocolLabel(protocol.protocol) }} · {{ protocol.baseUrl
            }}{{ protocol.enabled ? "" : "（已停用）" }}</SelectItem
          ></SelectContent
        ></Select
      >
      <p class="text-sm text-destructive">{{ fieldErrors.protocolId || fieldErrors.providerId }}</p>
    </div>
    <p class="text-sm text-muted-foreground">
      价格单位统一为 USD / 百万 Token；留空未知，0 为免费。
    </p>
    <div class="grid gap-4 sm:grid-cols-2">
      <div v-for="field in fields" :key="field.key" class="space-y-2">
        <Label :for="`connection-${field.key}`">{{ field.label }}</Label
        ><Input
          :id="`connection-${field.key}`"
          v-model="draft[field.key]"
          :aria-invalid="!!fieldErrors[field.key]"
          :class="field.key === 'providerModelId' ? 'font-mono' : ''"
        />
        <p class="text-sm text-destructive">{{ fieldErrors[field.key] }}</p>
      </div>
    </div>
    <details class="space-y-4 rounded border p-3">
      <summary class="cursor-pointer">能力覆盖与路由优先级</summary>
      <div class="grid gap-4 sm:grid-cols-2">
        <div
          v-for="field in ['maxInput', 'maxOutput', 'priority'] as const"
          :key="field"
          class="space-y-2"
        >
          <Label :for="`connection-${field}`">{{
            field === "priority"
              ? "连接优先级（数字越小越优先）"
              : field === "maxInput"
                ? "最大输入 Token 覆盖"
                : "最大输出 Token 覆盖"
          }}</Label
          ><Input
            :id="`connection-${field}`"
            v-model="draft[field]"
            :placeholder="field === 'priority' ? '0–4294967295' : '留空继承模型定义'"
            :aria-invalid="!!fieldErrors[field]"
          />
          <p class="text-sm text-destructive">{{ fieldErrors[field] }}</p>
        </div>
        <div v-for="capability in capabilities" :key="capability.key" class="space-y-2">
          <Label>{{ capability.label }}</Label
          ><Select
            :model-value="
              draft[capability.key] === null ? 'inherit' : draft[capability.key] ? 'yes' : 'no'
            "
            @update:model-value="
              (value) => (draft[capability.key] = value === 'inherit' ? null : value === 'yes')
            "
            ><SelectTrigger><SelectValue /></SelectTrigger
            ><SelectContent
              ><SelectItem value="inherit">继承模型定义</SelectItem
              ><SelectItem value="yes">支持</SelectItem
              ><SelectItem value="no">不支持</SelectItem></SelectContent
            ></Select
          >
        </div>
      </div>
    </details>
    <label class="flex min-h-9 items-center gap-2"
      ><Checkbox v-model="draft.enabled" />启用连接</label
    >
  </fieldset>
</template>
