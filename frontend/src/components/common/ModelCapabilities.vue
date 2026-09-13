<script setup lang="ts">
import { type ModelProviderSummary } from "@bindings/ModelProviderSummary";
import { type ModelResponse } from "@bindings/ModelResponse";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  Brain,
  Check,
  Eye,
  Minus,
  Workflow,
  Wrench,
} from "@lucide/vue";

type CapabilityValues = Pick<
  ModelResponse,
  "maxInputTokens" | "maxOutputTokens" | "toolCalling" | "vision" | "thinking" | "adaptiveThinking"
>;

const props = defineProps<{
  model: CapabilityValues;
  overrides?: Pick<ModelProviderSummary, keyof CapabilityValues>;
}>();

const tokenFields = [
  { key: "maxInputTokens", label: "最大输入", icon: ArrowDownToLine },
  { key: "maxOutputTokens", label: "最大输出", icon: ArrowUpFromLine },
] as const;
const capabilityFields = [
  { key: "toolCalling", label: "工具调用", icon: Wrench },
  { key: "vision", label: "视觉", icon: Eye },
  { key: "thinking", label: "推理", icon: Brain },
  { key: "adaptiveThinking", label: "自适应推理", icon: Workflow },
] as const;

function source(key: keyof CapabilityValues) {
  if (props.overrides === undefined) return null;
  return props.overrides[key] === null ? "继承" : "覆盖";
}

const tokenLimits = computed(() =>
  tokenFields.map((field) => ({
    ...field,
    value: props.overrides?.[field.key] ?? props.model[field.key],
    source: source(field.key),
  })),
);
const capabilities = computed(() =>
  capabilityFields.map((field) => {
    const override = props.overrides?.[field.key];
    const supported = (override ?? props.model[field.key]) === true;
    return {
      ...field,
      supported,
      status: supported ? "声明支持" : override === false ? "不支持" : "未声明支持",
      source: source(field.key),
    };
  }),
);
</script>

<template>
  <div class="min-w-0 space-y-4">
    <dl class="grid grid-cols-2 gap-4">
      <div v-for="limit in tokenLimits" :key="limit.key" class="min-w-0 space-y-2">
        <dt class="flex items-center gap-1.5 text-xs text-muted-foreground">
          <component :is="limit.icon" class="size-4 shrink-0 text-primary" aria-hidden="true" />
          {{ limit.label }}
        </dt>
        <dd class="space-y-1">
          <div class="flex items-baseline gap-1">
            <span class="min-w-0 text-xl leading-7 font-semibold break-all tabular-nums">{{
              limit.value.toLocaleString()
            }}</span>
            <sup
              v-if="limit.source === '继承'"
              class="relative -top-1 shrink-0 self-start rounded-sm bg-muted px-1 py-0.5 text-[10px] leading-none text-muted-foreground"
              >继承</sup
            >
            <span
              v-else-if="limit.source === '覆盖'"
              class="ml-1 shrink-0 text-xs text-muted-foreground"
              >覆盖</span
            >
          </div>
          <span class="block text-xs text-muted-foreground">Token</span>
        </dd>
      </div>
    </dl>
    <dl class="grid grid-cols-2 gap-4 border-t pt-4">
      <div v-for="capability in capabilities" :key="capability.key" class="min-w-0 space-y-2">
        <dt class="flex items-center gap-1.5 text-sm font-medium">
          <component
            :is="capability.icon"
            class="size-4 shrink-0 text-muted-foreground"
            aria-hidden="true"
          />
          {{ capability.label }}
        </dt>
        <dd class="flex items-center gap-1 text-xs">
          <span
            class="flex min-w-0 items-center gap-1"
            :class="capability.supported ? 'text-primary' : 'text-muted-foreground'"
          >
            <Check v-if="capability.supported" class="size-3 shrink-0" aria-hidden="true" />
            <Minus v-else class="size-3 shrink-0" aria-hidden="true" />
            <span class="min-w-0 break-words">{{ capability.status }}</span>
          </span>
          <sup
            v-if="capability.source === '继承'"
            class="relative -top-1 shrink-0 self-start rounded-sm bg-muted px-1 py-0.5 text-[10px] leading-none text-muted-foreground"
            >继承</sup
          >
          <span
            v-else-if="capability.source === '覆盖'"
            class="ml-1 shrink-0 border-l pl-2 text-muted-foreground"
            >覆盖</span
          >
        </dd>
      </div>
    </dl>
  </div>
</template>
