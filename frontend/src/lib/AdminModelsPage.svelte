<script lang="ts">
  import { getApi } from "$lib/api";
  import { auth } from "$lib/stores/auth.svelte";
  import { SKELETON_ROWS } from "$lib/constants";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Skeleton } from "$lib/components/ui/skeleton/index.js";
  import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
  } from "$lib/components/ui/dialog/index.js";
  import { Plus, Trash2, ChevronDown, ChevronRight, Cpu, Pencil, X, Link2 } from "@lucide/svelte";
  import type { AdminModelResponse } from "$bindings/AdminModelResponse";
  import type { ModelInput } from "$bindings/ModelInput";
  import type { ModelLinkView } from "$bindings/ModelLinkView";
  import type { ProviderResponse } from "$bindings/ProviderResponse";

  const api = getApi();

  // ── Model list state ──
  let models = $state<AdminModelResponse[]>([]);
  let providers = $state<ProviderResponse[]>([]);
  let loading = $state(true);
  let error = $state("");
  let expandedId = $state<number | null>(null);
  let linksCache = $state<Map<number, ModelLinkView[]>>(new Map());
  let linksLoading = $state<Set<number>>(new Set());

  // ── Create / edit model dialog ──
  let showModelDialog = $state(false);
  let editingModelId = $state<number | null>(null);
  let formModelName = $state("");
  let formDisplayName = $state("");
  let formDescription = $state("");
  let formMaxInput = $state(4096);
  let formMaxOutput = $state(4096);
  let formToolCalling = $state(false);
  let formVision = $state(false);
  let formThinking = $state(false);
  let formAdaptive = $state(false);
  let formStatus = $state("stable");
  let formSaving = $state(false);

  // ── Link edit dialog (provider link for a model) ──
  let editingModelForLink = $state<number | null>(null); // model id
  let editingLinkId = $state<number | null>(null); // null = 新建
  let linkProviderId = $state<number | null>(null);
  let linkProtocolId = $state<number | null>(null);
  let linkProviderModelId = $state("");
  let linkDisplayName = $state("");
  // 覆盖字段：null 表示不覆盖（使用模型标称值）。
  // number 字段用字符串中间态以支持 placeholder 与空输入（避免 0 被误判为有效输入）。
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

  // ── Delete confirmation ──
  let deleteDialogOpen = $state(false);
  let deleteTarget = $state<
    | { type: "model"; modelId: number; modelName: string }
    | { type: "link"; modelId: number; linkId: number; linkName: string }
    | null
  >(null);

  async function loadModels() {
    loading = true;
    error = "";
    try {
      [models, providers] = await Promise.all([
        api.admin.listAdminModels(),
        api.admin.listProviders(),
      ]);
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function toggleLinks(modelId: number) {
    if (expandedId === modelId) {
      expandedId = null;
      return;
    }
    expandedId = modelId;
    if (!linksCache.has(modelId)) {
      linksLoading.add(modelId);
      linksLoading = new Set(linksLoading);
      try {
        const links = await api.admin.listModelProviders(String(modelId));
        linksCache.set(modelId, links);
        linksCache = new Map(linksCache);
      } catch (e: any) {
        error = e.message;
      } finally {
        linksLoading.delete(modelId);
        linksLoading = new Set(linksLoading);
      }
    }
  }

  // ── Model form helpers ──
  function resetModelForm() {
    editingModelId = null;
    formModelName = "";
    formDisplayName = "";
    formDescription = "";
    formMaxInput = 4096;
    formMaxOutput = 4096;
    formToolCalling = false;
    formVision = false;
    formThinking = false;
    formAdaptive = false;
    formStatus = "stable";
    error = "";
  }

  function openCreateModelDialog() {
    resetModelForm();
    showModelDialog = true;
  }

  function openEditModelDialog(m: AdminModelResponse) {
    editingModelId = m.id;
    formModelName = m.modelName;
    formDisplayName = m.displayName;
    formDescription = m.description ?? "";
    formMaxInput = m.maxInputTokens;
    formMaxOutput = m.maxOutputTokens;
    formToolCalling = m.toolCalling;
    formVision = m.vision;
    formThinking = m.thinking;
    formAdaptive = m.adaptiveThinking;
    formStatus = m.status ?? "stable";
    error = "";
    showModelDialog = true;
  }

  async function saveModel() {
    if (!formModelName.trim()) {
      error = "模型唯一标识 (model_name) 必填";
      return;
    }
    error = "";
    formSaving = true;
    const input: ModelInput = {
      modelName: formModelName.trim(),
      displayName: formDisplayName.trim() || formModelName.trim(),
      description: formDescription.trim() || null,
      maxInputTokens: formMaxInput,
      maxOutputTokens: formMaxOutput,
      toolCalling: formToolCalling,
      vision: formVision,
      thinking: formThinking,
      adaptiveThinking: formAdaptive,
      status: formStatus.trim() || null,
    };
    try {
      if (editingModelId !== null) {
        await api.admin.updateAdminModel(String(editingModelId), input);
      } else {
        await api.admin.createAdminModel(input);
      }
      showModelDialog = false;
      resetModelForm();
      loadModels();
    } catch (e: any) {
      error = e.message;
    } finally {
      formSaving = false;
    }
  }

  // ── Link form helpers ──
  function resetLinkForm() {
    editingLinkId = null;
    linkProviderId = null;
    linkProtocolId = null;
    linkProviderModelId = "";
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

  function openCreateLink(modelId: number, modelName: string) {
    resetLinkForm();
    // 默认提供者侧模型 ID = 该模型的规范名（用户要求）
    linkProviderModelId = modelName;
    editingModelForLink = modelId;
  }

  function openEditLink(modelId: number, link: ModelLinkView) {
    editingModelForLink = modelId;
    editingLinkId = link.id;
    linkProviderId = link.providerId;
    linkProtocolId = link.protocolId;
    linkProviderModelId = link.providerModelId;
    linkDisplayName = link.displayName;
    linkMaxInputStr = link.maxInputTokens != null ? String(link.maxInputTokens) : "";
    linkMaxOutputStr = link.maxOutputTokens != null ? String(link.maxOutputTokens) : "";
    linkToolCalling = link.toolCalling;
    linkVision = link.vision;
    linkThinking = link.thinking;
    linkAdaptive = link.adaptiveThinking;
    linkInputPriceStr = link.inputPricePer1m != null ? String(link.inputPricePer1m) : "";
    linkOutputPriceStr = link.outputPricePer1m != null ? String(link.outputPricePer1m) : "";
    linkCachePriceStr = link.cacheReadPricePer1m != null ? String(link.cacheReadPricePer1m) : "";
    linkEnabled = link.enabled;
    linkPriorityStr = String(link.priority);
  }

  function closeLinkDialog() {
    editingModelForLink = null;
    resetLinkForm();
  }

  // 解析字符串为 number ｜ null：空字符串 → null（“不覆盖”）
  function parseNumOrNull(s: string): number | null {
    const t = s.trim();
    if (t === "") return null;
    const n = Number(t);
    return Number.isFinite(n) ? n : null;
  }

  // 当前选中 provider 下的协议列表（用于第二个下拉）
  let protocolsForSelectedProvider = $derived(
    linkProviderId !== null
      ? providers.find((p) => p.id === linkProviderId)?.protocols ?? []
      : [],
  );

  // 当前正在编辑连接的模型（用于 placeholder 展示标称值）
  let currentModel = $derived(
    editingModelForLink !== null
      ? models.find((m) => m.id === editingModelForLink) ?? null
      : null,
  );

  async function saveLink() {
    if (editingModelForLink === null) return;
    if (linkProviderId === null || linkProtocolId === null) {
      error = "提供者与协议均为必填";
      return;
    }
    if (!linkProviderModelId.trim()) {
      error = "提供者侧的模型 ID 必填（如 gpt-4o）";
      return;
    }
    error = "";
    const modelId = editingModelForLink;
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
      if (editingLinkId !== null) {
        await api.admin.updateModelProvider(String(modelId), String(editingLinkId), body);
      } else {
        await api.admin.addModelProvider(String(modelId), body);
      }
      // 刷新该模型的连接缓存
      linksCache.delete(modelId);
      linksCache = new Map(linksCache);
      closeLinkDialog();
      const links = await api.admin.listModelProviders(String(modelId));
      linksCache.set(modelId, links);
      linksCache = new Map(linksCache);
      loadModels();
    } catch (e: any) {
      error = e.message;
    }
  }

  function openDeleteDialog(
    type: "model" | "link",
    modelId: number,
    targetId: number,
    name: string,
  ) {
    if (type === "model") {
      deleteTarget = { type, modelId, modelName: name };
    } else {
      deleteTarget = { type, modelId, linkId: targetId, linkName: name };
    }
    deleteDialogOpen = true;
  }

  async function confirmDelete() {
    if (!deleteTarget) return;
    deleteDialogOpen = false;
    const t = deleteTarget;
    deleteTarget = null;
    error = "";
    try {
      if (t.type === "model") {
        await api.admin.deleteAdminModel(String(t.modelId));
        // 清掉该模型相关的连接缓存
        linksCache.delete(t.modelId);
        linksCache = new Map(linksCache);
        if (expandedId === t.modelId) expandedId = null;
        loadModels();
      } else {
        await api.admin.deleteModelProvider(String(t.modelId), String(t.linkId!));
        linksCache.delete(t.modelId);
        linksCache = new Map(linksCache);
        const links = await api.admin.listModelProviders(String(t.modelId));
        linksCache.set(t.modelId, links);
        linksCache = new Map(linksCache);
        loadModels();
      }
    } catch (e: any) {
      error = e.message;
    }
  }

  $effect(() => {
    if (auth.isAdmin) loadModels();
  });
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-xl font-bold font-mono text-foreground">模型管理</h2>
      <p class="text-sm text-muted-foreground mt-1">大语言模型标称能力 + 提供者连接</p>
    </div>
    <Button
      class="bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium gap-2 cursor-pointer"
      onclick={openCreateModelDialog}
    >
      <Plus class="h-4 w-4" />
      添加模型
    </Button>
  </div>

  {#if error}
    <Alert class="border-destructive/30 bg-destructive/10">
      <AlertDescription class="text-destructive text-sm">{error}</AlertDescription>
    </Alert>
  {/if}

  {#if loading}
    <div class="flex flex-col gap-3">
      {#each Array(SKELETON_ROWS.adminModels) as _}
        <Skeleton class="h-16 w-full rounded-lg" />
      {/each}
    </div>
  {:else if models.length === 0}
    <div class="flex flex-1 items-center justify-center text-muted-foreground">
      <div class="flex flex-col items-center gap-2">
        <Cpu class="h-8 w-8 opacity-30" />
        <p class="text-sm">暂无模型，点击上方按钮添加</p>
      </div>
    </div>
  {:else}
    <div class="flex flex-col gap-2 overflow-auto">
      {#each models as m}
        <div class="rounded-lg border border-border bg-card">
          <button
            class="flex w-full items-center gap-3 px-4 py-3 text-left cursor-pointer hover:bg-accent/50 transition-colors"
            onclick={() => toggleLinks(m.id)}
            onkeydown={(e) => e.key === "Enter" && toggleLinks(m.id)}
          >
            {#if expandedId === m.id}
              <ChevronDown class="h-4 w-4 text-muted-foreground shrink-0" />
            {:else}
              <ChevronRight class="h-4 w-4 text-muted-foreground shrink-0" />
            {/if}
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 flex-wrap">
                <span class="font-mono font-medium text-foreground">{m.modelName}</span>
                {#if m.status}
                  <Badge variant="secondary" class="text-xs">{m.status}</Badge>
                {/if}
                <Badge variant="outline" class="text-xs">{m.providerCount} 个连接</Badge>
              </div>
              <div class="flex gap-3 text-xs text-muted-foreground mt-0.5 flex-wrap">
                <span>{m.displayName}</span>
                <span>↑{m.maxInputTokens.toLocaleString()}</span>
                <span>↓{m.maxOutputTokens.toLocaleString()}</span>
                {#if m.toolCalling}<Badge variant="outline" class="text-[10px] py-0">tools</Badge>{/if}
                {#if m.vision}<Badge variant="outline" class="text-[10px] py-0">vision</Badge>{/if}
                {#if m.thinking}<Badge variant="outline" class="text-[10px] py-0">thinking</Badge>{/if}
              </div>
            </div>
            <div class="flex items-center gap-1" role="toolbar">
              <Button
                size="icon"
                variant="ghost"
                class="h-7 w-7 text-muted-foreground hover:text-foreground cursor-pointer"
                onclick={(e) => { e.stopPropagation(); openEditModelDialog(m); }}
                aria-label="编辑模型"
              >
                <Pencil class="h-3.5 w-3.5" />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                class="h-7 w-7 text-muted-foreground hover:text-destructive cursor-pointer"
                onclick={(e) => { e.stopPropagation(); openDeleteDialog("model", m.id, m.id, m.modelName); }}
                aria-label="删除模型"
              >
                <Trash2 class="h-3.5 w-3.5" />
              </Button>
            </div>
          </button>
          {#if expandedId === m.id}
            <div class="border-t border-border px-4 py-3 flex flex-col gap-3">
              <div class="flex items-center justify-between">
                <span class="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                  提供者连接
                </span>
                {#if editingModelForLink !== m.id}
                  <Button
                    size="sm"
                    variant="outline"
                    class="h-7 gap-1 cursor-pointer"
                    onclick={() => openCreateLink(m.id, m.modelName)}
                  >
                    <Link2 class="h-3 w-3" />
                    添加连接
                  </Button>
                {/if}
              </div>

              {#if linksLoading.has(m.id)}
                <div class="flex items-center gap-2 text-sm text-muted-foreground py-2">
                  <Spinner class="h-4 w-4" />
                  加载连接...
                </div>
              {:else if (linksCache.get(m.id) || []).length === 0}
                <div class="flex items-center gap-2 text-sm text-muted-foreground py-2 italic">
                  <Link2 class="h-4 w-4" />
                  暂无连接 — 该模型尚未关联任何提供者
                </div>
              {:else}
                <div class="flex flex-col gap-1.5">
                  {#each linksCache.get(m.id) || [] as link}
                    <div class="flex items-center justify-between rounded-md border border-border bg-muted/30 px-3 py-2 text-sm">
                      <div class="flex items-center gap-2 min-w-0 flex-wrap">
                        <Badge variant="outline" class="text-xs font-mono shrink-0">
                          {link.protocol}
                        </Badge>
                        <span class="font-mono text-foreground text-xs">{link.providerDisplayName}</span>
                        <span class="text-muted-foreground text-xs shrink-0">→ {link.providerModelId}</span>
                        <span class="text-muted-foreground text-xs shrink-0">P{link.priority}</span>
                        {#if !link.enabled}<Badge variant="secondary" class="text-xs">禁用</Badge>{/if}
                        {#if link.inputPricePer1m !== null}
                          <span class="text-muted-foreground text-xs shrink-0">${link.inputPricePer1m}/M</span>
                        {/if}
                      </div>
                      <div class="flex items-center gap-1 shrink-0">
                        <Button
                          size="icon"
                          variant="ghost"
                          class="h-6 w-6 text-muted-foreground hover:text-foreground cursor-pointer"
                          onclick={() => openEditLink(m.id, link)}
                          aria-label="编辑连接"
                        >
                          <Pencil class="h-3 w-3" />
                        </Button>
                        <Button
                          size="icon"
                          variant="ghost"
                          class="h-6 w-6 text-muted-foreground hover:text-destructive cursor-pointer"
                          onclick={() => openDeleteDialog("link", m.id, link.id, link.providerDisplayName)}
                          aria-label="删除连接"
                        >
                          <Trash2 class="h-3 w-3" />
                        </Button>
                      </div>
                    </div>
                  {/each}
                </div>
              {/if}

              {#if editingModelForLink === m.id}
                <div class="rounded-md border border-border bg-card p-3 flex flex-col gap-2">
                  <div class="flex items-center justify-between">
                    <span class="text-xs font-medium">
                      {editingLinkId !== null ? "编辑连接" : "新建连接"}
                    </span>
                    <Button
                      size="icon"
                      variant="ghost"
                      class="h-6 w-6 cursor-pointer"
                      onclick={closeLinkDialog}
                      aria-label="取消"
                    >
                      <X class="h-3 w-3" />
                    </Button>
                  </div>
                  <div class="grid grid-cols-2 gap-2">
                    <div class="flex flex-col gap-1">
                      <Label for="lp-pid" class="text-xs">提供者</Label>
                      <select
                        id="lp-pid"
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
                      <Label for="lp-protoid" class="text-xs">协议</Label>
                      <select
                        id="lp-protoid"
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
                      <Label for="lp-pmid" class="text-xs">提供者侧模型 ID</Label>
                      <Input
                        id="lp-pmid"
                        placeholder="gpt-4o"
                        bind:value={linkProviderModelId}
                        class="h-9 text-sm font-mono"
                      />
                    </div>
                    <div class="flex flex-col gap-1">
                      <Label for="lp-dn" class="text-xs">显示名</Label>
                      <Input
                        id="lp-dn"
                        placeholder="(可选) 默认用 provider_model_id"
                        bind:value={linkDisplayName}
                        class="h-9 text-sm"
                      />
                    </div>
                  </div>
                  <div class="grid grid-cols-3 gap-2">
                    <div class="flex flex-col gap-1">
                      <Label for="lp-mi" class="text-xs">最大输入</Label>
                      <Input
                        id="lp-mi"
                        type="number"
                        value={linkMaxInputStr}
                        oninput={(e) => (linkMaxInputStr = (e.target as HTMLInputElement).value)}
                        placeholder={currentModel ? `标称：${currentModel.maxInputTokens}` : "标称值"}
                        class="h-9 text-sm"
                      />
                    </div>
                    <div class="flex flex-col gap-1">
                      <Label for="lp-mo" class="text-xs">最大输出</Label>
                      <Input
                        id="lp-mo"
                        type="number"
                        value={linkMaxOutputStr}
                        oninput={(e) => (linkMaxOutputStr = (e.target as HTMLInputElement).value)}
                        placeholder={currentModel ? `标称：${currentModel.maxOutputTokens}` : "标称值"}
                        class="h-9 text-sm"
                      />
                    </div>
                    <div class="flex flex-col gap-1">
                      <Label for="lp-pri" class="text-xs">优先级</Label>
                      <Input
                        id="lp-pri"
                        type="number"
                        value={linkPriorityStr}
                        oninput={(e) => (linkPriorityStr = (e.target as HTMLInputElement).value)}
                        class="h-9 text-sm"
                      />
                    </div>
                  </div>
                  <div class="grid grid-cols-3 gap-2">
                    <div class="flex flex-col gap-1">
                      <Label for="lp-ip" class="text-xs">输入价格 /1M</Label>
                      <Input
                        id="lp-ip"
                        type="number"
                        step="0.01"
                        value={linkInputPriceStr}
                        oninput={(e) => (linkInputPriceStr = (e.target as HTMLInputElement).value)}
                        placeholder="（可选）覆盖标称定价"
                        class="h-9 text-sm"
                      />
                    </div>
                    <div class="flex flex-col gap-1">
                      <Label for="lp-op" class="text-xs">输出价格 /1M</Label>
                      <Input
                        id="lp-op"
                        type="number"
                        step="0.01"
                        value={linkOutputPriceStr}
                        oninput={(e) => (linkOutputPriceStr = (e.target as HTMLInputElement).value)}
                        placeholder="（可选）覆盖标称定价"
                        class="h-9 text-sm"
                      />
                    </div>
                    <div class="flex flex-col gap-1">
                      <Label for="lp-cp" class="text-xs">缓存读价格 /1M</Label>
                      <Input
                        id="lp-cp"
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
                    <Button variant="outline" class="flex-1 cursor-pointer" onclick={closeLinkDialog}>
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
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <!-- 模型创建/编辑对话框 -->
  <Dialog open={showModelDialog} onOpenChange={(v) => { if (!v) resetModelForm(); showModelDialog = v; }}>
    <DialogContent class="sm:max-w-md">
      <DialogHeader>
        <DialogTitle class="font-mono">{editingModelId !== null ? "编辑模型" : "添加模型"}</DialogTitle>
      </DialogHeader>
      <div class="flex flex-col gap-3 max-h-[80vh] overflow-y-auto pr-1">
        <div class="flex flex-col gap-2">
          <Label for="fm-mn">模型唯一标识 (model_name)</Label>
          <Input id="fm-mn" placeholder="openai/gpt-4o" bind:value={formModelName} class="font-mono" />
          <p class="text-xs text-muted-foreground">前缀通常为品牌，如 openai/anthropic/...</p>
        </div>
        <div class="flex flex-col gap-2">
          <Label for="fm-dn">显示名</Label>
          <Input id="fm-dn" placeholder="GPT-4o" bind:value={formDisplayName} />
        </div>
        <div class="flex flex-col gap-2">
          <Label for="fm-desc">描述</Label>
          <textarea
            id="fm-desc"
            rows="2"
            bind:value={formDescription}
            class="rounded-md border border-input bg-background px-2 py-1.5 text-sm resize-y"
            placeholder="(可选) 模型简述"
          ></textarea>
        </div>
        <div class="grid grid-cols-2 gap-2">
          <div class="flex flex-col gap-2">
            <Label for="fm-mi">最大输入 tokens</Label>
            <Input id="fm-mi" type="number" bind:value={formMaxInput} />
          </div>
          <div class="flex flex-col gap-2">
            <Label for="fm-mo">最大输出 tokens</Label>
            <Input id="fm-mo" type="number" bind:value={formMaxOutput} />
          </div>
        </div>
        <div class="flex flex-col gap-2">
          <Label for="fm-status">状态</Label>
          <Input id="fm-status" placeholder="stable / beta / deprecated" bind:value={formStatus} />
        </div>
        <div class="flex flex-wrap gap-x-4 gap-y-2 text-sm">
          <label class="flex items-center gap-2 cursor-pointer">
            <Checkbox checked={formToolCalling} onCheckedChange={(v) => (formToolCalling = v)} />
            工具调用
          </label>
          <label class="flex items-center gap-2 cursor-pointer">
            <Checkbox checked={formVision} onCheckedChange={(v) => (formVision = v)} />
            视觉
          </label>
          <label class="flex items-center gap-2 cursor-pointer">
            <Checkbox checked={formThinking} onCheckedChange={(v) => (formThinking = v)} />
            思考
          </label>
          <label class="flex items-center gap-2 cursor-pointer">
            <Checkbox checked={formAdaptive} onCheckedChange={(v) => (formAdaptive = v)} />
            自适应思考
          </label>
        </div>
        <div class="flex gap-2 pt-2">
          <Button
            variant="outline"
            class="flex-1 cursor-pointer"
            onclick={() => { showModelDialog = false; resetModelForm(); }}
          >
            取消
          </Button>
          <Button
            class="flex-1 bg-[#22C55E] hover:bg-[#16A34A] text-black cursor-pointer"
            onclick={saveModel}
            disabled={formSaving || !formModelName.trim()}
          >
            {formSaving ? "保存中..." : "保存"}
          </Button>
        </div>
      </div>
    </DialogContent>
  </Dialog>

  <!-- 删除确认对话框 -->
  <Dialog open={deleteDialogOpen} onOpenChange={(v) => (deleteDialogOpen = v)}>
    <DialogContent class="sm:max-w-sm">
      <DialogHeader>
        <DialogTitle class="font-mono text-sm">确认删除</DialogTitle>
      </DialogHeader>
      <div class="flex flex-col gap-4">
        {#if deleteTarget?.type === "model"}
          <p class="text-sm text-muted-foreground">
            确定要删除模型 <span class="font-mono font-medium text-foreground">{deleteTarget.modelName}</span> 吗？
            该操作会同时删除该模型下的所有提供者连接，且不可撤销。
          </p>
        {:else if deleteTarget?.type === "link"}
          <p class="text-sm text-muted-foreground">
            确定要删除连接 <span class="font-mono font-medium text-foreground">{deleteTarget.linkName}</span> 吗？
            该操作不可撤销。
          </p>
        {/if}
        <div class="flex gap-2">
          <Button variant="outline" class="flex-1 cursor-pointer" onclick={() => (deleteDialogOpen = false)}>
            取消
          </Button>
          <Button variant="destructive" class="flex-1 cursor-pointer" onclick={confirmDelete}>
            确认删除
          </Button>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</div>
