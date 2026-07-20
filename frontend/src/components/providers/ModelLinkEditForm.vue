<script setup lang="ts">
import { ref, watch, computed, onMounted } from "vue";

import { type AdminModelResponse } from "@bindings/AdminModelResponse";
import { type ModelLinkView } from "@bindings/ModelLinkView";
import { type ProviderResponse } from "@bindings/ProviderResponse";

import { getApi, formatTokens, parseTokens } from "~/lib/api";

const api = getApi();

const props = defineProps<{
  modelId: number;
  modelName: string;
  providers: ProviderResponse[];
  currentModel: AdminModelResponse | null;
  /** 编辑已有的连接；null 表示新建。 */
  editingLink: ModelLinkView | null;
}>();

const open = defineModel<boolean>("open", { default: false });

const emit = defineEmits<{
  saved: [];
  error: [msg: string];
}>();

const saving = ref(false);

// ── form state ──
const linkProviderId = ref<number | null>(null);
const linkProtocolId = ref<number | null>(null);
const linkProviderModelId = ref("");
const linkDisplayName = ref("");
const linkMaxInputStr = ref("");
const linkMaxOutputStr = ref("");
const linkToolCalling = ref(false);
const linkVision = ref(false);
const linkThinking = ref(false);
const linkAdaptive = ref(false);
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
  linkToolCalling.value = props.currentModel?.toolCalling ?? false;
  linkVision.value = props.currentModel?.vision ?? false;
  linkThinking.value = props.currentModel?.thinking ?? false;
  linkAdaptive.value = props.currentModel?.adaptiveThinking ?? false;
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
  linkToolCalling.value = link.toolCalling ?? props.currentModel?.toolCalling ?? false;
  linkVision.value = link.vision ?? props.currentModel?.vision ?? false;
  linkThinking.value = link.thinking ?? props.currentModel?.thinking ?? false;
  linkAdaptive.value = link.adaptiveThinking ?? props.currentModel?.adaptiveThinking ?? false;
  linkInputPriceStr.value = link.inputPricePer1m != null ? String(link.inputPricePer1m) : "";
  linkOutputPriceStr.value = link.outputPricePer1m != null ? String(link.outputPricePer1m) : "";
  linkCachePriceStr.value =
    link.cacheReadPricePer1m != null ? String(link.cacheReadPricePer1m) : "";
  linkEnabled.value = link.enabled;
  linkPriorityStr.value = String(link.priority);
}

function syncForm() {
  if (props.editingLink) fillFromLink(props.editingLink);
  else resetForm();
}

// v-if 创建组件时 open 已经是 true，watch 不会触发，
// 因此 onMounted 主动填充表单。watch 负责后续 open→close→open 的循环。
onMounted(() => {
  syncForm();
});
watch(open, (v, oldV) => {
  // 从 false→true 时重新填充表单
  if (v && !oldV) syncForm();
});

const protocolsForSelectedProvider = computed(() => {
  if (linkProviderId.value === null) return [];
  return props.providers.find((p) => p.id === linkProviderId.value)?.protocols ?? [];
});

function parseNumOrNull(s: string | number): number | null {
  if (typeof s === "number") return Number.isFinite(s) ? s : null;
  const t = s.trim();
  if (t === "") return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
}

/** 与标称值相同 → null（继承）；不同 → 覆盖值 */
function diffOrNull(value: boolean, nominal: boolean | undefined): boolean | null {
  return value === (nominal ?? false) ? null : value;
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
    maxInputTokens: parseTokens(linkMaxInputStr.value),
    maxOutputTokens: parseTokens(linkMaxOutputStr.value),
    toolCalling: diffOrNull(linkToolCalling.value, props.currentModel?.toolCalling),
    vision: diffOrNull(linkVision.value, props.currentModel?.vision),
    thinking: diffOrNull(linkThinking.value, props.currentModel?.thinking),
    adaptiveThinking: diffOrNull(linkAdaptive.value, props.currentModel?.adaptiveThinking),
    inputPricePer1m: parseNumOrNull(linkInputPriceStr.value),
    outputPricePer1m: parseNumOrNull(linkOutputPriceStr.value),
    cacheReadPricePer1m: parseNumOrNull(linkCachePriceStr.value),
    enabled: linkEnabled.value,
    priority: parseNumOrNull(linkPriorityStr.value) ?? 100,
  };
  saving.value = true;
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
    open.value = false;
  } catch (e: any) {
    emit("error", e.message);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <Dialog :open="open" @update:open="(v: boolean) => (open = v)">
    <DialogContent class="sm:max-w-2xl">
      <DialogHeader>
        <DialogTitle class="font-mono">
          {{ editingLink ? "编辑连接" : "新建连接" }} · {{ modelName }}
        </DialogTitle>
      </DialogHeader>

      <div class="flex flex-col gap-3 py-1">
        <div class="grid grid-cols-2 gap-3">
          <div class="flex flex-col gap-1">
            <Label class="text-xs">提供者</Label>
            <Select v-model="linkProviderId">
              <SelectTrigger class="h-9 w-full">
                <SelectValue placeholder="选择提供者..." />
              </SelectTrigger>
              <SelectContent>
                <SelectItem v-for="p in providers" :key="p.id" :value="p.id">
                  {{ p.providerId }}（{{ p.displayName }}）
                </SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="flex flex-col gap-1">
            <Label class="text-xs">协议</Label>
            <Select v-model="linkProtocolId" :disabled="linkProviderId === null">
              <SelectTrigger class="h-9 w-full">
                <SelectValue :placeholder="linkProviderId === null ? '先选择提供者' : '选择协议...'" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem v-for="proto in protocolsForSelectedProvider" :key="proto.id" :value="proto.id">
                  {{ proto.protocol }} — {{ proto.baseUrl }}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-3">
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

        <div class="grid grid-cols-3 gap-3">
          <div class="flex flex-col gap-1">
            <Label class="text-xs">最大输入</Label>
            <Input
              v-model="linkMaxInputStr"
              :placeholder="currentModel ? `标称：${formatTokens(currentModel.maxInputTokens)}` : '如 1M / 256K'"
              class="h-9 text-sm"
            />
          </div>
          <div class="flex flex-col gap-1">
            <Label class="text-xs">最大输出</Label>
            <Input
              v-model="linkMaxOutputStr"
              :placeholder="currentModel ? `标称：${formatTokens(currentModel.maxOutputTokens)}` : '如 1M / 256K'"
              class="h-9 text-sm"
            />
          </div>
          <div class="flex flex-col gap-1">
            <Label class="text-xs">优先级</Label>
            <Input v-model="linkPriorityStr" type="number" class="h-9 text-sm" />
          </div>
        </div>

        <div class="grid grid-cols-3 gap-3">
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

        <div class="flex flex-col gap-1.5">
          <Label class="text-xs text-muted-foreground">能力（勾选表示该提供者支持，与标称一致时视为继承）</Label>
          <div class="flex flex-wrap gap-x-4 gap-y-1.5 text-xs">
            <label class="flex cursor-pointer items-center gap-1.5">
              <Checkbox v-model="linkToolCalling" />
              工具调用
              <span v-if="currentModel" class="text-muted-foreground">标称:{{ currentModel.toolCalling ? "✓" : "✗" }}</span>
            </label>
            <label class="flex cursor-pointer items-center gap-1.5">
              <Checkbox v-model="linkVision" />
              视觉
              <span v-if="currentModel" class="text-muted-foreground">标称:{{ currentModel.vision ? "✓" : "✗" }}</span>
            </label>
            <label class="flex cursor-pointer items-center gap-1.5">
              <Checkbox v-model="linkThinking" />
              思考
              <span v-if="currentModel" class="text-muted-foreground">标称:{{ currentModel.thinking ? "✓" : "✗" }}</span>
            </label>
            <label class="flex cursor-pointer items-center gap-1.5">
              <Checkbox v-model="linkAdaptive" />
              自适应思考
              <span v-if="currentModel" class="text-muted-foreground">标称:{{ currentModel.adaptiveThinking ? "✓" : "✗" }}</span>
            </label>
          </div>
        </div>

        <label class="flex cursor-pointer items-center gap-1.5 text-xs">
          <Checkbox v-model="linkEnabled" />
          启用此连接
        </label>
      </div>

      <div class="flex gap-2 pt-1">
        <Button variant="outline" class="flex-1 cursor-pointer" @click="open = false">取消</Button>
        <Button
          class="flex-1 cursor-pointer bg-cta text-black hover:bg-cta-hover"
          :disabled="saving"
          @click="saveLink"
        >
          {{ saving ? "保存中..." : editingLink ? "更新" : "创建" }}
        </Button>
      </div>
    </DialogContent>
  </Dialog>
</template>
