<script lang="ts">
  // 提供者创建对话框 — 从 ProvidersPage 抽出。
  // 见 PLAN.md §10 Phase B B.3。
  //
  // 父组件用法：
  //   <ProviderCreateDialog onCreated={loadProviders} onError={(e) => error = e} />

  import { getApi } from "$lib/api";
  import {
    PROTOCOL_OPTIONS,
    QUOTA_ADAPTER_OPTIONS,
    QUOTA_ADAPTER_NONE,
    quotaAdapterFromSelect,
    quotaAdapterToSelect,
  } from "$lib/constants";
  import { emptyProtocol, buildQuotaConfigString } from "$lib/utils/provider";
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
  import { Plus, Trash2 } from "@lucide/svelte";
  import type { ApiKeyEntry } from "$bindings/ApiKeyEntry";
  import type { ProtocolInput } from "$bindings/ProtocolInput";
  import type { ProviderQuotaAdapter } from "$bindings/ProviderQuotaAdapter";
  import ProtocolEditForm from "./ProtocolEditForm.svelte";

  const api = getApi();

  let {
    onCreated,
    onError,
  }: {
    onCreated: () => void;
    onError: (e: string) => void;
  } = $props();

  // ── 对话框状态 ──
  let showCreate = $state(false);
  let newProviderId = $state("");
  let newDisplayName = $state("");
  let newApiKeyLabel = $state("");
  let newApiKeyValue = $state("");
  let newProtocols = $state<ProtocolInput[]>([]);
  let newProtocolDraft = $state<ProtocolInput | null>(null);
  let newQuotaAdapter = $state<ProviderQuotaAdapter | null>(null);
  let newQuotaBaseUrl = $state("");
  let newQuotaKeyLabelFilter = $state("");

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
  }

  async function handleCreate() {
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
      resetCreateForm();
      onCreated();
    } catch (e: any) {
      onError(e.message);
    }
  }

  // ── 协议 draft helpers ──
  function addNewProtocolToCreate() {
    newProtocolDraft = emptyProtocol();
  }

  function confirmNewProtocolToCreate() {
    if (!newProtocolDraft) return;
    if (!newProtocolDraft.baseUrl.trim()) {
      onError("协议端点 URL 必填");
      return;
    }
    newProtocols = [...newProtocols, newProtocolDraft];
    newProtocolDraft = null;
  }

  function cancelNewProtocolToCreate() {
    newProtocolDraft = null;
  }

  function removeNewProtocolFromCreate(index: number) {
    newProtocols = newProtocols.filter((_, i) => i !== index);
  }
</script>

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
          <ProtocolEditForm
            bind:draft={newProtocolDraft}
            title="新建协议"
            confirmText="加入列表"
            onConfirm={confirmNewProtocolToCreate}
            onCancel={cancelNewProtocolToCreate}
          />
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
