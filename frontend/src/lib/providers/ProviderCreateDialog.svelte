<script lang="ts">
  // 提供者创建/编辑对话框 — 从 ProvidersPage 抽出。
  // 通过 `provider` prop 区分模式：
  //   - provider = null  → 新建模式（带「添加自定义提供者」触发按钮）
  //   - provider !== null → 编辑模式（外部触发，通过 onUpdated 通知刷新）
  //
  // 编辑模式下，providerId 只读，其余字段预填现有值。
  // API Key 列表式管理：编辑时空 key 框 = 保留原值的占位（提交时由后端处理）。
  // 协议也在本弹窗中集中编辑，ProviderRow 不再带协议编辑入口。

  import { getApi } from "$lib/api";
  import {
    QUOTA_ADAPTER_OPTIONS,
    QUOTA_ADAPTER_NONE,
    quotaAdapterFromSelect,
    quotaAdapterToSelect,
  } from "$lib/constants";
  import {
    emptyProtocol,
    protocolViewToInput,
    buildQuotaConfigString,
    parseQuotaConfigString,
  } from "$lib/utils/provider";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
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
  import { Plus, Trash2, Pencil } from "@lucide/svelte";
  import type { ApiKeyEntry } from "$bindings/ApiKeyEntry";
  import type { ProviderResponse } from "$bindings/ProviderResponse";
  import type { ProtocolInput } from "$bindings/ProtocolInput";
  import type { ProviderQuotaAdapter } from "$bindings/ProviderQuotaAdapter";
  import ProtocolEditForm from "./ProtocolEditForm.svelte";

  const api = getApi();

  // 编辑模式：通过 `open` prop 双向绑定，由父组件控制打开/关闭
  let {
    provider = null as ProviderResponse | null,
    openExternal = $bindable(false),
    onCreated,
    onUpdated,
    onError,
  }: {
    provider?: ProviderResponse | null;
    openExternal?: boolean;
    onCreated?: () => void;
    onUpdated?: () => void;
    onError?: (e: string) => void;
  } = $props();

  // ── 模式判定 ──
  const isEdit = $derived(provider !== null);

  // ── 对话框状态 ──
  let showCreate = $state(false);  // 新建模式自持

  // ── 表单字段 ──
  let providerId = $state("");
  let displayName = $state("");
  let apiKeys = $state<ApiKeyEntry[]>([]);
  let protocols = $state<ProtocolInput[]>([]);
  let protocolDraft = $state<ProtocolInput | null>(null);
  let protocolDraftIndex = $state<number | null>(null);
  let enabled = $state(true);
  let priority = $state<number>(100);
  let quotaAdapter = $state<ProviderQuotaAdapter | null>(null);
  let quotaBaseUrl = $state("");
  let quotaKeyLabelFilter = $state("");

  // 打开新建时重置；打开编辑时预填
  function syncFormFromProvider() {
    if (provider) {
      providerId = provider.providerId;
      displayName = provider.displayName;
      apiKeys = provider.apiKeys.map((k) => ({
        label: k.label,
        // 编辑模式不回填明文 key，留空表示「不修改」
        key: "",
        weight: k.weight,
      }));
      protocols = provider.protocols.map(protocolViewToInput);
      enabled = provider.enabled;
      priority = provider.priority;
      quotaAdapter = provider.quotaAdapter;
      const cfg = provider.quotaAdapterConfig
        ? parseQuotaConfigString(provider.quotaAdapterConfig)
        : null;
      quotaBaseUrl = cfg?.baseUrl ?? "";
      quotaKeyLabelFilter = cfg?.keyLabelFilter ?? "";
    } else {
      resetForm();
    }
  }

  function resetForm() {
    providerId = "";
    displayName = "";
    apiKeys = [];
    protocols = [];
    protocolDraft = null;
    protocolDraftIndex = null;
    enabled = true;
    priority = 100;
    quotaAdapter = null;
    quotaBaseUrl = "";
    quotaKeyLabelFilter = "";
  }

  // 编辑模式：openExternal 由 false → true 时预填表单
  // 直接通过 bind:open={openExternal} 让父组件控制开关，无需内部 open 状态。
  let prevOpenExternal = openExternal;
  $effect(() => {
    if (openExternal && !prevOpenExternal) {
      syncFormFromProvider();
    }
    prevOpenExternal = openExternal;
  });

  // ── API Key 列表操作 ──
  function addApiKey() {
    apiKeys = [...apiKeys, { label: `key-${apiKeys.length + 1}`, key: "", weight: 1 }];
  }

  function removeApiKey(index: number) {
    apiKeys = apiKeys.filter((_, i) => i !== index);
  }

  // ── 协议 draft helpers ──
  function openProtocolEditor(index?: number) {
    if (index !== undefined && protocols[index]) {
      protocolDraft = { ...protocols[index] };
      protocolDraftIndex = index;
    } else {
      protocolDraft = emptyProtocol();
      protocolDraftIndex = null;
    }
  }

  function confirmProtocolDraft() {
    if (!protocolDraft) return;
    if (!protocolDraft.baseUrl.trim()) {
      onError?.("协议端点 URL 必填");
      return;
    }
    if (protocolDraftIndex !== null && protocols[protocolDraftIndex]) {
      const list = [...protocols];
      list[protocolDraftIndex] = protocolDraft;
      protocols = list;
    } else {
      protocols = [...protocols, protocolDraft];
    }
    protocolDraft = null;
    protocolDraftIndex = null;
  }

  function cancelProtocolDraft() {
    protocolDraft = null;
    protocolDraftIndex = null;
  }

  function removeProtocol(index: number) {
    protocols = protocols.filter((_, i) => i !== index);
  }

  // ── 提交 ──
  async function handleSubmit() {
    if (!isEdit && !providerId.trim()) {
      onError?.("提供者 ID 必填");
      return;
    }

    // 过滤掉 label 和 key 都为空的条目
    const cleanedKeys: ApiKeyEntry[] = apiKeys.filter(
      (k) => k.label.trim() !== "" || k.key.trim() !== "",
    );

    const quotaAdapterConfig = buildQuotaConfigString(quotaBaseUrl, quotaKeyLabelFilter);

    try {
      if (isEdit && provider) {
        await api.admin.updateProvider(String(provider.id), {
          displayName: displayName.trim() || provider.providerId,
          apiKeys: cleanedKeys,
          protocols,
          enabled,
          priority,
          quotaAdapter,
          quotaAdapterConfig,
        });
        onUpdated?.();
        openExternal = false;
      } else {
        await api.admin.createProvider({
          providerId: providerId.trim(),
          displayName: displayName.trim() || providerId.trim(),
          apiKeys: cleanedKeys,
          protocols,
          enabled,
          priority,
          quotaAdapter,
          quotaAdapterConfig,
        });
        onCreated?.();
        showCreate = false;
        resetForm();
      }
    } catch (e: any) {
      onError?.(e.message);
    }
  }

  const canSubmit = $derived(isEdit ? true : providerId.trim() !== "");
</script>

{#if isEdit}
  <!-- 编辑模式：弹窗由父组件通过 `openExternal` 控制，本组件不渲染触发按钮 -->
  <Dialog
    open={openExternal}
    onOpenChange={(v) => { openExternal = v; if (!v) resetForm(); }}
  >
    <DialogContent class="sm:max-w-4xl">
      <DialogHeader>
        <DialogTitle class="font-mono">编辑提供者 · {providerId}</DialogTitle>
      </DialogHeader>
      <div class="grid grid-cols-1 gap-x-5 gap-y-3 max-h-[80vh] overflow-y-auto pr-1 lg:grid-cols-12">
        {@render formInner()}
      </div>
    </DialogContent>
  </Dialog>
{:else}
  <!-- 新建模式：自带触发按钮 -->
  <Dialog
    open={showCreate}
    onOpenChange={(v) => { if (!v) resetForm(); showCreate = v; }}
  >
    <DialogTrigger asChild>
      <Button
        class="bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium gap-2 cursor-pointer"
        onclick={() => { resetForm(); showCreate = true; }}
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
        {@render formInner()}
      </div>
    </DialogContent>
  </Dialog>
{/if}

{#snippet formInner()}
  <!-- 左列：基本属性 + API Key 列表 + 额度适配器 -->
  <div class="flex flex-col gap-3 lg:col-span-7">
    <div class="flex flex-col gap-2">
      <Label for="pid">提供者 ID</Label>
      <Input
        id="pid"
        placeholder="openai"
        bind:value={providerId}
        disabled={isEdit}
        class={isEdit ? "font-mono opacity-60" : ""}
      />
      {#if isEdit}
        <p class="text-xs text-muted-foreground">提供者 ID 创建后不可修改</p>
      {/if}
    </div>
    <div class="flex flex-col gap-2">
      <Label for="dn">显示名称</Label>
      <Input id="dn" placeholder="OpenAI" bind:value={displayName} />
    </div>

    <!-- API Key 列表（列表式管理） -->
    <div class="flex flex-col gap-2 pt-2 border-t border-border mt-1">
      <div class="flex items-center justify-between">
        <Label>API Keys</Label>
        <Button
          type="button"
          size="sm"
          variant="outline"
          class="h-7 gap-1 cursor-pointer"
          onclick={addApiKey}
        >
          <Plus class="h-3 w-3" />
          添加 Key
        </Button>
      </div>
      <p class="text-xs text-muted-foreground -mt-1">
        {#if isEdit}
          编辑模式下留空 key 框 = 保留原值；删除条目会移除该 key。
        {:else}
          每个 Key 可带 label 和权重，用于多 Key 轮询。
        {/if}
      </p>
      {#if apiKeys.length === 0}
        <p class="text-xs text-muted-foreground italic py-1">
          {#if isEdit}该提供者暂无 API Key，请添加{:else}暂未配置 Key，可稍后在编辑界面补充{/if}
        </p>
      {:else}
        <div class="flex flex-col gap-2">
          {#each apiKeys as k, i}
            <div class="grid grid-cols-12 gap-2 items-center">
              <Input
                placeholder="label"
                bind:value={k.label}
                class="col-span-4 h-9 text-sm"
              />
              <Input
                type="password"
                placeholder={isEdit ? "留空保留原值" : "sk-..."}
                bind:value={k.key}
                class="col-span-5 h-9 text-sm font-mono"
              />
              <Input
                type="number"
                placeholder="权重"
                bind:value={k.weight}
                class="col-span-2 h-9 text-sm"
              />
              <Button
                type="button"
                size="icon"
                variant="ghost"
                class="col-span-1 h-9 w-9 text-muted-foreground hover:text-destructive cursor-pointer"
                onclick={() => removeApiKey(i)}
                aria-label="删除 Key"
              >
                <Trash2 class="h-4 w-4" />
              </Button>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- 额度适配器配置（可选）-->
    <div class="flex flex-col gap-2 pt-2 border-t border-border mt-1">
      <Label>额度适配器（可选）</Label>
      <p class="text-xs text-muted-foreground -mt-1">
        声明该提供者使用的上游额度查询协议，用于在后台实时查询每个 API Key 的剩余额度。
      </p>
      <Select
        type="single"
        value={quotaAdapterToSelect(quotaAdapter)}
        onValueChange={(v) => (quotaAdapter = quotaAdapterFromSelect(v ?? QUOTA_ADAPTER_NONE))}
      >
        <SelectTrigger class="cursor-pointer">
          <span class="text-sm">{QUOTA_ADAPTER_OPTIONS.find((o) => o.value === quotaAdapterToSelect(quotaAdapter))?.label ?? "不查询上游额度"}</span>
        </SelectTrigger>
        <SelectContent>
          {#each QUOTA_ADAPTER_OPTIONS as opt}
            <SelectItem value={opt.value}>{opt.label}</SelectItem>
          {/each}
        </SelectContent>
      </Select>
      {#if quotaAdapter}
        <div class="flex flex-col gap-2 pl-2 border-l-2 border-border">
          <div class="flex flex-col gap-1">
            <Label for="qa-url" class="text-xs">覆盖端点 URL（可选）</Label>
            <Input
              id="qa-url"
              placeholder="留空使用适配器默认值，如 https://api.code.umans.ai/v1/usage"
              bind:value={quotaBaseUrl}
              class="h-9 text-sm font-mono"
            />
          </div>
          <div class="flex flex-col gap-1">
            <Label for="qa-klf" class="text-xs">仅查询该 label 的 Key（可选）</Label>
            <Input
              id="qa-klf"
              placeholder="留空 = 查询全部 Key"
              bind:value={quotaKeyLabelFilter}
              class="h-9 text-sm"
            />
          </div>
        </div>
      {/if}
    </div>
  </div>

  <!-- 右列：协议配置 -->
  <div class="flex flex-col gap-2 pt-2 border-t border-border mt-1 lg:col-span-5 lg:pt-0 lg:border-t-0 lg:border-l lg:pl-5 lg:mt-0">
    <div class="flex items-center justify-between">
      <Label>协议配置（可选）</Label>
      {#if !protocolDraft}
        <Button
          type="button"
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
    {#if protocols.length === 0 && !protocolDraft}
      <p class="text-xs text-muted-foreground">
        创建后可再补；空配置启动也支持。
      </p>
    {/if}
    {#each protocols as p, i}
      <div class="flex items-center justify-between rounded-md border border-border bg-muted/30 px-2 py-1.5 text-xs">
        <div class="flex flex-col min-w-0">
          <span class="font-mono">{p.protocol}</span>
          <span class="text-muted-foreground truncate">{p.baseUrl}</span>
        </div>
        <div class="flex items-center gap-1 shrink-0">
          <Button
            type="button"
            size="icon"
            variant="ghost"
            class="h-6 w-6 text-muted-foreground hover:text-foreground cursor-pointer"
            onclick={() => openProtocolEditor(i)}
            aria-label="编辑协议"
          >
            <Pencil class="h-3 w-3" />
          </Button>
          <Button
            type="button"
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
    {#if protocolDraft}
      <ProtocolEditForm
        bind:draft={protocolDraft}
        title={protocolDraftIndex !== null ? "编辑协议" : "新建协议"}
        confirmText="保存"
        onConfirm={confirmProtocolDraft}
        onCancel={cancelProtocolDraft}
      />
    {/if}
  </div>

  <!-- 状态 + 优先级 -->
  <div class="flex items-end gap-4 lg:col-span-12 pt-2 border-t border-border">
    <div class="flex items-center gap-2">
      <Label for="en">启用</Label>
      <input
        id="en"
        type="checkbox"
        bind:checked={enabled}
        class="size-4 cursor-pointer"
      />
    </div>
    <div class="flex flex-col gap-1 w-32">
      <Label for="pr">优先级</Label>
      <Input id="pr" type="number" bind:value={priority} class="h-9" />
    </div>
  </div>

  <Button
    class="bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium cursor-pointer lg:col-span-12"
    onclick={handleSubmit}
    disabled={!canSubmit}
  >
    {isEdit ? "保存修改" : "创建"}
  </Button>
{/snippet}
