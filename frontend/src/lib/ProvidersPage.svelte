<script lang="ts">
  // 提供者管理 — orchestrator 容器。
  // 子组件：ProviderCreateDialog / ProviderRow。
  // 见 PLAN.md §10 Phase B B.3。
  import { getApi } from "$lib/api";
  import { auth } from "$lib/stores/auth.svelte";
  import { SKELETON_ROWS } from "$lib/constants";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";
  import { Skeleton } from "$lib/components/ui/skeleton/index.js";
  import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
  } from "$lib/components/ui/dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Globe } from "@lucide/svelte";
  import type { ProviderResponse } from "$bindings/ProviderResponse";
  import type { ProviderModelResponse } from "$bindings/ProviderModelResponse";
  import ProviderCreateDialog from "./providers/ProviderCreateDialog.svelte";
  import ProviderRow from "./providers/ProviderRow.svelte";

  const api = getApi();

  // ── 列表状态 ──
  let providers = $state<ProviderResponse[]>([]);
  let loading = $state(true);
  let error = $state("");
  let expandedId = $state<number | null>(null);
  let modelsCache = $state<Map<number, ProviderModelResponse[]>>(new Map());
  let modelsLoading = $state<Set<number>>(new Set());

  // ── 删除状态 ──
  let deleteDialogOpen = $state(false);
  let deleteTarget = $state<{
    type: "provider" | "model";
    providerId: number;
    providerName: string;
    modelId?: number;
    modelName?: string;
  } | null>(null);

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

  async function handleToggle(p: ProviderResponse) {
    error = "";
    try {
      const protocols = p.protocols.map((proto) => ({
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

  function openDeleteDialog(
    type: "provider" | "model",
    providerId: number,
    providerName: string,
    modelId?: number,
    modelName?: string,
  ) {
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
        expandedId = t.providerId;
        modelsCache.delete(t.providerId);
        modelsCache = new Map(modelsCache);
        toggleModels(t.providerId);
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
    <ProviderCreateDialog onCreated={loadProviders} onError={(e) => (error = e)} />
  </div>

  <!-- Error -->
  {#if error}
    <Alert class="border-destructive/30 bg-destructive/10">
      <AlertDescription class="text-destructive text-sm">{error}</AlertDescription>
    </Alert>
  {/if}

  <!-- List -->
  {#if loading}
    <div class="flex flex-col gap-3">
      {#each Array(SKELETON_ROWS.providers) as _}
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
        <ProviderRow
          provider={p}
          expanded={expandedId === p.id}
          models={modelsCache.get(p.id) || []}
          modelsLoading={modelsLoading.has(p.id)}
          onToggleExpand={() => toggleModels(p.id)}
          onToggleEnabled={() => handleToggle(p)}
          onDeleteProvider={() => openDeleteDialog("provider", p.id, p.displayName || p.providerId)}
          onToggleModel={(m) => handleToggleModel(p.id, m)}
          onDeleteModel={(m) => openDeleteDialog("model", p.id, p.displayName || p.providerId, m.id, m.modelName)}
          onProtocolsChanged={loadProviders}
          onError={(e) => (error = e)}
        />
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
