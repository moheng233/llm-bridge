<script lang="ts">
  // 模型管理 — orchestrator 容器。
  // 子组件：ModelFormDialog / ModelRow（内嵌 ModelLinkEditForm）。
  // 见 PLAN.md §10 Phase B B.4。
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
  import { Cpu, Plus } from "@lucide/svelte";
  import type { AdminModelResponse } from "$bindings/AdminModelResponse";
  import type { ProviderResponse } from "$bindings/ProviderResponse";
  import type { ModelLinkView } from "$bindings/ModelLinkView";
  import ModelFormDialog from "./admin-models/ModelFormDialog.svelte";
  import ModelRow from "./admin-models/ModelRow.svelte";

  const api = getApi();

  // ── 列表状态 ──
  let models = $state<AdminModelResponse[]>([]);
  let providers = $state<ProviderResponse[]>([]);
  let loading = $state(true);
  let error = $state("");
  let expandedId = $state<number | null>(null);
  let linksCache = $state<Map<number, ModelLinkView[]>>(new Map());
  let linksLoading = $state<Set<number>>(new Set());

  // ── 模型表单对话框 ──
  let showModelDialog = $state(false);
  let editingModel = $state<AdminModelResponse | null>(null);

  // ── 删除状态 ──
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

  // 刷新某模型的连接缓存
  async function refreshLinks(modelId: number) {
    linksLoading.add(modelId);
    linksLoading = new Set(linksLoading);
    try {
      const links = await api.admin.listModelProviders(String(modelId));
      linksCache.set(modelId, links);
      linksCache = new Map(linksCache);
      loadModels();
    } catch (e: any) {
      error = e.message;
    } finally {
      linksLoading.delete(modelId);
      linksLoading = new Set(linksLoading);
    }
  }

  function openCreateModelDialog() {
    editingModel = null;
    showModelDialog = true;
  }

  function openEditModelDialog(m: AdminModelResponse) {
    editingModel = m;
    showModelDialog = true;
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
        linksCache.delete(t.modelId);
        linksCache = new Map(linksCache);
        if (expandedId === t.modelId) expandedId = null;
        loadModels();
      } else {
        await api.admin.deleteModelProvider(String(t.modelId), String(t.linkId));
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
        <ModelRow
          model={m}
          expanded={expandedId === m.id}
          links={linksCache.get(m.id) || []}
          linksLoading={linksLoading.has(m.id)}
          {providers}
          onToggleExpand={() => toggleLinks(m.id)}
          onEditModel={() => openEditModelDialog(m)}
          onDeleteModel={() => openDeleteDialog("model", m.id, m.id, m.modelName)}
          onAddLink={() => {}}
          onEditLink={() => {}}
          onDeleteLink={(link) => openDeleteDialog("link", m.id, link.id, link.providerDisplayName)}
          onLinkSaved={() => refreshLinks(m.id)}
          onError={(e) => (error = e)}
        />
      {/each}
    </div>
  {/if}

  <!-- 模型创建/编辑对话框 -->
  <ModelFormDialog
    bind:open={showModelDialog}
    {editingModel}
    onSaved={loadModels}
    onError={(e) => (error = e)}
  />

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
