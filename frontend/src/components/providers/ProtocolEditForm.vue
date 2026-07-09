<script setup lang="ts">
import { type ProtocolInput } from "@bindings/ProtocolInput";
import { type ProviderCompatibility } from "@bindings/ProviderCompatibility";
import { X } from "@lucide/vue";

import { PROTOCOL_OPTIONS } from "~/lib/constants";

const draft = defineModel<ProtocolInput>({ required: true });

const props = withDefaults(
  defineProps<{
    title?: string;
    confirmText?: string;
  }>(),
  {
    title: "新建协议",
    confirmText: "保存",
  },
);

const emit = defineEmits<{
  confirm: [];
  cancel: [];
}>();
</script>

<template>
  <div class="mt-1 flex flex-col gap-2 rounded-md border border-border bg-card p-3">
    <div class="flex items-center justify-between">
      <span class="text-xs font-medium">{{ title }}</span>
      <Button
        size="icon"
        variant="ghost"
        class="h-6 w-6 cursor-pointer"
        @click="emit('cancel')"
        aria-label="取消"
      >
        <X class="h-3 w-3" />
      </Button>
    </div>
    <div class="flex flex-col gap-1">
      <Label class="text-xs">协议类型</Label>
      <Select
        :model-value="draft.protocol"
        @update:model-value="
          (v: any) => {
            if (v) draft.protocol = v as ProviderCompatibility;
          }
        "
      >
        <SelectTrigger class="cursor-pointer">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="opt in PROTOCOL_OPTIONS" :key="opt.value" :value="opt.value">{{
            opt.label
          }}</SelectItem>
        </SelectContent>
      </Select>
    </div>
    <div class="flex flex-col gap-1">
      <Label class="text-xs">端点 URL</Label>
      <Input v-model="draft.baseUrl" placeholder="https://api.openai.com/v1" class="h-9 text-sm" />
    </div>
    <div class="grid grid-cols-2 gap-2">
      <div class="flex flex-col gap-1">
        <Label class="text-xs">优先级</Label>
        <Input v-model.number="draft.priority" type="number" class="h-9 text-sm" />
      </div>
      <div class="flex flex-col gap-1">
        <Label class="text-xs">compat settings</Label>
        <Input
          :model-value="draft.compatSettings ?? ''"
          @update:model-value="(v: any) => (draft.compatSettings = v || null)"
          placeholder="compat JSON, 可选"
          class="h-9 font-mono text-sm"
        />
      </div>
    </div>
    <label class="flex cursor-pointer items-center gap-2 text-xs">
      <Checkbox
        :model-value="draft.enabled"
        @update:model-value="(v: any) => (draft.enabled = v)"
      />
      启用
    </label>
    <div class="flex gap-2 pt-1">
      <Button variant="outline" class="flex-1 cursor-pointer" @click="emit('cancel')">取消</Button>
      <Button
        class="flex-1 cursor-pointer bg-[#22C55E] text-black hover:bg-[#16A34A]"
        @click="emit('confirm')"
        :disabled="!draft.baseUrl.trim()"
      >
        {{ confirmText }}
      </Button>
    </div>
  </div>
</template>
