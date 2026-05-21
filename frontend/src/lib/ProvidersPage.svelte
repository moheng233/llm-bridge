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
  import { Plus, Trash2, ChevronDown, ChevronRight, Globe, Cpu, Search } from "@lucide/svelte";
  import type { ProviderResponse } from "$bindings/ProviderResponse";
  import type { ProviderModelResponse } from "$bindings/ProviderModelResponse";
  import type { ApiKeyEntry } from "$bindings/ApiKeyEntry";
  import type { CatalogProviderSummary } from "$bindings/CatalogProviderSummary";
  import type { ImportedProvider } from "$bindings/ImportedProvider";

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
  let newBaseUrl = $state("");
  let newApiKeyLabel = $state("");
  let newApiKeyValue = $state("");

  // ── Import (models.dev) dialog ──
  let showImport = $state(false);
  let catalogProviders = $state<CatalogProviderSummary[]>([]);
  let catalogLoading = $state(false);
  let catalogError = $state("");
  let importFilter = $state("");
  let importingId = $state<string | null>(null);
  let importError = $state("");
  let lastImportResult = $state<ImportedProvider | null>(null);
  // Key-entry modal
  let keyModalOpen = $state(false);
  let keyModalProviderId = $state("");
  let keyModalLabel = $state("default");
  let keyModalValue = $state("");
  let keyModalError = $state("");

  let filteredProviders = $derived(
    catalogProviders.filter((p) => {
      const q = importFilter.toLowerCase();
      if (!q) return true;
      return (
        p.provider_id.toLowerCase().includes(q) ||
        p.display_name.toLowerCase().includes(q) ||
        p.npm.toLowerCase().includes(q)
      );
    }),
  );

  async function loadCatalog() {
    catalogLoading = true;
    catalogError = "";
    try {
      const resp = await fetch("/api/v1/admin/models-dev/search?q=", {
        credentials: "include",
      });
      if (!resp.ok) throw new Error(`加载失败 (${resp.status})`);
      catalogProviders = await resp.json();
    } catch (e: any) {
      catalogError = e.message;
    } finally {
      catalogLoading = false;
    }
  }

  function openImportDialog() {
    importFilter = "";
    lastImportResult = null;
    importError = "";
    importingId = null;
    showImport = true;
    if (catalogProviders.length === 0) {
      loadCatalog();
    }
  }

  function openKeyModal(providerId: string) {
    keyModalOpen = true;
    keyModalProviderId = providerId;
    keyModalLabel = "default";
    keyModalValue = "";
    keyModalError = "";
  }

  async function handleImport() {
    keyModalError = "";
    const apiKeys: ApiKeyEntry[] = keyModalValue.trim()
      ? [{ label: keyModalLabel || "default", key: keyModalValue, weight: 1 }]
      : [];

    importError = "";
    lastImportResult = null;
    importingId = keyModalProviderId;
    keyModalOpen = false;
    try {
      const result = await api.admin.importModelsDev({
        providerId: keyModalProviderId,
        apiKeys,
      });
      lastImportResult = result;
      loadProviders();
    } catch (e: any) {
      importError = e.message;
    } finally {
      importingId = null;
    }
  }

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
        provider_id: newProviderId,
        display_name: newDisplayName || newProviderId,
        npm: undefined,
        base_url: newBaseUrl || undefined,
        api_keys: apiKeys,
        enabled: true,
        priority: 100,
      });
      showCreate = false;
      newProviderId = "";
      newDisplayName = "";
      newBaseUrl = "";
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
        display_name: p.display_name,
        enabled: !p.enabled,
        priority: p.priority,
        api_keys: p.api_keys.map((k: any) => ({
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
        providerModelId: m.provider_model_id,
        compatibility: m.compatibility,
        displayName: m.display_name,
        description: m.description,
        maxInputTokens: m.max_input_tokens,
        maxOutputTokens: m.max_output_tokens,
        toolCalling: m.tool_calling,
        vision: m.vision,
        thinking: m.thinking,
        adaptiveThinking: m.adaptive_thinking,
        inputPricePer1m: m.input_price_per_1m,
        outputPricePer1m: m.output_price_per_1m,
        cacheReadPricePer1m: m.cache_read_price_per_1m,
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
      <!-- Import from models.dev -->
      <Dialog
        open={showImport}
        onOpenChange={(v) => {
          showImport = v;
          if (!v) {
            importFilter = "";
            lastImportResult = null;
            keyModalOpen = false;
          }
        }}
      >
        <DialogTrigger asChild>
          <Button
            variant="outline"
            class="gap-2 cursor-pointer"
            onclick={openImportDialog}
          >
            <Globe class="h-4 w-4" />
            导入提供者
          </Button>
        </DialogTrigger>
        <DialogContent class="sm:max-w-3xl max-h-[85vh] flex flex-col">
          <DialogHeader>
            <DialogTitle class="font-mono">从 models.dev 导入提供者</DialogTitle>
          </DialogHeader>

          <div class="flex flex-col gap-3 min-h-0">
            <!-- Search -->
            <div class="relative">
              <Search class="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
              <Input
                class="pl-9"
                placeholder="筛选提供者..."
                value={importFilter}
                oninput={(e) => (importFilter = e.currentTarget.value)}
              />
            </div>

            {#if catalogLoading}
              <div class="flex items-center justify-center py-16">
                <Spinner class="h-8 w-8 text-[#22C55E]" />
              </div>
            {:else if catalogError}
              <Alert class="border-destructive/30 bg-destructive/10">
                <AlertDescription class="text-destructive text-sm">{catalogError}</AlertDescription>
              </Alert>
            {:else}
              <div class="text-xs text-muted-foreground">
                {filteredProviders.length} 个提供者
              </div>
              <!-- Card grid -->
              <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3 overflow-auto min-h-0 pb-2">
                {#each filteredProviders as p (p.provider_id)}
                  <div
                    class="flex flex-col rounded-lg border border-border bg-card p-3 gap-2 transition-colors"
                  >
                    <!-- Card header -->
                    <div class="flex items-start gap-2">
                      <!-- Provider logo -->
                      <div class="h-8 w-8 rounded-lg bg-white shrink-0 flex items-center justify-center p-1">
                        <img
                          src={"https://models.dev/logos/" + p.provider_id + ".svg"}
                          alt={p.provider_id}
                          class="h-full w-full object-contain"
                          onerror={(e) => {
                            (e.target as HTMLImageElement).style.display = "none";
                          }}
                        />
                      </div>
                      <div class="flex flex-col gap-0.5 min-w-0 flex-1">
                        <div class="flex items-center gap-1.5">
                          <Globe class="h-3.5 w-3.5 text-[#22C55E] shrink-0 hidden" />
                          <span class="font-mono font-semibold text-foreground text-sm truncate">
                            {p.provider_id}
                          </span>
                        </div>
                        <span class="text-xs text-muted-foreground truncate">{p.display_name}</span>
                      </div>
                      <Badge variant="secondary" class="text-xs shrink-0">
                        {p.model_count} 模型
                      </Badge>
                    </div>

                    <!-- npm / doc -->
                    <div class="flex items-center gap-2 text-xs text-muted-foreground">
                      <span class="font-mono truncate">{p.npm}</span>
                      {#if p.doc}
                        <a
                          href={p.doc}
                          target="_blank"
                          rel="noopener noreferrer"
                          class="text-[#22C55E] hover:underline shrink-0 ml-auto"
                        >
                          文档
                        </a>
                      {/if}
                    </div>

                    <!-- Divider -->
                    <div class="border-t border-border -mx-1"></div>

                    <Button
                      size="sm"
                      variant="outline"
                      class="w-full cursor-pointer"
                      disabled={importingId !== null}
                      onclick={() => openKeyModal(p.provider_id)}
                    >
                      导入
                    </Button>
                  </div>
                {/each}
              </div>

              {#if filteredProviders.length === 0 && importFilter.trim()}
                <div class="flex items-center justify-center py-8 text-sm text-muted-foreground">
                  无匹配的提供者
                </div>
              {/if}
            {/if}

            {#if importError}
              <Alert class="border-destructive/30 bg-destructive/10 shrink-0">
                <AlertDescription class="text-destructive text-sm">{importError}</AlertDescription>
              </Alert>
            {/if}
            {#if lastImportResult}
              <Alert class="border-[#22C55E]/30 bg-[#22C55E]/10 shrink-0">
                <AlertDescription class="text-sm">
                  成功导入 <span class="font-mono font-medium">{lastImportResult.provider_id}</span
                  >，包含 {lastImportResult.imported_models.length} 个模型
                </AlertDescription>
              </Alert>
            {/if}
          </div>

          <!-- Key-entry modal (nested inside main import dialog) -->
          <Dialog open={keyModalOpen} onOpenChange={(v) => (keyModalOpen = v)}>
            <DialogContent class="sm:max-w-sm">
              <DialogHeader>
                <DialogTitle class="font-mono text-sm">
                  导入 {keyModalProviderId}
                </DialogTitle>
              </DialogHeader>
              <div class="flex flex-col gap-3">
                <div class="flex flex-col gap-2">
                  <Label for="kml">API Key 标签</Label>
                  <Input id="kml" placeholder="default" bind:value={keyModalLabel} />
                </div>
                <div class="flex flex-col gap-2">
                  <Label for="kmv">API Key</Label>
                  <Input
                    id="kmv"
                    type="password"
                    placeholder="sk-..."
                    bind:value={keyModalValue}
                  />
                </div>
                {#if keyModalError}
                  <Alert class="border-destructive/30 bg-destructive/10">
                    <AlertDescription class="text-destructive text-xs">{keyModalError}</AlertDescription>
                  </Alert>
                {/if}
                <div class="flex gap-2">
                  <Button
                    variant="outline"
                    class="flex-1 cursor-pointer"
                    onclick={() => (keyModalOpen = false)}
                  >
                    取消
                  </Button>
                  <Button
                    class="flex-1 bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium cursor-pointer"
                    onclick={handleImport}
                  >
                    确认导入
                  </Button>
                </div>
              </div>
            </DialogContent>
          </Dialog>
        </DialogContent>
      </Dialog>

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
              <Label for="bu">Base URL（可选）</Label>
              <Input
                id="bu"
                placeholder="https://api.openai.com/v1"
                bind:value={newBaseUrl}
              />
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
                <span class="font-mono font-medium text-foreground">{p.provider_id}</span>
                <Badge
                  variant={p.enabled ? "default" : "secondary"}
                  class="text-xs"
                >
                  {p.enabled ? "启用" : "禁用"}
                </Badge>
              </div>
              <div class="flex gap-3 text-xs text-muted-foreground mt-0.5">
                <span>{p.display_name}</span>
                <span>{p.model_count} 个模型</span>
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
                onclick={() => openDeleteDialog("provider", p.id, p.display_name || p.provider_id)}
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
                        <span class="font-mono text-foreground truncate">{m.model_name}</span>
                        <span class="text-muted-foreground text-xs shrink-0"
                          >→ {m.provider_model_id}</span
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
                              p.display_name || p.provider_id,
                              m.id,
                              m.model_name,
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
