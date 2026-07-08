<script lang="ts">
  // 模型连接编辑表单 — 从 AdminModelsPage 抽出。
  // 见 PLAN.md §10 Phase B B.4。
  //
  // 父组件用法：
  //   <ModelLinkEditForm
  //     modelId={m.id}
  //     modelName={m.modelName}
  //     providers={providers}
  //     currentModel={m}
  //     editingLink={editingLink}  // null = 新建
  //     onSaved={() => refreshLinks(m.id)}
  //     onCancel={() => editingModelForLink = null}
  //     onError={(e) => error = e}
  //   />

  import { getApi } from "$lib/api";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { X } from "@lucide/svelte";
  import type { ProviderResponse } from "$bindings/ProviderResponse";
  import type { ModelLinkView } from "$bindings/ModelLinkView";
  import type { AdminModelResponse } from "$bindings/AdminModelResponse";

  const api = getApi();

  let {
    modelId,
    modelName,
    providers,
    currentModel,
    editingLink = null,
    onSaved,
    onCancel,
    onError,
  }: {
    modelId: number;
    modelName: string;
    providers: ProviderResponse[];
    currentModel: AdminModelResponse | null;
    editingLink: ModelLinkView | null;
    onSaved: () => void;
    onCancel: () => void;
    onError: (e: string) => void;
  } = $props();

  // ── 表单状态 ──
  let linkProviderId = $state<number | null>(null);
  let linkProtocolId = $state<number | null>(null);
  let linkProviderModelId = $state("");
  let linkDisplayName = $state("");
  let linkMaxInputStr = $state("");
  let linkMaxOutputStr = $state("");
  let linkToolCalling = $state<boolean | null>(null);
  let linkVision = $state<boolean | null>(null);
  let linkThinking = $state<boolean | null>(null);
  let linkAdaptive = $state<boolean | null>(null);
  let linkInputPriceStr = $state("");
  let linkOutputPriceStr = $state("");
  let linkCachePriceStr = $state("");
  let linkEnabled = $state(true);
  let linkPriorityStr = $state("100");

  // 当 editingLink 变化时回填表单
  $effect(() => {
    if (editingLink) {
      linkProviderId = editingLink.providerId;
      linkProtocolId = editingLink.protocolId;
      linkProviderModelId = editingLink.providerModelId;
      linkDisplayName = editingLink.displayName;
      linkMaxInputStr = editingLink.maxInputTokens != null ? String(editingLink.maxInputTokens) : "";
      linkMaxOutputStr = editingLink.maxOutputTokens != null ? String(editingLink.maxOutputTokens) : "";
      linkToolCalling = editingLink.toolCalling;
      linkVision = editingLink.vision;
      linkThinking = editingLink.thinking;
      linkAdaptive = editingLink.adaptiveThinking;
      linkInputPriceStr = editingLink.inputPricePer1m != null ? String(editingLink.inputPricePer1m) : "";
      linkOutputPriceStr = editingLink.outputPricePer1m != null ? String(editingLink.outputPricePer1m) : "";
      linkCachePriceStr = editingLink.cacheReadPricePer1m != null ? String(editingLink.cacheReadPricePer1m) : "";
      linkEnabled = editingLink.enabled;
      linkPriorityStr = String(editingLink.priority);
    } else {
      // 新建：默认提供者侧模型 ID = 该模型的规范名
      linkProviderId = null;
      linkProtocolId = null;
      linkProviderModelId = modelName;
      linkDisplayName = "";
      linkMaxInputStr = "";
      linkMaxOutputStr = "";
      linkToolCalling = null;
      linkVision = null;
      linkThinking = null;
      linkAdaptive = null;
      linkInputPriceStr = "";
      linkOutputPriceStr = "";
      linkCachePriceStr = "";
      linkEnabled = true;
      linkPriorityStr = "100";
    }
  });

  // 当前选中 provider 下的协议列表
  let protocolsForSelectedProvider = $derived(
    linkProviderId !== null
      ? providers.find((p) => p.id === linkProviderId)?.protocols ?? []
      : [],
  );

  function parseNumOrNull(s: string): number | null {
    const t = s.trim();
    if (t === "") return null;
    const n = Number(t);
    return Number.isFinite(n) ? n : null;
  }

  async function saveLink() {
    if (linkProviderId === null || linkProtocolId === null) {
      onError("提供者与协议均为必填");
      return;
    }
    if (!linkProviderModelId.trim()) {
      onError("提供者侧的模型 ID 必填（如 gpt-4o）");
      return;
    }
    const body = {
      providerId: linkProviderId,
      providerModelId: linkProviderModelId.trim(),
      protocolId: linkProtocolId,
      displayName: linkDisplayName.trim() || linkProviderModelId.trim(),
      maxInputTokens: parseNumOrNull(linkMaxInputStr),
      maxOutputTokens: parseNumOrNull(linkMaxOutputStr),
      toolCalling: linkToolCalling,
      vision: linkVision,
      thinking: linkThinking,
      adaptiveThinking: linkAdaptive,
      inputPricePer1m: parseNumOrNull(linkInputPriceStr),
      outputPricePer1m: parseNumOrNull(linkOutputPriceStr),
      cacheReadPricePer1m: parseNumOrNull(linkCachePriceStr),
      enabled: linkEnabled,
      priority: parseNumOrNull(linkPriorityStr) ?? 100,
    };
    try {
      if (editingLink) {
        await api.admin.updateModelProvider(String(modelId), String(editingLink.id), body);
      } else {
        await api.admin.addModelProvider(String(modelId), body);
      }
      onSaved();
    } catch (e: any) {
      onError(e.message);
    }
  }
</script>

<div class="rounded-md border border-border bg-card p-3 flex flex-col gap-2">
  <div class="flex items-center justify-between">
    <span class="text-xs font-medium">
      {editingLink ? "编辑连接" : "新建连接"}
    </span>
    <Button
      size="icon"
      variant="ghost"
      class="h-6 w-6 cursor-pointer"
      onclick={onCancel}
      aria-label="取消"
    >
      <X class="h-3 w-3" />
    </Button>
  </div>
  <div class="grid grid-cols-2 gap-2">
    <div class="flex flex-col gap-1">
      <Label class="text-xs">提供者</Label>
      <select
        class="h-9 rounded-md border border-input bg-background px-2 text-sm cursor-pointer"
        bind:value={linkProviderId}
      >
        <option value={null} disabled>选择提供者...</option>
        {#each providers as p}
          <option value={p.id}>{p.providerId}（{p.displayName}）</option>
        {/each}
      </select>
    </div>
    <div class="flex flex-col gap-1">
      <Label class="text-xs">协议</Label>
      <select
        class="h-9 rounded-md border border-input bg-background px-2 text-sm cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
        bind:value={linkProtocolId}
        disabled={linkProviderId === null}
      >
        <option value={null} disabled>
          {linkProviderId === null ? "先选择提供者" : "选择协议..."}
        </option>
        {#each protocolsForSelectedProvider as proto}
          <option value={proto.id}>{proto.protocol} — {proto.baseUrl}</option>
        {/each}
      </select>
    </div>
  </div>
  <div class="grid grid-cols-2 gap-2">
    <div class="flex flex-col gap-1">
      <Label class="text-xs">提供者侧模型 ID</Label>
      <Input
        placeholder="gpt-4o"
        bind:value={linkProviderModelId}
        class="h-9 text-sm font-mono"
      />
    </div>
    <div class="flex flex-col gap-1">
      <Label class="text-xs">显示名</Label>
      <Input
        placeholder="(可选) 默认用 provider_model_id"
        bind:value={linkDisplayName}
        class="h-9 text-sm"
      />
    </div>
  </div>
  <div class="grid grid-cols-3 gap-2">
    <div class="flex flex-col gap-1">
      <Label class="text-xs">最大输入</Label>
      <Input
        type="number"
        value={linkMaxInputStr}
        oninput={(e) => (linkMaxInputStr = (e.target as HTMLInputElement).value)}
        placeholder={currentModel ? `标称：${currentModel.maxInputTokens}` : "标称值"}
        class="h-9 text-sm"
      />
    </div>
    <div class="flex flex-col gap-1">
      <Label class="text-xs">最大输出</Label>
      <Input
        type="number"
        value={linkMaxOutputStr}
        oninput={(e) => (linkMaxOutputStr = (e.target as HTMLInputElement).value)}
        placeholder={currentModel ? `标称：${currentModel.maxOutputTokens}` : "标称值"}
        class="h-9 text-sm"
      />
    </div>
    <div class="flex flex-col gap-1">
      <Label class="text-xs">优先级</Label>
      <Input
        type="number"
        value={linkPriorityStr}
        oninput={(e) => (linkPriorityStr = (e.target as HTMLInputElement).value)}
        class="h-9 text-sm"
      />
    </div>
  </div>
  <div class="grid grid-cols-3 gap-2">
    <div class="flex flex-col gap-1">
      <Label class="text-xs">输入价格 /1M</Label>
      <Input
        type="number"
        step="0.01"
        value={linkInputPriceStr}
        oninput={(e) => (linkInputPriceStr = (e.target as HTMLInputElement).value)}
        placeholder="（可选）覆盖标称定价"
        class="h-9 text-sm"
      />
    </div>
    <div class="flex flex-col gap-1">
      <Label class="text-xs">输出价格 /1M</Label>
      <Input
        type="number"
        step="0.01"
        value={linkOutputPriceStr}
        oninput={(e) => (linkOutputPriceStr = (e.target as HTMLInputElement).value)}
        placeholder="（可选）覆盖标称定价"
        class="h-9 text-sm"
      />
    </div>
    <div class="flex flex-col gap-1">
      <Label class="text-xs">缓存读价格 /1M</Label>
      <Input
        type="number"
        step="0.01"
        value={linkCachePriceStr}
        oninput={(e) => (linkCachePriceStr = (e.target as HTMLInputElement).value)}
        placeholder="（可选）覆盖标称定价"
        class="h-9 text-sm"
      />
    </div>
  </div>
  <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs">
    <label class="flex items-center gap-1.5 cursor-pointer">
      <Checkbox
        checked={linkToolCalling ?? false}
        onCheckedChange={(v) => (linkToolCalling = linkToolCalling === null ? v : null)}
      />
      工具调用
      {#if currentModel}
        <span class="text-muted-foreground ml-0.5">（标称：{currentModel.toolCalling ? "✓" : "✗"}）</span>
      {/if}
      {#if linkToolCalling !== null}
        <button
          type="button"
          class="ml-1 text-muted-foreground hover:text-foreground underline"
          onclick={() => (linkToolCalling = null)}
        >清除</button>
      {/if}
    </label>
    <label class="flex items-center gap-1.5 cursor-pointer">
      <Checkbox
        checked={linkVision ?? false}
        onCheckedChange={(v) => (linkVision = linkVision === null ? v : null)}
      />
      视觉
      {#if currentModel}
        <span class="text-muted-foreground ml-0.5">（标称：{currentModel.vision ? "✓" : "✗"}）</span>
      {/if}
      {#if linkVision !== null}
        <button
          type="button"
          class="ml-1 text-muted-foreground hover:text-foreground underline"
          onclick={() => (linkVision = null)}
        >清除</button>
      {/if}
    </label>
    <label class="flex items-center gap-1.5 cursor-pointer">
      <Checkbox
        checked={linkThinking ?? false}
        onCheckedChange={(v) => (linkThinking = linkThinking === null ? v : null)}
      />
      思考
      {#if currentModel}
        <span class="text-muted-foreground ml-0.5">（标称：{currentModel.thinking ? "✓" : "✗"}）</span>
      {/if}
      {#if linkThinking !== null}
        <button
          type="button"
          class="ml-1 text-muted-foreground hover:text-foreground underline"
          onclick={() => (linkThinking = null)}
        >清除</button>
      {/if}
    </label>
    <label class="flex items-center gap-1.5 cursor-pointer">
      <Checkbox
        checked={linkAdaptive ?? false}
        onCheckedChange={(v) => (linkAdaptive = linkAdaptive === null ? v : null)}
      />
      自适应思考
      {#if currentModel}
        <span class="text-muted-foreground ml-0.5">（标称：{currentModel.adaptiveThinking ? "✓" : "✗"}）</span>
      {/if}
      {#if linkAdaptive !== null}
        <button
          type="button"
          class="ml-1 text-muted-foreground hover:text-foreground underline"
          onclick={() => (linkAdaptive = null)}
        >清除</button>
      {/if}
    </label>
    <label class="flex items-center gap-1.5 cursor-pointer">
      <Checkbox checked={linkEnabled} onCheckedChange={(v) => (linkEnabled = v)} />
      启用
    </label>
  </div>
  <div class="flex gap-2 pt-1">
    <Button variant="outline" class="flex-1 cursor-pointer" onclick={onCancel}>
      取消
    </Button>
    <Button
      class="flex-1 bg-[#22C55E] hover:bg-[#16A34A] text-black cursor-pointer"
      onclick={saveLink}
      disabled={linkProviderId === null || linkProtocolId === null || !linkProviderModelId.trim()}
    >
      保存
    </Button>
  </div>
</div>
