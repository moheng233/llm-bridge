<script setup lang="ts">
import { type ProtocolInput } from "@bindings/ProtocolInput";
import { type ProviderQuotaAdapter } from "@bindings/ProviderQuotaAdapter";
import { Plus, Trash2, Pencil } from "@lucide/vue";

import ProtocolEditForm from "./ProtocolEditForm.vue";
import {
  QUOTA_ADAPTER_OPTIONS,
  QUOTA_ADAPTER_NONE,
  quotaAdapterFromSelect,
  quotaAdapterToSelect,
} from "~/lib/constants";

const isEdit = defineModel<boolean>("isEdit", { default: false });
const providerId = defineModel<string>("providerId", { default: "" });
const displayName = defineModel<string>("displayName", { default: "" });
const apiKeys = defineModel<any[]>("apiKeys", { default: () => [] });
const protocols = defineModel<ProtocolInput[]>("protocols", { default: () => [] });
const protocolDraft = defineModel<ProtocolInput | null>("protocolDraft", { default: null });
const priority = defineModel<number>("priority", { default: 100 });
const quotaAdapter = defineModel<ProviderQuotaAdapter | null>("quotaAdapter", { default: null });
const quotaBaseUrl = defineModel<string>("quotaBaseUrl", { default: "" });
const quotaKeyLabelFilter = defineModel<string>("quotaKeyLabelFilter", { default: "" });

const emit = defineEmits<{
  addApiKey: [];
  removeApiKey: [index: number];
  openProtocolEditor: [index?: number];
  confirmProtocolDraft: [];
  cancelProtocolDraft: [];
  removeProtocol: [index: number];
  submit: [];
  cancel: [];
}>();
</script>

<template>
  <div class="grid max-h-[80vh] grid-cols-1 gap-x-5 gap-y-3 overflow-y-auto pr-1 lg:grid-cols-12">
    <!-- Left column: basic + API keys + quota -->
    <div class="flex flex-col gap-3 lg:col-span-7">
      <div class="flex flex-col gap-2">
        <Label for="pid">提供者 ID</Label>
        <Input
          id="pid"
          v-model="providerId"
          :placeholder="isEdit ? '' : 'openai'"
          :disabled="isEdit"
          :class="isEdit ? 'font-mono opacity-60' : ''"
        />
        <p v-if="isEdit" class="text-xs text-muted-foreground">提供者 ID 创建后不可修改</p>
      </div>
      <div class="flex flex-col gap-2">
        <Label for="dn">显示名称</Label>
        <Input id="dn" v-model="displayName" placeholder="OpenAI" />
      </div>

      <!-- API Keys -->
      <div class="mt-1 flex flex-col gap-2 border-t border-border pt-2">
        <div class="flex items-center justify-between">
          <Label>API Keys</Label>
          <Button
            type="button"
            size="sm"
            variant="outline"
            class="h-7 cursor-pointer gap-1"
            @click="emit('addApiKey')"
          >
            <Plus class="h-3 w-3" /> 添加 Key
          </Button>
        </div>
        <p class="-mt-1 text-xs text-muted-foreground">
          {{
            isEdit
              ? "编辑模式下留空 key 框 = 保留原值；删除条目会移除该 key。"
              : "每个 Key 可带 label 和权重，用于多 Key 轮询。"
          }}
        </p>
        <p v-if="apiKeys.length === 0" class="py-1 text-xs text-muted-foreground italic">
          {{ isEdit ? "该提供者暂无 API Key，请添加" : "暂未配置 Key，可稍后在编辑界面补充" }}
        </p>
        <div v-else class="flex flex-col gap-2">
          <div v-for="(k, i) in apiKeys" :key="i" class="grid grid-cols-12 items-center gap-2">
            <Input v-model="k.label" placeholder="label" class="col-span-4 h-9 text-sm" />
            <Input
              v-model="k.key"
              type="password"
              :placeholder="isEdit ? '留空保留原值' : 'sk-...'"
              class="col-span-5 h-9 font-mono text-sm"
            />
            <Input
              v-model.number="k.weight"
              type="number"
              placeholder="权重"
              class="col-span-2 h-9 text-sm"
            />
            <Button
              type="button"
              size="icon"
              variant="ghost"
              class="col-span-1 h-9 w-9 cursor-pointer text-muted-foreground hover:text-destructive"
              @click="emit('removeApiKey', i)"
              aria-label="删除 Key"
            >
              <Trash2 class="h-4 w-4" />
            </Button>
          </div>
        </div>
      </div>

      <!-- Quota adapter -->
      <div class="mt-1 flex flex-col gap-2 border-t border-border pt-2">
        <Label>额度适配器（可选）</Label>
        <p class="-mt-1 text-xs text-muted-foreground">声明该提供者使用的上游额度查询协议。</p>
        <Select
          :model-value="quotaAdapterToSelect(quotaAdapter)"
          @update:model-value="
            (v: any) => (quotaAdapter = quotaAdapterFromSelect(v ?? QUOTA_ADAPTER_NONE))
          "
        >
          <SelectTrigger class="cursor-pointer"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem v-for="opt in QUOTA_ADAPTER_OPTIONS" :key="opt.value" :value="opt.value">{{
              opt.label
            }}</SelectItem>
          </SelectContent>
        </Select>
        <div v-if="quotaAdapter" class="flex flex-col gap-2 border-l-2 border-border pl-2">
          <div class="flex flex-col gap-1">
            <Label class="text-xs">覆盖端点 URL（可选）</Label>
            <Input
              v-model="quotaBaseUrl"
              placeholder="留空使用适配器默认值"
              class="h-9 font-mono text-sm"
            />
          </div>
          <div class="flex flex-col gap-1">
            <Label class="text-xs">仅查询该 label 的 Key（可选）</Label>
            <Input
              v-model="quotaKeyLabelFilter"
              placeholder="留空 = 查询全部 Key"
              class="h-9 text-sm"
            />
          </div>
        </div>
      </div>

      <!-- Priority -->
      <div class="flex flex-col gap-2">
        <Label for="prio">优先级</Label>
        <Input id="prio" v-model.number="priority" type="number" class="h-9 text-sm" />
      </div>
    </div>

    <!-- Right column: protocols -->
    <div
      class="mt-1 flex flex-col gap-2 border-t border-border pt-2 lg:col-span-5 lg:mt-0 lg:border-t-0 lg:border-l lg:pt-0 lg:pl-5"
    >
      <div class="flex items-center justify-between">
        <Label>协议配置（可选）</Label>
        <Button
          v-if="!protocolDraft"
          type="button"
          size="sm"
          variant="outline"
          class="h-7 cursor-pointer gap-1"
          @click="emit('openProtocolEditor')"
        >
          <Plus class="h-3 w-3" /> 添加协议
        </Button>
      </div>
      <p v-if="protocols.length === 0 && !protocolDraft" class="text-xs text-muted-foreground">
        创建后可再补；空配置启动也支持。
      </p>
      <div
        v-for="(p, i) in protocols"
        :key="i"
        class="flex items-center justify-between rounded-md border border-border bg-muted/30 px-2 py-1.5 text-xs"
      >
        <div class="flex min-w-0 flex-col">
          <span class="font-mono">{{ p.protocol }}</span>
          <span class="truncate text-muted-foreground">{{ p.baseUrl }}</span>
        </div>
        <div class="flex shrink-0 items-center gap-1">
          <Button
            type="button"
            size="icon"
            variant="ghost"
            class="h-6 w-6 cursor-pointer text-muted-foreground hover:text-foreground"
            @click="emit('openProtocolEditor', i)"
            aria-label="编辑协议"
          >
            <Pencil class="h-3 w-3" />
          </Button>
          <Button
            type="button"
            size="icon"
            variant="ghost"
            class="h-6 w-6 cursor-pointer text-muted-foreground hover:text-destructive"
            @click="emit('removeProtocol', i)"
            aria-label="删除协议"
          >
            <Trash2 class="h-3 w-3" />
          </Button>
        </div>
      </div>
      <ProtocolEditForm
        v-if="protocolDraft"
        v-model="protocolDraft"
        @confirm="emit('confirmProtocolDraft')"
        @cancel="emit('cancelProtocolDraft')"
      />

      <!-- Submit -->
      <div class="flex gap-2 border-t border-border pt-2">
        <Button variant="outline" class="flex-1 cursor-pointer" @click="emit('cancel')"
          >取消</Button
        >
        <Button
          class="flex-1 cursor-pointer bg-[#22C55E] text-black hover:bg-[#16A34A]"
          @click="emit('submit')"
          :disabled="!isEdit && !providerId.trim()"
        >
          {{ isEdit ? "保存" : "创建" }}
        </Button>
      </div>
    </div>
  </div>
</template>
