<script lang="ts">
  import { getApi } from "$lib/api";
  import { auth } from "$lib/stores/auth.svelte";
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
  import { Plus, Trash2, ChevronDown, ChevronRight, Globe } from "@lucide/svelte";
  import type { ProviderResponse } from "$bindings/ProviderResponse";
  import type { ProviderModelResponse } from "$bindings/ProviderModelResponse";
  import type { ApiKeyEntry } from "$bindings/ApiKeyEntry";

  const api = getApi();

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
        enabled: true,
        priority: 100,
      });
      showCreate = false;
      newProviderId = "";
      newDisplayName = "";
      newApiKeyLabel = "";
      newApiKeyValue = "";
      loadProviders();
    } catch (e: any) {
      error = e.message;
    }
  }

  // ── Delete confirmation ──
  let deleteDialogOpen = $state(false);
  let deleteTarget = $state<{ type: "provider" | "model"; providerId: number; providerName: string; modelId?: number; modelName?: string } | null>(null);

  async function handleToggle(p: ProviderResponse) {
    error = "";
    try {
      await api.admin.updateProvider(String(p.id), {
        displayName: p.displayName,
        enabled: !p.enabled,
        priority: p.priority,
        apiKeys: p.apiKeys.map((k: any) => ({
          label: k.label,
          key: "",
          weight: k.weight,
        })),
      });
      loadProviders();
    } catch (e: any) {
      error = e.message;
    }
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
        maxInputTokens: m.maxInputTokens,
        maxOutputTokens: m.maxOutputTokens,
        toolCalling: m.toolCalling,
        vision: m.vision,
        thinking: m.thinking,
        adaptiveThinking: m.adaptiveThinking,
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
      <Dialog open={showCreate} onOpenChange={(v) => (showCreate = v)}>
        <DialogTrigger asChild>
          <Button
            class="bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium gap-2 cursor-pointer"
            onclick={() => (showCreate = true)}
          >
            <Plus class="h-4 w-4" />
            添加自定义提供者
          </Button>
        </DialogTrigger>
        <DialogContent class="sm:max-w-md">
          <DialogHeader>
            <DialogTitle class="font-mono">添加自定义提供者</DialogTitle>
          </DialogHeader>
          <div class="flex flex-col gap-3">
            <div class="flex flex-col gap-2">
              <Label for="pid">提供者 ID</Label>
              <Input id="pid" placeholder="openai" bind:value={newProviderId} />
            </div>
            <div class="flex flex-col gap-2">
              <Label for="dn">显示名称</Label>
              <Input id="dn" placeholder="OpenAI" bind:value={newDisplayName} />
            </div>
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
            <Button
              class="bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium cursor-pointer"
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
                <span>{p.modelCount} 个模型</span>
                <span>优先级: {p.priority}</span>
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
            <div class="border-t border-border px-4 py-3">
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
                    <div class="flex items-center justify-between py-1.5 text-sm">
                      <div class="flex items-center gap-2 min-w-0">
                        <Badge variant="outline" class="text-xs font-mono shrink-0">
                          {m.compatibility}
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
