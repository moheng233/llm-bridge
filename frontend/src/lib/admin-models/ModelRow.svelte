<script lang="ts">
  // 单个模型展开行 — 从 AdminModelsPage 抽出。
  // 见 PLAN.md §10 Phase B B.4。
  //
  // 父组件用法：
  //   <ModelRow
  //     model={m}
  //     expanded={expandedId === m.id}
  //     links={linksCache.get(m.id) || []}
  //     linksLoading={linksLoading.has(m.id)}
  //     providers={providers}
  //     onToggleExpand={() => toggleLinks(m.id)}
  //     onEditModel={() => openEditModelDialog(m)}
  //     onDeleteModel={() => openDeleteDialog("model", m.id, m.id, m.modelName)}
  //     onAddLink={() => openCreateLink(m.id, m.modelName)}
  //     onEditLink={(link) => openEditLink(m.id, link)}
  //     onDeleteLink={(link) => openDeleteDialog("link", m.id, link.id, link.providerDisplayName)}
  //     onLinkSaved={() => refreshLinks(m.id)}
  //     onError={(e) => error = e}
  //   />

  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { ChevronDown, ChevronRight, Pencil, Trash2, Link2 } from "@lucide/svelte";
  import type { AdminModelResponse } from "$bindings/AdminModelResponse";
  import type { ModelLinkView } from "$bindings/ModelLinkView";
  import type { ProviderResponse } from "$bindings/ProviderResponse";
  import ModelLinkEditForm from "./ModelLinkEditForm.svelte";

  let {
    model: m,
    expanded,
    links = [],
    linksLoading = false,
    providers,
    onToggleExpand,
    onEditModel,
    onDeleteModel,
    onAddLink,
    onEditLink,
    onDeleteLink,
    onLinkSaved,
    onError,
  }: {
    model: AdminModelResponse;
    expanded: boolean;
    links: ModelLinkView[];
    linksLoading: boolean;
    providers: ProviderResponse[];
    onToggleExpand: () => void;
    onEditModel: () => void;
    onDeleteModel: () => void;
    onAddLink: () => void;
    onEditLink: (link: ModelLinkView) => void;
    onDeleteLink: (link: ModelLinkView) => void;
    onLinkSaved: () => void;
    onError: (e: string) => void;
  } = $props();

  // 本行内连接编辑状态
  let editingLink = $state<ModelLinkView | null | undefined>(undefined);
  // undefined = 未激活, null = 新建, ModelLinkView = 编辑

  function startCreate() {
    editingLink = null;
    onAddLink();
  }

  function startEdit(link: ModelLinkView) {
    editingLink = link;
    onEditLink(link);
  }

  function closeLinkForm() {
    editingLink = undefined;
  }

  function handleSaved() {
    editingLink = undefined;
    onLinkSaved();
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
        onclick={(e) => { e.stopPropagation(); onEditModel(); }}
        aria-label="编辑模型"
      >
        <Pencil class="h-3.5 w-3.5" />
      </Button>
      <Button
        size="icon"
        variant="ghost"
        class="h-7 w-7 text-muted-foreground hover:text-destructive cursor-pointer"
        onclick={(e) => { e.stopPropagation(); onDeleteModel(); }}
        aria-label="删除模型"
      >
        <Trash2 class="h-3.5 w-3.5" />
      </Button>
    </div>
  </button>
  {#if expanded}
    <div class="border-t border-border px-4 py-3 flex flex-col gap-3">
      <div class="flex items-center justify-between">
        <span class="text-xs font-medium text-muted-foreground uppercase tracking-wide">
          提供者连接
        </span>
        {#if editingLink === undefined}
          <Button
            size="sm"
            variant="outline"
            class="h-7 gap-1 cursor-pointer"
            onclick={startCreate}
          >
            <Link2 class="h-3 w-3" />
            添加连接
          </Button>
        {/if}
      </div>

      {#if linksLoading}
        <div class="flex items-center gap-2 text-sm text-muted-foreground py-2">
          <Spinner class="h-4 w-4" />
          加载连接...
        </div>
      {:else if links.length === 0 && editingLink === undefined}
        <div class="flex items-center gap-2 text-sm text-muted-foreground py-2 italic">
          <Link2 class="h-4 w-4" />
          暂无连接 — 该模型尚未关联任何提供者
        </div>
      {:else}
        <div class="flex flex-col gap-1.5">
          {#each links as link}
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
                  onclick={() => startEdit(link)}
                  aria-label="编辑连接"
                >
                  <Pencil class="h-3 w-3" />
                </Button>
                <Button
                  size="icon"
                  variant="ghost"
                  class="h-6 w-6 text-muted-foreground hover:text-destructive cursor-pointer"
                  onclick={() => onDeleteLink(link)}
                  aria-label="删除连接"
                >
                  <Trash2 class="h-3 w-3" />
                </Button>
              </div>
            </div>
          {/each}
        </div>
      {/if}

      {#if editingLink !== undefined}
        <ModelLinkEditForm
          modelId={m.id}
          modelName={m.modelName}
          {providers}
          currentModel={m}
          {editingLink}
          onSaved={handleSaved}
          onCancel={closeLinkForm}
          {onError}
        />
      {/if}
    </div>
  {/if}
</div>
