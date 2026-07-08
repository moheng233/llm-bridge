<script lang="ts">
  import { getApi } from "$lib/api";
  import { auth } from "$lib/stores/auth.svelte";
  import {
    PROTOCOL_OPTIONS,
    QUOTA_ADAPTER_OPTIONS,
    QUOTA_ADAPTER_NONE,
    quotaAdapterFromSelect,
    quotaAdapterToSelect,
    quotaAdapterLabel,
  } from "$lib/constants";
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
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
  } from "$lib/components/ui/select/index.js";
  import { Plus, Trash2, ChevronDown, ChevronRight, Globe, Pencil, X, Cpu } from "@lucide/svelte";
  import type { ProviderResponse } from "$bindings/ProviderResponse";
  import type { ProviderModelResponse } from "$bindings/ProviderModelResponse";
  import type { ApiKeyEntry } from "$bindings/ApiKeyEntry";
  import type { ProtocolInput } from "$bindings/ProtocolInput";
  import type { ProtocolView } from "$bindings/ProtocolView";
  import type { ProviderCompatibility } from "$bindings/ProviderCompatibility";
  import type { ProviderQuotaAdapter } from "$bindings/ProviderQuotaAdapter";

  const api = getApi();

  // 创建一个空的 ProtocolInput（用于新增）
  function emptyProtocol(): ProtocolInput {
    return {
      protocol: "openAiChatCompletions",
      baseUrl: "",
      enabled: true,
      priority: 100,
    };
  }

  // 将适配器配置字段拼接为后端期望的 JSON 字符串。
  // 三个字段都为空时返回 null，表示该 Provider 不带适配器配置（使用内置默认值）。
  function buildQuotaConfigString(
    baseUrl: string,
    keyLabelFilter: string,
  ): string | null {
    const cfg: Record<string, string> = {};
    if (baseUrl.trim()) cfg.baseUrl = baseUrl.trim();
    if (keyLabelFilter.trim()) cfg.keyLabelFilter = keyLabelFilter.trim();
    if (Object.keys(cfg).length === 0) return null;
    return JSON.stringify(cfg);
  }

  // ── Provider list state ──
  let providers = $state<ProviderResponse[]>([]);
  let loading = $state(true);
  let error = $state("");
  let expandedId = $state<number | null>(null);
  let modelsCache = $state<Map<number, ProviderModelResponse[]>>(new Map());
  let modelsLoading = $state<Set<number>>(new Set());

  // ── Create dialog ──
  let showCreate = $state(false);
  let newProviderId = $state("");
  let newDisplayName = $state("");
  let newApiKeyLabel = $state("");
  let newApiKeyValue = $state("");
  // 创建时可选附带协议列表（PLAN §5.3 — 创建 Provider 时一并设协议）
  let newProtocols = $state<ProtocolInput[]>([]);
  // 创建对话框中当前编辑的协议表单（null 表示未在编辑）
  let newProtocolDraft = $state<ProtocolInput | null>(null);
  // ── Quota adapter (创建对话框) ──
  let newQuotaAdapter = $state<ProviderQuotaAdapter | null>(null);
  let newQuotaBaseUrl = $state("");
  let newQuotaKeyLabelFilter = $state("");

  // ── Protocol edit dialog (existing providers) ──
  // 当前正在编辑协议的 provider id；null 表示无编辑对话框打开
  let protocolEditingId = $state<number | null>(null);
  // 编辑对话框中当前编辑的协议表单
  let protocolDraft = $state<ProtocolInput | null>(null);
  // 编辑对话框中当前编辑的协议（用于在列表中替换）；null = 新增
  let protocolDraftIndex = $state<number | null>(null);

  // ── Provider list actions ──

  async function loadProviders() {
    loading = true;
    error = "";
    try {
      providers = await api.admin.listProviders();
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function toggleModels(providerId: number) {
    if (expandedId === providerId) {
      expandedId = null;
      return;
    }
    expandedId = providerId;
    if (!modelsCache.has(providerId)) {
      modelsLoading.add(providerId);
      modelsLoading = new Set(modelsLoading);
      try {
        const models = await api.admin.listProviderModels(String(providerId));
        modelsCache.set(providerId, models);
        modelsCache = new Map(modelsCache);
      } catch (e: any) {
        error = e.message;
      } finally {
        modelsLoading.delete(providerId);
        modelsLoading = new Set(modelsLoading);
      }
    }
  }

  async function handleCreate() {
    error = "";
    const apiKeys: ApiKeyEntry[] = newApiKeyValue.trim()
      ? [{ label: newApiKeyLabel || "default", key: newApiKeyValue, weight: 1 }]
      : [];
    try {
      await api.admin.createProvider({
        providerId: newProviderId,
        displayName: newDisplayName || newProviderId,
        apiKeys: apiKeys,
        protocols: newProtocols,
        enabled: true,
        priority: 100,
        quotaAdapter: newQuotaAdapter,
        quotaAdapterConfig: buildQuotaConfigString(newQuotaBaseUrl, newQuotaKeyLabelFilter),
      });
      showCreate = false;
      newProviderId = "";
      newDisplayName = "";
      newApiKeyLabel = "";
      newApiKeyValue = "";
      newProtocols = [];
      newProtocolDraft = null;
      newQuotaAdapter = null;
      newQuotaBaseUrl = "";
      newQuotaKeyLabelFilter = "";
      loadProviders();
    } catch (e: any) {
      error = e.message;
    }
  }

  function resetCreateForm() {
    newProviderId = "";
    newDisplayName = "";
    newApiKeyLabel = "";
    newApiKeyValue = "";
    newProtocols = [];
    newProtocolDraft = null;
    newQuotaAdapter = null;
    newQuotaBaseUrl = "";
    newQuotaKeyLabelFilter = "";
    error = "";
  }

  // ── Delete confirmation ──
  let deleteDialogOpen = $state(false);
  let deleteTarget = $state<{ type: "provider" | "model"; providerId: number; providerName: string; modelId?: number; modelName?: string } | null>(null);

  async function handleToggle(p: ProviderResponse) {
    error = "";
    try {
      // 将现有 protocols 转回 ProtocolInput 形式（不带 id 时后端会按 replaced 处理；带 id 时保持更新）
      const protocols: ProtocolInput[] = p.protocols.map((proto) => ({
        id: proto.id,
        protocol: proto.protocol,
        baseUrl: proto.baseUrl,
        compatSettings: proto.compatSettings,
        enabled: proto.enabled,
        priority: proto.priority,
      }));
      await api.admin.updateProvider(String(p.id), {
        displayName: p.displayName,
        enabled: !p.enabled,
        priority: p.priority,
        apiKeys: p.apiKeys.map((k: any) => ({
          label: k.label,
          key: "",
          weight: k.weight,
        })),
        protocols,
        quotaAdapter: p.quotaAdapter,
        quotaAdapterConfig: p.quotaAdapterConfig,
      });
      loadProviders();
    } catch (e: any) {
      error = e.message;
    }
  }

  // ── Protocol management (existing providers) ──
  //
  // 协议编辑采用「联调 PUT /protocols」语义：在客户端维护完整列表，
  // 用户每次保存单个协议时调用 replaceProviderProtocols 全量替换。
  // 后端做 diff，未在列表中出现的协议会被删除。

  function openProtocolEditor(provider: ProviderResponse, index?: number) {
    if (index !== undefined && provider.protocols[index]) {
      // 编辑现有
      const src = provider.protocols[index];
      protocolDraft = {
        id: src.id,
        protocol: src.protocol,
        baseUrl: src.baseUrl,
        compatSettings: src.compatSettings,
        enabled: src.enabled,
        priority: src.priority,
      };
      protocolDraftIndex = index;
    } else {
      // 新建
      protocolDraft = emptyProtocol();
      protocolDraftIndex = null;
    }
    protocolEditingId = provider.id;
    error = "";
  }

  function cancelProtocolEdit() {
    protocolDraft = null;
    protocolDraftIndex = null;
    protocolEditingId = null;
  }

  async function saveProtocolDraft(provider: ProviderResponse) {
    if (!protocolDraft) return;
    if (!protocolDraft.baseUrl.trim()) {
      error = "协议端点 URL 必填";
      return;
    }
    error = "";
    // 构建目标列表：从现有 protocols 复制，替换 draftIndex 或追加
    const list: ProtocolInput[] = provider.protocols.map((p) => ({
      id: p.id,
      protocol: p.protocol,
      baseUrl: p.baseUrl,
      compatSettings: p.compatSettings,
      enabled: p.enabled,
      priority: p.priority,
    }));
    if (protocolDraftIndex !== null && list[protocolDraftIndex]) {
      list[protocolDraftIndex] = protocolDraft;
    } else {
      list.push(protocolDraft);
    }
    try {
      await api.admin.replaceProviderProtocols(String(provider.id), list);
      cancelProtocolEdit();
      loadProviders();
    } catch (e: any) {
      error = e.message;
    }
  }

  async function removeProtocol(provider: ProviderResponse, index: number) {
    error = "";
    const list: ProtocolInput[] = provider.protocols
      .filter((_, i) => i !== index)
      .map((p) => ({
        id: p.id,
        protocol: p.protocol,
        baseUrl: p.baseUrl,
        compatSettings: p.compatSettings,
        enabled: p.enabled,
        priority: p.priority,
      }));
    try {
      await api.admin.replaceProviderProtocols(String(provider.id), list);
      loadProviders();
    } catch (e: any) {
      error = e.message;
    }
  }

  // ── Protocol draft helpers for create dialog ──
  //
  // newProtocols 是 client-side 临时数组，保存到 Provider 时一起随请求带过去。

  function addNewProtocolToCreate() {
    newProtocolDraft = emptyProtocol();
  }

  function confirmNewProtocolToCreate() {
    if (!newProtocolDraft) return;
    if (!newProtocolDraft.baseUrl.trim()) {
      error = "协议端点 URL 必填";
      return;
    }
    newProtocols = [...newProtocols, newProtocolDraft];
    newProtocolDraft = null;
    error = "";
  }

  function cancelNewProtocolToCreate() {
    newProtocolDraft = null;
  }

  function removeNewProtocolFromCreate(index: number) {
    newProtocols = newProtocols.filter((_, i) => i !== index);
  }

  function openDeleteDialog(type: "provider" | "model", providerId: number, providerName: string, modelId?: number, modelName?: string) {
    deleteTarget = { type, providerId, providerName, modelId, modelName };
    deleteDialogOpen = true;
  }

  async function handleConfirmDelete() {
    if (!deleteTarget) return;
    error = "";
    deleteDialogOpen = false;
    const t = deleteTarget;
    deleteTarget = null;
    try {
      if (t.type === "provider") {
        await api.admin.deleteProvider(String(t.providerId));
        loadProviders();
      } else if (t.type === "model" && t.modelId !== undefined) {
        await api.admin.deleteProviderModel(String(t.providerId), String(t.modelId));
        // Refresh model list
        expandedId = t.providerId;
        modelsCache.delete(t.providerId);
        modelsCache = new Map(modelsCache);
        toggleModels(t.providerId);
      }
    } catch (e: any) {
      error = e.message;
    }
  }

  async function handleToggleModel(providerId: number, m: ProviderModelResponse) {
    error = "";
    try {
      await api.admin.updateProviderModel(String(providerId), String(m.id), {
        providerModelId: m.providerModelId,
        protocolId: m.protocolId,
        displayName: m.displayName,
        maxInputTokens: m.maxInputTokens ?? 0,
        maxOutputTokens: m.maxOutputTokens ?? 0,
        toolCalling: m.toolCalling ?? false,
        vision: m.vision ?? false,
        thinking: m.thinking ?? false,
        adaptiveThinking: m.adaptiveThinking ?? false,
        inputPricePer1m: m.inputPricePer1m,
        outputPricePer1m: m.outputPricePer1m,
        cacheReadPricePer1m: m.cacheReadPricePer1m,
        enabled: !m.enabled,
      });
      // Refresh the model cache for the currently expanded provider
      if (expandedId === providerId) {
        modelsCache.delete(providerId);
        modelsCache = new Map(modelsCache);
        modelsLoading.add(providerId);
        modelsLoading = new Set(modelsLoading);
        try {
          const models = await api.admin.listProviderModels(String(providerId));
          modelsCache.set(providerId, models);
          modelsCache = new Map(modelsCache);
        } catch (_e: any) {
          // ignore
        } finally {
          modelsLoading.delete(providerId);
          modelsLoading = new Set(modelsLoading);
        }
      }
    } catch (e: any) {
      error = e.message;
    }
  }

  $effect(() => {
    if (auth.isAdmin) loadProviders();
  });
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-xl font-bold font-mono text-foreground">提供者管理</h2>
      <p class="text-sm text-muted-foreground mt-1">配置上游 LLM 提供者</p>
    </div>
    <div class="flex items-center gap-2">
      <!-- Add custom provider -->
      <Dialog open={showCreate} onOpenChange={(v) => { if (!v) resetCreateForm(); showCreate = v; }}>
        <DialogTrigger asChild>
          <Button
            class="bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium gap-2 cursor-pointer"
            onclick={() => { resetCreateForm(); showCreate = true; }}
          >
            <Plus class="h-4 w-4" />
            添加自定义提供者
          </Button>
        </DialogTrigger>
        <DialogContent class="sm:max-w-4xl">
          <DialogHeader>
            <DialogTitle class="font-mono">添加自定义提供者</DialogTitle>
          </DialogHeader>
          <div class="grid grid-cols-1 gap-x-5 gap-y-3 max-h-[80vh] overflow-y-auto pr-1 lg:grid-cols-12">
            <!-- 左列：基本属性 + 额度适配器 -->
            <div class="flex flex-col gap-3 lg:col-span-7">
              <div class="flex flex-col gap-2">
                <Label for="pid">提供者 ID</Label>
                <Input id="pid" placeholder="openai" bind:value={newProviderId} />
              </div>
              <div class="flex flex-col gap-2">
                <Label for="dn">显示名称</Label>
                <Input id="dn" placeholder="OpenAI" bind:value={newDisplayName} />
              </div>
              <div class="grid grid-cols-2 gap-3">
                <div class="flex flex-col gap-2">
                  <Label for="kl">API Key 标签</Label>
                  <Input id="kl" placeholder="default" bind:value={newApiKeyLabel} />
                </div>
                <div class="flex flex-col gap-2">
                  <Label for="kv">API Key</Label>
                  <Input
                    id="kv"
                    type="password"
                    placeholder="sk-..."
                    bind:value={newApiKeyValue}
                  />
                </div>
              </div>

              <!-- 额度适配器配置（可选）-->
              <div class="flex flex-col gap-2 pt-2 border-t border-border mt-1">
                <Label>额度适配器（可选）</Label>
                <p class="text-xs text-muted-foreground -mt-1">
                  声明该提供者使用的上游额度查询协议，用于在后台实时查询每个 API Key 的剩余额度。
                </p>
                <Select
                  type="single"
                  value={quotaAdapterToSelect(newQuotaAdapter)}
                  onValueChange={(v) => (newQuotaAdapter = quotaAdapterFromSelect(v ?? QUOTA_ADAPTER_NONE))}
                >
                  <SelectTrigger class="cursor-pointer">
                    <span class="text-sm">{QUOTA_ADAPTER_OPTIONS.find((o) => o.value === quotaAdapterToSelect(newQuotaAdapter))?.label ?? "不查询上游额度"}</span>
                  </SelectTrigger>
                  <SelectContent>
                    {#each QUOTA_ADAPTER_OPTIONS as opt}
                      <SelectItem value={opt.value}>{opt.label}</SelectItem>
                    {/each}
                  </SelectContent>
                </Select>
                {#if newQuotaAdapter}
                  <div class="flex flex-col gap-2 pl-2 border-l-2 border-border">
                    <div class="flex flex-col gap-1">
                      <Label for="qa-url" class="text-xs">覆盖端点 URL（可选）</Label>
                      <Input
                        id="qa-url"
                        placeholder="留空使用适配器默认值，如 https://api.code.umans.ai/v1/usage"
                        bind:value={newQuotaBaseUrl}
                        class="h-9 text-sm font-mono"
                      />
                    </div>
                    <div class="flex flex-col gap-1">
                      <Label for="qa-klf" class="text-xs">仅查询该 label 的 Key（可选）</Label>
                      <Input
                        id="qa-klf"
                        placeholder="留空 = 查询全部 Key"
                        bind:value={newQuotaKeyLabelFilter}
                        class="h-9 text-sm"
                      />
                    </div>
                  </div>
                {/if}
              </div>
            </div>

            <!-- 右列：协议配置 -->
            <!-- 协议列表（PLAN §3.3 — 创建 Provider 时一并设协议）-->
            <div class="flex flex-col gap-2 pt-2 border-t border-border mt-1 lg:col-span-5 lg:pt-0 lg:border-t-0 lg:border-l lg:pl-5 lg:mt-0">
              <div class="flex items-center justify-between">
                <Label>协议配置（可选）</Label>
                {#if !newProtocolDraft}
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    class="h-7 gap-1 cursor-pointer"
                    onclick={addNewProtocolToCreate}
                  >
                    <Plus class="h-3 w-3" />
                    添加协议
                  </Button>
                {/if}
              </div>
              {#if newProtocols.length === 0 && !newProtocolDraft}
                <p class="text-xs text-muted-foreground">
                  创建后可再补；空配置启动也支持。
                </p>
              {/if}
              {#each newProtocols as p, i}
                <div class="flex items-center justify-between rounded-md border border-border bg-muted/30 px-2 py-1.5 text-xs">
                  <div class="flex flex-col min-w-0">
                    <span class="font-mono">{p.protocol}</span>
                    <span class="text-muted-foreground truncate">{p.baseUrl}</span>
                  </div>
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    class="h-6 w-6 text-muted-foreground hover:text-destructive cursor-pointer"
                    onclick={() => removeNewProtocolFromCreate(i)}
                  >
                    <Trash2 class="h-3 w-3" />
                  </Button>
                </div>
              {/each}
              {#if newProtocolDraft}
                {@const draft = newProtocolDraft}
                <div class="rounded-md border border-border bg-card p-2 flex flex-col gap-2">
                  <div class="flex items-center justify-between">
                    <span class="text-xs font-medium">新建协议</span>
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      class="h-6 w-6 cursor-pointer"
                      onclick={cancelNewProtocolToCreate}
                    >
                      <X class="h-3 w-3" />
                    </Button>
                  </div>
                  <div class="flex flex-col gap-1">
                    <Label for="np-proto" class="text-xs">协议类型</Label>
                    <Select type="single" value={draft.protocol} onValueChange={(v) => v && (draft.protocol = v as ProviderCompatibility)}>
                      <SelectTrigger id="np-proto" class="cursor-pointer">
                        <span class="text-sm">{PROTOCOL_OPTIONS.find((o) => o.value === draft.protocol)?.label ?? draft.protocol}</span>
                      </SelectTrigger>
                      <SelectContent>
                        {#each PROTOCOL_OPTIONS as opt}
                          <SelectItem value={opt.value}>{opt.label}</SelectItem>
                        {/each}
                      </SelectContent>
                    </Select>
                  </div>
                  <div class="flex flex-col gap-1">
                    <Label for="np-url" class="text-xs">端点 URL</Label>
                    <Input
                      id="np-url"
                      placeholder="https://api.openai.com/v1"
                      bind:value={draft.baseUrl}
                      class="h-9 text-sm"
                    />
                  </div>
                  <div class="grid grid-cols-1 gap-2">
                    <div class="flex flex-col gap-1">
                      <Label for="np-pri" class="text-xs">优先级</Label>
                      <Input
                        id="np-pri"
                        type="number"
                        bind:value={draft.priority}
                        class="h-9 text-sm"
                      />
                    </div>
                    <div class="flex flex-col gap-1">
                      <Label for="np-cs" class="text-xs">compat settings</Label>
                      <Input
                        id="np-cs"
                        placeholder='compat JSON, 可选'
                        value={draft.compatSettings ?? ""}
                        oninput={(e) => (draft.compatSettings = (e.target as HTMLInputElement).value || null)}
                        class="h-9 text-sm font-mono"
                      />
                    </div>
                  </div>
                  <label class="flex items-center gap-2 text-xs cursor-pointer">
                    <Checkbox checked={draft.enabled} onCheckedChange={(v) => (draft.enabled = v)} />
                    启用
                  </label>
                  <div class="flex gap-2 pt-1">
                    <Button
                      type="button"
                      variant="outline"
                      class="flex-1 cursor-pointer"
                      onclick={cancelNewProtocolToCreate}
                    >
                      取消
                    </Button>
                    <Button
                      type="button"
                      class="flex-1 bg-[#22C55E] hover:bg-[#16A34A] text-black cursor-pointer"
                      onclick={confirmNewProtocolToCreate}
                      disabled={!draft.baseUrl.trim()}
                    >
                      加入列表
                    </Button>
                  </div>
                </div>
              {/if}
            </div>

            <Button
              class="bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium cursor-pointer lg:col-span-12"
              onclick={handleCreate}
              disabled={!newProviderId.trim()}
            >
              创建
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  </div>

  {#if error}
    <Alert class="border-destructive/30 bg-destructive/10">
      <AlertDescription class="text-destructive text-sm">{error}</AlertDescription>
    </Alert>
  {/if}

  {#if loading}
    <div class="flex flex-col gap-3">
      {#each Array(4) as _}
        <Skeleton class="h-16 w-full rounded-lg" />
      {/each}
    </div>
  {:else if providers.length === 0}
    <div class="flex flex-1 items-center justify-center text-muted-foreground">
      <div class="flex flex-col items-center gap-2">
        <Globe class="h-8 w-8 opacity-30" />
        <p class="text-sm">暂无提供者，点击上方按钮添加</p>
      </div>
    </div>
  {:else}
    <div class="flex flex-col gap-2 overflow-auto">
      {#each providers as p}
        <div class="rounded-lg border border-border bg-card">
          <button
            class="flex w-full items-center gap-3 px-4 py-3 text-left cursor-pointer hover:bg-accent/50 transition-colors"
            onclick={() => toggleModels(p.id)}
            onkeydown={(e) => e.key === "Enter" && toggleModels(p.id)}
          >
            {#if expandedId === p.id}
              <ChevronDown class="h-4 w-4 text-muted-foreground shrink-0" />
            {:else}
              <ChevronRight class="h-4 w-4 text-muted-foreground shrink-0" />
            {/if}
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <span class="font-mono font-medium text-foreground">{p.providerId}</span>
                <Badge
                  variant={p.enabled ? "default" : "secondary"}
                  class="text-xs"
                >
                  {p.enabled ? "启用" : "禁用"}
                </Badge>
              </div>
              <div class="flex gap-3 text-xs text-muted-foreground mt-0.5">
                <span>{p.displayName}</span>
                <span>{p.protocols.length} 个协议</span>
                <span>{p.modelCount} 个模型</span>
                <span>优先级: {p.priority}</span>
                {#if p.quotaAdapter}
                  <span class="text-foreground/70">
                    额度适配器: {quotaAdapterLabel(p.quotaAdapter)}
                  </span>
                {/if}
              </div>
            </div>
            <div class="flex items-center gap-1" role="toolbar">
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span onclick={() => handleToggle(p)} onkeydown={(e) => e.key === 'Enter' && handleToggle(p)} class="cursor-pointer inline-flex items-center" role="button" tabindex="0" aria-label={p.enabled ? "禁用提供者" : "启用提供者"}>
                <Checkbox
                  checked={p.enabled}
                  class="pointer-events-none"
                />
              </span>
              <Button
                size="icon"
                variant="ghost"
                class="h-8 w-8 text-muted-foreground hover:text-destructive cursor-pointer"
                onclick={() => openDeleteDialog("provider", p.id, p.displayName || p.providerId)}
              >
                <Trash2 class="h-4 w-4" />
              </Button>
            </div>
          </button>
          {#if expandedId === p.id}
            <div class="border-t border-border px-4 py-3 flex flex-col gap-4">
              <!-- 额度适配器区块 -->
              <div class="flex flex-col gap-1.5">
                <span class="text-xs font-medium text-muted-foreground uppercase tracking-wide">额度适配器</span>
                {#if p.quotaAdapter}
                  <div class="rounded-md border border-border bg-muted/30 px-3 py-2 text-sm flex flex-col gap-1">
                    <div class="flex items-center gap-2">
                      <Badge variant="outline" class="text-xs font-mono shrink-0">
                        {quotaAdapterLabel(p.quotaAdapter)}
                      </Badge>
                    </div>
                    {#if p.quotaAdapterConfig}
                      <pre class="text-xs font-mono text-muted-foreground whitespace-pre-wrap break-all m-0">{p.quotaAdapterConfig}</pre>
                    {:else}
                      <span class="text-xs text-muted-foreground italic">使用适配器默认配置</span>
                    {/if}
                  </div>
                {:else}
                  <p class="text-xs text-muted-foreground italic">
                    未配置 — 该提供者不查询上游额度。如需查询，请在创建时指定额度适配器。
                  </p>
                {/if}
              </div>

              <!-- 协议配置区块 -->
              <div class="flex flex-col gap-2">
                <div class="flex items-center justify-between">
                  <span class="text-xs font-medium text-muted-foreground uppercase tracking-wide">协议</span>
                  {#if protocolEditingId !== p.id}
                    <Button
                      size="sm"
                      variant="outline"
                      class="h-7 gap-1 cursor-pointer"
                      onclick={() => openProtocolEditor(p)}
                    >
                      <Plus class="h-3 w-3" />
                      添加协议
                    </Button>
                  {/if}
                </div>

                {#if p.protocols.length === 0 && protocolEditingId !== p.id}
                  <p class="text-xs text-muted-foreground italic py-1">
                    暂无协议 — 该提供者暂不可用，请先添加至少一个协议
                  </p>
                {:else}
                  <div class="flex flex-col gap-1.5">
                    {#each p.protocols as proto, i}
                      <div class="flex items-center justify-between rounded-md border border-border bg-muted/30 px-3 py-2 text-sm">
                        <div class="flex items-center gap-2 min-w-0">
                          <Badge variant="outline" class="text-xs font-mono shrink-0">
                            {proto.protocol}
                          </Badge>
                          <span class="text-muted-foreground text-xs shrink-0">P{proto.priority}</span>
                          <span class="font-mono text-foreground text-xs truncate">{proto.baseUrl}</span>
                          {#if !proto.enabled}
                            <Badge variant="secondary" class="text-xs">禁用</Badge>
                          {/if}
                        </div>
                        <div class="flex items-center gap-1 shrink-0">
                          <Button
                            size="icon"
                            variant="ghost"
                            class="h-6 w-6 text-muted-foreground hover:text-foreground cursor-pointer"
                            onclick={() => openProtocolEditor(p, i)}
                            aria-label="编辑协议"
                          >
                            <Pencil class="h-3 w-3" />
                          </Button>
                          <Button
                            size="icon"
                            variant="ghost"
                            class="h-6 w-6 text-muted-foreground hover:text-destructive cursor-pointer"
                            onclick={() => removeProtocol(p, i)}
                            aria-label="删除协议"
                          >
                            <Trash2 class="h-3 w-3" />
                          </Button>
                        </div>
                      </div>
                    {/each}
                  </div>
                {/if}

                <!-- 单个协议编辑表单（增/改）-->
                {#if protocolEditingId === p.id && protocolDraft}
                  {@const draft = protocolDraft}
                  <div class="rounded-md border border-border bg-card p-3 flex flex-col gap-2 mt-1">
                    <div class="flex items-center justify-between">
                      <span class="text-xs font-medium">
                        {protocolDraftIndex !== null ? "编辑协议" : "新建协议"}
                      </span>
                      <Button
                        size="icon"
                        variant="ghost"
                        class="h-6 w-6 cursor-pointer"
                        onclick={cancelProtocolEdit}
                        aria-label="取消"
                      >
                        <X class="h-3 w-3" />
                      </Button>
                    </div>
                    <div class="flex flex-col gap-1">
                      <Label for="ep-proto" class="text-xs">协议类型</Label>
                      <Select type="single" value={draft.protocol} onValueChange={(v) => v && (draft.protocol = v as ProviderCompatibility)}>
                        <SelectTrigger id="ep-proto" class="cursor-pointer">
                          <span class="text-sm">{PROTOCOL_OPTIONS.find((o) => o.value === draft.protocol)?.label ?? draft.protocol}</span>
                        </SelectTrigger>
                        <SelectContent>
                          {#each PROTOCOL_OPTIONS as opt}
                            <SelectItem value={opt.value}>{opt.label}</SelectItem>
                          {/each}
                        </SelectContent>
                      </Select>
                    </div>
                    <div class="flex flex-col gap-1">
                      <Label for="ep-url" class="text-xs">端点 URL</Label>
                      <Input
                        id="ep-url"
                        placeholder="https://api.openai.com/v1"
                        bind:value={draft.baseUrl}
                        class="h-9 text-sm"
                      />
                    </div>
                    <div class="grid grid-cols-2 gap-2">
                      <div class="flex flex-col gap-1">
                        <Label for="ep-pri" class="text-xs">优先级</Label>
                        <Input
                          id="ep-pri"
                          type="number"
                          bind:value={draft.priority}
                          class="h-9 text-sm"
                        />
                      </div>
                      <div class="flex flex-col gap-1">
                        <Label for="ep-cs" class="text-xs">compat settings</Label>
                        <Input
                          id="ep-cs"
                          placeholder='compat JSON, 可选'
                          value={draft.compatSettings ?? ""}
                          oninput={(e) => (draft.compatSettings = (e.target as HTMLInputElement).value || null)}
                          class="h-9 text-sm font-mono"
                        />
                      </div>
                    </div>
                    <label class="flex items-center gap-2 text-xs cursor-pointer">
                      <Checkbox checked={draft.enabled} onCheckedChange={(v) => (draft.enabled = v)} />
                      启用
                    </label>
                    <div class="flex gap-2 pt-1">
                      <Button
                        variant="outline"
                        class="flex-1 cursor-pointer"
                        onclick={cancelProtocolEdit}
                      >
                        取消
                      </Button>
                      <Button
                        class="flex-1 bg-[#22C55E] hover:bg-[#16A34A] text-black cursor-pointer"
                        onclick={() => saveProtocolDraft(p)}
                        disabled={!draft.baseUrl.trim()}
                      >
                        保存
                      </Button>
                    </div>
                  </div>
                {/if}
              </div>

              <!-- 模型列表区块 -->
              <div class="flex flex-col gap-2">
                <span class="text-xs font-medium text-muted-foreground uppercase tracking-wide">模型</span>
                {#if modelsLoading.has(p.id)}
                  <div class="flex items-center gap-2 text-sm text-muted-foreground py-2">
                    <Spinner class="h-4 w-4" />
                    加载模型...
                  </div>
                {:else if (modelsCache.get(p.id) || []).length === 0}
                  <div class="flex items-center gap-2 text-sm text-muted-foreground py-2">
                    <Cpu class="h-4 w-4" />
                    暂无模型
                  </div>
                {:else}
                  <div class="flex flex-col gap-1">
                    {#each modelsCache.get(p.id) || [] as m}
                      {@const linkedProto = p.protocols.find((pr) => pr.id === m.protocolId)}
                      <div class="flex items-center justify-between py-1.5 text-sm">
                        <div class="flex items-center gap-2 min-w-0">
                          <Badge variant="outline" class="text-xs font-mono shrink-0">
                            {linkedProto ? linkedProto.protocol : `#${m.protocolId}`}
                          </Badge>
                          <span class="font-mono text-foreground truncate">{m.modelName}</span>
                          <span class="text-muted-foreground text-xs shrink-0"
                            >→ {m.providerModelId}</span
                          >
                        </div>
                        <div class="flex items-center gap-1 shrink-0">
                          <!-- svelte-ignore a11y_no_static_element_interactions -->
                          <span
                            onclick={() => handleToggleModel(p.id, m)}
                            onkeydown={(e) => e.key === 'Enter' && handleToggleModel(p.id, m)}
                            class="cursor-pointer inline-flex items-center"
                            role="button"
                            tabindex="0"
                            aria-label={m.enabled ? "禁用模型" : "启用模型"}
                          >
                            <Badge
                              variant={m.enabled ? "default" : "secondary"}
                              class="text-xs pointer-events-none"
                            >
                              {m.enabled ? "启用" : "禁用"}
                            </Badge>
                          </span>
                          <Button
                            size="icon"
                            variant="ghost"
                            class="h-6 w-6 text-muted-foreground hover:text-destructive cursor-pointer"
                            onclick={() =>
                              openDeleteDialog(
                                "model",
                                p.id,
                                p.displayName || p.providerId,
                                m.id,
                                m.modelName,
                              )}
                          >
                            <Trash2 class="h-3 w-3" />
                          </Button>
                        </div>
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <!-- Delete confirmation dialog -->
  <Dialog open={deleteDialogOpen} onOpenChange={(v) => (deleteDialogOpen = v)}>
    <DialogContent class="sm:max-w-sm">
      <DialogHeader>
        <DialogTitle class="font-mono text-sm">确认删除</DialogTitle>
      </DialogHeader>
      <div class="flex flex-col gap-4">
        {#if deleteTarget?.type === "provider"}
          <p class="text-sm text-muted-foreground">
            确定要删除提供者 <span class="font-mono font-medium text-foreground">{deleteTarget.providerName}</span> 吗？该操作会同时删除其下所有模型，且不可撤销。
          </p>
        {:else if deleteTarget?.type === "model"}
          <p class="text-sm text-muted-foreground">
            确定要删除模型 <span class="font-mono font-medium text-foreground">{deleteTarget.modelName}</span>（属于 <span class="font-mono text-foreground">{deleteTarget.providerName}</span>）吗？该操作不可撤销。
          </p>
        {/if}
        <div class="flex gap-2">
          <Button
            variant="outline"
            class="flex-1 cursor-pointer"
            onclick={() => (deleteDialogOpen = false)}
          >
            取消
          </Button>
          <Button
            variant="destructive"
            class="flex-1 cursor-pointer"
            onclick={handleConfirmDelete}
          >
            确认删除
          </Button>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</div>
