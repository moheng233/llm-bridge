<script setup lang="ts">
import { type ProviderModelResponse } from "@bindings/ProviderModelResponse";
import { type ProviderResponse } from "@bindings/ProviderResponse";
import { Trash2, ChevronDown, ChevronRight, Pencil, Cpu } from "@lucide/vue";

import { quotaAdapterLabel } from "~/lib/constants";

const props = defineProps<{
  provider: ProviderResponse;
  expanded: boolean;
  models: ProviderModelResponse[];
  modelsLoading: boolean;
}>();

const emit = defineEmits<{
  toggleExpand: [];
  toggleEnabled: [];
  edit: [];
  deleteProvider: [];
  toggleModel: [model: ProviderModelResponse];
  deleteModel: [model: ProviderModelResponse];
  error: [msg: string];
}>();
</script>

<template>
  <div class="rounded-lg border border-border bg-card">
    <button
      class="flex w-full cursor-pointer items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-accent/50"
      @click="emit('toggleExpand')"
    >
      <ChevronDown v-if="expanded" class="h-4 w-4 shrink-0 text-muted-foreground" />
      <ChevronRight v-else class="h-4 w-4 shrink-0 text-muted-foreground" />
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <span class="font-mono font-medium text-foreground">{{ provider.providerId }}</span>
          <Badge :variant="provider.enabled ? 'default' : 'secondary'" class="text-xs">
            {{ provider.enabled ? "启用" : "禁用" }}
          </Badge>
        </div>
        <div class="mt-0.5 flex gap-3 text-xs text-muted-foreground">
          <span>{{ provider.displayName }}</span>
          <span>{{ provider.protocols.length }} 个协议</span>
          <span>{{ provider.modelCount }} 个模型</span>
          <span>优先级: {{ provider.priority }}</span>
          <span v-if="provider.quotaAdapter" class="text-foreground/70"
            >额度适配器: {{ quotaAdapterLabel(provider.quotaAdapter) }}</span
          >
        </div>
      </div>
      <div class="flex items-center gap-1" role="toolbar">
        <span
          class="inline-flex cursor-pointer items-center"
          role="button"
          tabindex="0"
          :aria-label="provider.enabled ? '禁用提供者' : '启用提供者'"
          @keydown.enter="emit('toggleEnabled')"
          @click.stop="emit('toggleEnabled')"
        >
          <Checkbox :checked="provider.enabled" class="pointer-events-none" />
        </span>
        <Button
          size="icon"
          variant="ghost"
          class="h-8 w-8 cursor-pointer text-muted-foreground hover:text-foreground"
          @click.stop="emit('edit')"
          aria-label="编辑提供者"
        >
          <Pencil class="h-4 w-4" />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          class="h-8 w-8 cursor-pointer text-muted-foreground hover:text-destructive"
          @click.stop="emit('deleteProvider')"
        >
          <Trash2 class="h-4 w-4" />
        </Button>
      </div>
    </button>

    <div v-if="expanded" class="flex flex-col gap-4 border-t border-border px-4 py-3">
      <!-- API Keys 摘要（只读） -->
      <div class="flex flex-col gap-1.5">
        <span class="text-xs font-medium tracking-wide text-muted-foreground uppercase"
          >API Keys</span
        >
        <p v-if="provider.apiKeys.length === 0" class="text-xs text-muted-foreground italic">
          暂无 API Key — 该提供者无法路由，请点击编辑按钮添加
        </p>
        <div v-else class="flex flex-col gap-1">
          <div
            v-for="k in provider.apiKeys"
            :key="k.label"
            class="flex items-center gap-2 py-1 text-xs"
          >
            <Badge variant="outline" class="shrink-0 font-mono text-xs">{{ k.label }}</Badge>
            <span class="font-mono text-muted-foreground">{{ k.maskedKey }}</span>
            <span class="text-muted-foreground">权重 {{ k.weight }}</span>
          </div>
        </div>
      </div>

      <!-- 额度适配器区块 -->
      <div class="flex flex-col gap-1.5">
        <span class="text-xs font-medium tracking-wide text-muted-foreground uppercase"
          >额度适配器</span
        >
        <div
          v-if="provider.quotaAdapter"
          class="flex flex-col gap-1 rounded-md border border-border bg-muted/30 px-3 py-2 text-sm"
        >
          <div class="flex items-center gap-2">
            <Badge variant="outline" class="shrink-0 font-mono text-xs">{{
              quotaAdapterLabel(provider.quotaAdapter)
            }}</Badge>
          </div>
          <pre
            v-if="provider.quotaAdapterConfig"
            class="m-0 font-mono text-xs break-all whitespace-pre-wrap text-muted-foreground"
            >{{ provider.quotaAdapterConfig }}</pre>
          <span v-else class="text-xs text-muted-foreground italic">使用适配器默认配置</span>
        </div>
        <p v-else class="text-xs text-muted-foreground italic">
          未配置 — 该提供者不查询上游额度。如需查询，请在创建时指定额度适配器。
        </p>
      </div>

      <!-- 协议区块（只读） -->
      <div class="flex flex-col gap-2">
        <span class="text-xs font-medium tracking-wide text-muted-foreground uppercase">协议</span>
        <p v-if="provider.protocols.length === 0" class="py-1 text-xs text-muted-foreground italic">
          暂无协议 — 该提供者暂不可用，请点击编辑按钮添加
        </p>
        <div v-else class="flex flex-col gap-1.5">
          <div
            v-for="proto in provider.protocols"
            :key="proto.id"
            class="flex items-center justify-between rounded-md border border-border bg-muted/30 px-3 py-2 text-sm"
          >
            <div class="flex min-w-0 items-center gap-2">
              <Badge variant="outline" class="shrink-0 font-mono text-xs">{{
                proto.protocol
              }}</Badge>
              <span class="shrink-0 text-xs text-muted-foreground">P{{ proto.priority }}</span>
              <span class="truncate font-mono text-xs text-foreground">{{ proto.baseUrl }}</span>
              <Badge v-if="!proto.enabled" variant="secondary" class="text-xs">禁用</Badge>
            </div>
          </div>
        </div>
      </div>

      <!-- 模型列表 -->
      <div class="flex flex-col gap-2">
        <span class="text-xs font-medium tracking-wide text-muted-foreground uppercase">模型</span>
        <div
          v-if="modelsLoading"
          class="flex items-center gap-2 py-2 text-sm text-muted-foreground"
        >
          <Spinner class="h-4 w-4" /> 加载模型...
        </div>
        <div
          v-else-if="models.length === 0"
          class="flex items-center gap-2 py-2 text-sm text-muted-foreground italic"
        >
          <Cpu class="h-4 w-4" /> 暂无模型
        </div>
        <div v-else class="flex flex-col gap-1.5">
          <div
            v-for="m in models"
            :key="m.id"
            class="flex items-center justify-between rounded-md border border-border bg-muted/30 px-3 py-2 text-sm"
          >
            <div class="flex min-w-0 flex-wrap items-center gap-2">
              <span class="font-mono text-xs text-foreground">{{
                m.modelName || m.providerModelId
              }}</span>
              <Badge variant="outline" class="font-mono text-xs">{{ m.providerModelId }}</Badge>
              <Badge :variant="m.enabled ? 'default' : 'secondary'" class="text-xs">{{
                m.enabled ? "启用" : "禁用"
              }}</Badge>
              <span v-if="m.inputPricePer1m !== null" class="text-xs text-muted-foreground"
                >${{ m.inputPricePer1m }}/M in</span
              >
              <span v-if="m.outputPricePer1m !== null" class="text-xs text-muted-foreground"
                >${{ m.outputPricePer1m }}/M out</span
              >
            </div>
            <div class="flex shrink-0 items-center gap-1">
              <span
                class="inline-flex cursor-pointer items-center"
                role="button"
                tabindex="0"
                :aria-label="m.enabled ? '禁用模型' : '启用模型'"
                @keydown.enter="emit('toggleModel', m)"
                @click="emit('toggleModel', m)"
              >
                <Checkbox :checked="m.enabled" class="pointer-events-none h-4 w-4" />
              </span>
              <Button
                size="icon"
                variant="ghost"
                class="h-6 w-6 cursor-pointer text-muted-foreground hover:text-destructive"
                @click="emit('deleteModel', m)"
              >
                <Trash2 class="h-3 w-3" />
              </Button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
