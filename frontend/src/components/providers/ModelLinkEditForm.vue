<script setup lang="ts">
import { ref, watch, computed } from "vue";

import { type AdminModelResponse } from "@bindings/AdminModelResponse";
import { type ModelLinkView } from "@bindings/ModelLinkView";
import { type ProviderResponse } from "@bindings/ProviderResponse";
import { X } from "@lucide/vue";

import { getApi } from "~/lib/api";

const api = getApi();

const props = defineProps<{
  modelId: number;
  modelName: string;
  providers: ProviderResponse[];
  currentModel: AdminModelResponse | null;
  editingLink: ModelLinkView | null;
}>();

const emit = defineEmits<{
  saved: [];
  cancel: [];
  error: [msg: string];
}>();

// form state
const linkProviderId = ref<number | null>(null);
const linkProtocolId = ref<number | null>(null);
const linkProviderModelId = ref("");
const linkDisplayName = ref("");
const linkMaxInputStr = ref("");
const linkMaxOutputStr = ref("");
const linkToolCalling = ref<boolean | null>(null);
const linkVision = ref<boolean | null>(null);
const linkThinking = ref<boolean | null>(null);
const linkAdaptive = ref<boolean | null>(null);
const linkInputPriceStr = ref("");
const linkOutputPriceStr = ref("");
const linkCachePriceStr = ref("");
const linkEnabled = ref(true);
const linkPriorityStr = ref("100");

function resetForm() {
  linkProviderId.value = null;
  linkProtocolId.value = null;
  linkProviderModelId.value = props.modelName;
  linkDisplayName.value = "";
  linkMaxInputStr.value = "";
  linkMaxOutputStr.value = "";
  linkToolCalling.value = null;
  linkVision.value = null;
  linkThinking.value = null;
  linkAdaptive.value = null;
  linkInputPriceStr.value = "";
  linkOutputPriceStr.value = "";
  linkCachePriceStr.value = "";
  linkEnabled.value = true;
  linkPriorityStr.value = "100";
}

function fillFromLink(link: ModelLinkView) {
  linkProviderId.value = link.providerId;
  linkProtocolId.value = link.protocolId;
  linkProviderModelId.value = link.providerModelId;
  linkDisplayName.value = link.displayName;
  linkMaxInputStr.value = link.maxInputTokens != null ? String(link.maxInputTokens) : "";
  linkMaxOutputStr.value = link.maxOutputTokens != null ? String(link.maxOutputTokens) : "";
  linkToolCalling.value = link.toolCalling;
  linkVision.value = link.vision;
  linkThinking.value = link.thinking;
  linkAdaptive.value = link.adaptiveThinking;
  linkInputPriceStr.value = link.inputPricePer1m != null ? String(link.inputPricePer1m) : "";
  linkOutputPriceStr.value = link.outputPricePer1m != null ? String(link.outputPricePer1m) : "";
  linkCachePriceStr.value =
    link.cacheReadPricePer1m != null ? String(link.cacheReadPricePer1m) : "";
  linkEnabled.value = link.enabled;
  linkPriorityStr.value = String(link.priority);
}

watch(
  () => props.editingLink,
  (link) => {
    if (link) fillFromLink(link);
    else resetForm();
  },
  { immediate: true },
);

const protocolsForSelectedProvider = computed(() => {
  if (linkProviderId.value === null) return [];
  return props.providers.find((p) => p.id === linkProviderId.value)?.protocols ?? [];
});

function parseNumOrNull(s: string): number | null {
  const t = s.trim();
  if (t === "") return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
}

function toggleCheckbox(state: boolean | null, setTrue: boolean): boolean | null {
  return state === null ? setTrue : null;
}

async function saveLink() {
  if (linkProviderId.value === null || linkProtocolId.value === null) {
    emit("error", "提供者与协议均为必填");
    return;
  }
  if (!linkProviderModelId.value.trim()) {
    emit("error", "提供者侧的模型 ID 必填（如 gpt-4o）");
    return;
  }
  const body = {
    providerId: linkProviderId.value,
    providerModelId: linkProviderModelId.value.trim(),
    protocolId: linkProtocolId.value,
    displayName: linkDisplayName.value.trim() || linkProviderModelId.value.trim(),
    maxInputTokens: parseNumOrNull(linkMaxInputStr.value),
    maxOutputTokens: parseNumOrNull(linkMaxOutputStr.value),
    toolCalling: linkToolCalling.value,
    vision: linkVision.value,
    thinking: linkThinking.value,
    adaptiveThinking: linkAdaptive.value,
    inputPricePer1m: parseNumOrNull(linkInputPriceStr.value),
    outputPricePer1m: parseNumOrNull(linkOutputPriceStr.value),
    cacheReadPricePer1m: parseNumOrNull(linkCachePriceStr.value),
    enabled: linkEnabled.value,
    priority: parseNumOrNull(linkPriorityStr.value) ?? 100,
  };
  try {
    if (props.editingLink) {
      await api.admin.updateModelProvider(
        String(props.modelId),
        String(props.editingLink.id),
        body,
      );
    } else {
      await api.admin.addModelProvider(String(props.modelId), body);
    }
    emit("saved");
  } catch (e: any) {
    emit("error", e.message);
  }
}
</script>

<template>
  <div class="flex flex-col gap-2 rounded-md border border-border bg-card p-3">
    <div class="flex items-center justify-between">
      <span class="text-xs font-medium">
        {{ editingLink ? "编辑连接" : "新建连接" }}
      </span>
      <Button
        size="icon"
        variant="ghost"
        class="h-6 w-6 cursor-pointer"
        @click="emit('cancel')"
        aria-label="取消"
        ><X class="h-3 w-3"
      /></Button>
    </div>
    <div class="grid grid-cols-2 gap-2">
      <div class="flex flex-col gap-1">
        <Label class="text-xs">提供者</Label>
        <select
          v-model="linkProviderId"
          class="h-9 cursor-pointer rounded-md border border-input bg-background px-2 text-sm"
        >
          <option :value="null" disabled>选择提供者...</option>
          <option v-for="p in providers" :key="p.id" :value="p.id">
            {{ p.providerId }}（{{ p.displayName }}）
          </option>
        </select>
      </div>
      <div class="flex flex-col gap-1">
        <Label class="text-xs">协议</Label>
        <select
          v-model="linkProtocolId"
          class="h-9 cursor-pointer rounded-md border border-input bg-background px-2 text-sm disabled:cursor-not-allowed disabled:opacity-50"
          :disabled="linkProviderId === null"
        >
          <option :value="null" disabled>
            {{ linkProviderId === null ? "先选择提供者" : "选择协议..." }}
          </option>
          <option v-for="proto in protocolsForSelectedProvider" :key="proto.id" :value="proto.id">
            {{ proto.protocol }} — {{ proto.baseUrl }}
          </option>
        </select>
      </div>
    </div>
    <div class="grid grid-cols-2 gap-2">
      <div class="flex flex-col gap-1">
        <Label class="text-xs">提供者侧模型 ID</Label>
        <Input v-model="linkProviderModelId" placeholder="gpt-4o" class="h-9 font-mono text-sm" />
      </div>
      <div class="flex flex-col gap-1">
        <Label class="text-xs">显示名</Label>
        <Input
          v-model="linkDisplayName"
          placeholder="(可选) 默认用 provider_model_id"
          class="h-9 text-sm"
        />
      </div>
    </div>
    <div class="grid grid-cols-3 gap-2">
      <div class="flex flex-col gap-1">
        <Label class="text-xs">最大输入</Label>
        <Input
          v-model="linkMaxInputStr"
          type="number"
          :placeholder="currentModel ? `标称：${currentModel.maxInputTokens}` : '标称值'"
          class="h-9 text-sm"
        />
      </div>
      <div class="flex flex-col gap-1">
        <Label class="text-xs">最大输出</Label>
        <Input
          v-model="linkMaxOutputStr"
          type="number"
          :placeholder="currentModel ? `标称：${currentModel.maxOutputTokens}` : '标称值'"
          class="h-9 text-sm"
        />
      </div>
      <div class="flex flex-col gap-1">
        <Label class="text-xs">优先级</Label>
        <Input v-model="linkPriorityStr" type="number" class="h-9 text-sm" />
      </div>
    </div>
    <div class="grid grid-cols-3 gap-2">
      <div class="flex flex-col gap-1">
        <Label class="text-xs">输入价格 /1M</Label>
        <Input
          v-model="linkInputPriceStr"
          type="number"
          step="0.01"
          placeholder="（可选）覆盖标称定价"
          class="h-9 text-sm"
        />
      </div>
      <div class="flex flex-col gap-1">
        <Label class="text-xs">输出价格 /1M</Label>
        <Input
          v-model="linkOutputPriceStr"
          type="number"
          step="0.01"
          placeholder="（可选）覆盖标称定价"
          class="h-9 text-sm"
        />
      </div>
      <div class="flex flex-col gap-1">
        <Label class="text-xs">缓存读价格 /1M</Label>
        <Input
          v-model="linkCachePriceStr"
          type="number"
          step="0.01"
          placeholder="（可选）覆盖标称定价"
          class="h-9 text-sm"
        />
      </div>
    </div>
    <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs">
      <label class="flex cursor-pointer items-center gap-1.5">
        <Checkbox
          :checked="linkToolCalling ?? false"
          @update:checked="linkToolCalling = toggleCheckbox(linkToolCalling, true)"
        />
        工具调用
        <span v-if="currentModel" class="ml-0.5 text-muted-foreground"
          >（标称：{{ currentModel.toolCalling ? "✓" : "✗" }}）</span
        >
        <button
          v-if="linkToolCalling !== null"
          type="button"
          class="ml-1 text-muted-foreground underline hover:text-foreground"
          @click="linkToolCalling = null"
        >
          清除
        </button>
      </label>
      <label class="flex cursor-pointer items-center gap-1.5">
        <Checkbox
          :checked="linkVision ?? false"
          @update:checked="linkVision = toggleCheckbox(linkVision, true)"
        />
        视觉
        <span v-if="currentModel" class="ml-0.5 text-muted-foreground"
          >（标称：{{ currentModel.vision ? "✓" : "✗" }}）</span
        >
        <button
          v-if="linkVision !== null"
          type="button"
          class="ml-1 text-muted-foreground underline hover:text-foreground"
          @click="linkVision = null"
        >
          清除
        </button>
      </label>
      <label class="flex cursor-pointer items-center gap-1.5">
        <Checkbox
          :checked="linkThinking ?? false"
          @update:checked="linkThinking = toggleCheckbox(linkThinking, true)"
        />
        思考
        <span v-if="currentModel" class="ml-0.5 text-muted-foreground"
          >（标称：{{ currentModel.thinking ? "✓" : "✗" }}）</span
        >
        <button
          v-if="linkThinking !== null"
          type="button"
          class="ml-1 text-muted-foreground underline hover:text-foreground"
          @click="linkThinking = null"
        >
          清除
        </button>
      </label>
      <label class="flex cursor-pointer items-center gap-1.5">
        <Checkbox
          :checked="linkAdaptive ?? false"
          @update:checked="linkAdaptive = toggleCheckbox(linkAdaptive, true)"
        />
        自适应思考
        <span v-if="currentModel" class="ml-0.5 text-muted-foreground"
          >（标称：{{ currentModel.adaptiveThinking ? "✓" : "✗" }}）</span
        >
        <button
          v-if="linkAdaptive !== null"
          type="button"
          class="ml-1 text-muted-foreground underline hover:text-foreground"
          @click="linkAdaptive = null"
        >
          清除
        </button>
      </label>
    </div>
    <div class="flex gap-2 pt-1">
      <Button variant="outline" class="flex-1 cursor-pointer" @click="emit('cancel')">取消</Button>
      <Button
        class="flex-1 cursor-pointer bg-[#22C55E] text-black hover:bg-[#16A34A]"
        @click="saveLink"
      >
        {{ editingLink ? "更新" : "创建" }}
      </Button>
    </div>
  </div>
</template>
