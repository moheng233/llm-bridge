<script lang="ts">
  // 单个 Provider 展开行 — 从 ProvidersPage 抽出。
  // 见 PLAN.md §10 Phase B B.3。
  //
  // 父组件用法：
  //   <ProviderRow
  //     provider={p}
  //     expanded={expandedId === p.id}
  //     models={modelsCache.get(p.id) || []}
  //     modelsLoading={modelsLoading.has(p.id)}
  //     onToggleExpand={() => toggleModels(p.id)}
  //     onToggleEnabled={() => handleToggle(p)}
  //     onDeleteProvider={() => openDeleteDialog("provider", p.id, p.displayName || p.providerId)}
  //     onToggleModel={(m) => handleToggleModel(p.id, m)}
  //     onDeleteModel={(m) => openDeleteDialog("model", p.id, p.displayName || p.providerId, m.id, m.modelName)}
  //     onProtocolsChanged={loadProviders}
  //     onError={(e) => error = e}
  //   />

  import { getApi } from "$lib/api";
  import { quotaAdapterLabel } from "$lib/constants";
  import { emptyProtocol, protocolViewToInput } from "$lib/utils/provider";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Plus, Trash2, ChevronDown, ChevronRight, Pencil, Cpu } from "@lucide/svelte";
  import type { ProviderResponse } from "$bindings/ProviderResponse";
  import type { ProviderModelResponse } from "$bindings/ProviderModelResponse";
  import type { ProtocolInput } from "$bindings/ProtocolInput";
  import ProtocolEditForm from "./ProtocolEditForm.svelte";

  const api = getApi();

  let {
    provider: p,
    expanded,
    models = [],
    modelsLoading = false,
    onToggleExpand,
    onToggleEnabled,
    onDeleteProvider,
    onToggleModel,
    onDeleteModel,
    onProtocolsChanged,
    onError,
  }: {
    provider: ProviderResponse;
    expanded: boolean;
    models: ProviderModelResponse[];
    modelsLoading: boolean;
    onToggleExpand: () => void;
    onToggleEnabled: () => void;
    onDeleteProvider: () => void;
    onToggleModel: (m: ProviderModelResponse) => void;
    onDeleteModel: (m: ProviderModelResponse) => void;
    onProtocolsChanged: () => void;
    onError: (e: string) => void;
  } = $props();

  // ── 协议编辑状态（本行内自持） ──
  let protocolEditing = $state(false);
  let protocolDraft = $state<ProtocolInput | null>(null);
  let protocolDraftIndex = $state<number | null>(null);

  function openProtocolEditor(index?: number) {
    if (index !== undefined && p.protocols[index]) {
      const src = p.protocols[index];
      protocolDraft = protocolViewToInput(src);
      protocolDraftIndex = index;
    } else {
      protocolDraft = emptyProtocol();
      protocolDraftIndex = null;
    }
    protocolEditing = true;
  }

  function cancelProtocolEdit() {
    protocolDraft = null;
    protocolDraftIndex = null;
    protocolEditing = false;
  }

  async function saveProtocolDraft() {
    if (!protocolDraft) return;
    if (!protocolDraft.baseUrl.trim()) {
      onError("协议端点 URL 必填");
      return;
    }
    const list: ProtocolInput[] = p.protocols.map(protocolViewToInput);
    if (protocolDraftIndex !== null && list[protocolDraftIndex]) {
      list[protocolDraftIndex] = protocolDraft;
    } else {
      list.push(protocolDraft);
    }
    try {
      await api.admin.replaceProviderProtocols(String(p.id), list);
      cancelProtocolEdit();
      onProtocolsChanged();
    } catch (e: any) {
      onError(e.message);
    }
  }

  async function removeProtocol(index: number) {
    const list: ProtocolInput[] = p.protocols
      .filter((_, i) => i !== index)
      .map(protocolViewToInput);
    try {
      await api.admin.replaceProviderProtocols(String(p.id), list);
      onProtocolsChanged();
    } catch (e: any) {
      onError(e.message);
    }
  }
</script>

<div class="rounded-lg border border-border bg-card">
  <button
    class="flex w-full items-center gap-3 px-4 py-3 text-left cursor-pointer hover:bg-accent/50 transition-colors"
    onclick={onToggleExpand}
    onkeydown={(e) => e.key === "Enter" && onToggleExpand()}
  >
    {#if expanded}
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
      <span onclick={onToggleEnabled} onkeydown={(e) => e.key === 'Enter' && onToggleEnabled()} class="cursor-pointer inline-flex items-center" role="button" tabindex="0" aria-label={p.enabled ? "禁用提供者" : "启用提供者"}>
        <Checkbox
          checked={p.enabled}
          class="pointer-events-none"
        />
      </span>
      <Button
        size="icon"
        variant="ghost"
        class="h-8 w-8 text-muted-foreground hover:text-destructive cursor-pointer"
        onclick={onDeleteProvider}
      >
        <Trash2 class="h-4 w-4" />
      </Button>
    </div>
  </button>
  {#if expanded}
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
          {#if !protocolEditing}
            <Button
              size="sm"
              variant="outline"
              class="h-7 gap-1 cursor-pointer"
              onclick={() => openProtocolEditor()}
            >
              <Plus class="h-3 w-3" />
              添加协议
            </Button>
          {/if}
        </div>

        {#if p.protocols.length === 0 && !protocolEditing}
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
                    onclick={() => openProtocolEditor(i)}
                    aria-label="编辑协议"
                  >
                    <Pencil class="h-3 w-3" />
                  </Button>
                  <Button
                    size="icon"
                    variant="ghost"
                    class="h-6 w-6 text-muted-foreground hover:text-destructive cursor-pointer"
                    onclick={() => removeProtocol(i)}
                    aria-label="删除协议"
                  >
                    <Trash2 class="h-3 w-3" />
                  </Button>
                </div>
              </div>
            {/each}
          </div>
        {/if}

        {#if protocolEditing && protocolDraft}
          <ProtocolEditForm
            bind:draft={protocolDraft}
            title={protocolDraftIndex !== null ? "编辑协议" : "新建协议"}
            confirmText="保存"
            onConfirm={saveProtocolDraft}
            onCancel={cancelProtocolEdit}
          />
        {/if}
      </div>

      <!-- 模型列表区块 -->
      <div class="flex flex-col gap-2">
        <span class="text-xs font-medium text-muted-foreground uppercase tracking-wide">模型</span>
        {#if modelsLoading}
          <div class="flex items-center gap-2 text-sm text-muted-foreground py-2">
            <Spinner class="h-4 w-4" />
            加载模型...
          </div>
        {:else if models.length === 0}
          <div class="flex items-center gap-2 text-sm text-muted-foreground py-2">
            <Cpu class="h-4 w-4" />
            暂无模型
          </div>
        {:else}
          <div class="flex flex-col gap-1">
            {#each models as m}
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
                    onclick={() => onToggleModel(m)}
                    onkeydown={(e) => e.key === 'Enter' && onToggleModel(m)}
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
                    onclick={() => onDeleteModel(m)}
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
