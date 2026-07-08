<script lang="ts">
  import { getApi, formatTime, formatQuotaPeriod } from "$lib/api";
  import { QUOTA_PERIOD_OPTIONS, quotaPeriodLabel, SKELETON_ROWS } from "$lib/constants";
  import { auth } from "$lib/stores/auth.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";
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
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Skeleton } from "$lib/components/ui/skeleton/index.js";
  import { Plus, Trash2, Key, Copy, Check } from "@lucide/svelte";
  import type { TokenListItem } from "$bindings/TokenListItem";
  import type { CreateTokenResponse } from "$bindings/CreateTokenResponse";

  const api = getApi();

  let tokens = $state<TokenListItem[]>([]);
  let loading = $state(true);
  let error = $state("");

  // Create dialog state
  let showCreate = $state(false);
  let newName = $state("");
  let newRequestQuota = $state(0);
  let newTokenQuota = $state(0);
  let newQuotaPeriod = $state("unlimited");
  let createdToken = $state<CreateTokenResponse | null>(null);
  let tokenCopied = $state(false);

  // Load tokens
  async function loadTokens() {
    loading = true;
    error = "";
    try {
      tokens = await api.tokens.listTokens();
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (auth.isAuthenticated) loadTokens();
  });

  // Create token
  async function handleCreate() {
    error = "";
    try {
      const result = await api.tokens.createToken({
        name: newName,
        allowedModels: [],
        requestQuota: newRequestQuota,
        tokenQuota: newTokenQuota,
        quotaPeriod: newQuotaPeriod,
      });
      createdToken = result;
      tokenCopied = false;
      loadTokens();
    } catch (e: any) {
      error = e.message;
    }
  }

  async function copyToken() {
    if (createdToken) {
      await navigator.clipboard.writeText(createdToken.token);
      tokenCopied = true;
    }
  }

  function closeCreate() {
    showCreate = false;
    newName = "";
    newRequestQuota = 0;
    newTokenQuota = 0;
    newQuotaPeriod = "unlimited";
    createdToken = null;
    tokenCopied = false;
  }

  // Delete token
  async function handleDelete(id: number) {
    error = "";
    try {
      await api.tokens.deleteToken(String(id));
      loadTokens();
    } catch (e: any) {
      error = e.message;
    }
  }

  // Toggle token active
  async function handleToggle(t: TokenListItem) {
    error = "";
    try {
      await api.tokens.updateToken(String(t.id), { active: !t.active });
      loadTokens();
    } catch (e: any) {
      error = e.message;
    }
  }

  function quotaLabel(t: TokenListItem): string {
    const parts: string[] = [];
    if (t.requestQuota > 0) parts.push(`${t.requestQuota} 次请求`);
    if (t.tokenQuota > 0) parts.push(`${(t.tokenQuota / 1_000_000).toFixed(1)}M tokens`);
    if (parts.length === 0) return "不限制";
    return parts.join(" · ");
  }
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-xl font-bold font-mono text-foreground">API Token</h2>
      <p class="text-sm text-muted-foreground mt-1">管理你的 API Token，用于调用 LLM 接口</p>
    </div>
    <Dialog open={showCreate} onOpenChange={(v) => showCreate = v}>
      <DialogTrigger asChild>
        <Button class="bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium gap-2 cursor-pointer" onclick={() => showCreate = true}>
          <Plus class="h-4 w-4" />
          创建 Token
        </Button>
      </DialogTrigger>
      <DialogContent class="sm:max-w-md">
        <DialogHeader>
          <DialogTitle class="font-mono">创建新 Token</DialogTitle>
        </DialogHeader>
        {#if createdToken}
          <div class="flex flex-col gap-4">
            <Alert class="border-[#22C55E]/30 bg-[#22C55E]/10">
              <AlertDescription class="text-[#22C55E] text-sm">
                Token 创建成功！请立即复制保存，此 Token 仅显示一次。
              </AlertDescription>
            </Alert>
            <div class="flex items-center gap-2">
              <code class="flex-1 rounded-md bg-muted px-3 py-2 font-mono text-sm break-all">{createdToken.token}</code>
              <Button size="icon" variant="outline" class="cursor-pointer shrink-0" onclick={copyToken}>
                {#if tokenCopied}
                  <Check class="h-4 w-4 text-[#22C55E]" />
                {:else}
                  <Copy class="h-4 w-4" />
                {/if}
              </Button>
            </div>
            <Button variant="secondary" class="cursor-pointer" onclick={closeCreate}>关闭</Button>
          </div>
        {:else}
          <div class="flex flex-col gap-4">
            <div class="flex flex-col gap-2">
              <Label for="name">名称</Label>
              <Input id="name" placeholder="dev-machine" bind:value={newName} />
            </div>
            <div class="grid grid-cols-2 gap-3">
              <div class="flex flex-col gap-2">
                <Label for="rq">请求配额</Label>
                <Input id="rq" type="number" placeholder="0 = 不限制" bind:value={newRequestQuota} />
              </div>
              <div class="flex flex-col gap-2">
                <Label for="tq">Token 配额</Label>
                <Input id="tq" type="number" placeholder="0 = 不限制" bind:value={newTokenQuota} />
              </div>
            </div>
            <div class="flex flex-col gap-2">
              <Label>配额周期</Label>
              <Select type="single" value={newQuotaPeriod} onValueChange={(v) => newQuotaPeriod = v ?? "unlimited"}>
                <SelectTrigger>
                  <span class="text-sm">{quotaPeriodLabel(newQuotaPeriod)}</span>
                </SelectTrigger>
                <SelectContent>
                  {#each QUOTA_PERIOD_OPTIONS as opt}
                    <SelectItem value={opt.value}>{opt.label}</SelectItem>
                  {/each}
                </SelectContent>
              </Select>
            </div>
            <Button class="bg-[#22C55E] hover:bg-[#16A34A] text-black font-medium cursor-pointer" onclick={handleCreate} disabled={!newName.trim()}>
              创建
            </Button>
          </div>
        {/if}
      </DialogContent>
    </Dialog>
  </div>

  <!-- Error -->
  {#if error}
    <Alert class="border-destructive/30 bg-destructive/10">
      <AlertDescription class="text-destructive text-sm">{error}</AlertDescription>
    </Alert>
  {/if}

  <!-- Token List -->
  {#if loading}
    <div class="flex flex-col gap-3">
      {#each Array(SKELETON_ROWS.tokens) as _}
        <Skeleton class="h-20 w-full rounded-lg" />
      {/each}
    </div>
  {:else if tokens.length === 0}
    <div class="flex flex-1 items-center justify-center">
      <div class="flex flex-col items-center gap-3 text-muted-foreground">
        <Key class="h-12 w-12 opacity-30" />
        <p class="text-sm">暂无 Token，点击上方按钮创建</p>
      </div>
    </div>
  {:else}
    <div class="flex flex-col gap-3 overflow-auto">
      {#each tokens as t}
        <div class="flex items-center justify-between rounded-lg border border-border bg-card p-4 transition-colors hover:border-border/80 cursor-pointer">
          <div class="flex flex-col gap-1 min-w-0">
            <div class="flex items-center gap-2">
              <span class="font-mono font-semibold text-foreground">{t.name}</span>
              <code class="text-xs text-muted-foreground font-mono">{t.tokenPrefix}</code>
              <Badge variant={t.active ? "default" : "secondary"} class="text-xs">
                {t.active ? "启用" : "禁用"}
              </Badge>
            </div>
            <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
              <span>配额: {quotaLabel(t)}</span>
              <span>周期: {formatQuotaPeriod(t.quotaPeriod)}</span>
              <span>创建: {formatTime(t.createdAt)}</span>
              {#if t.lastUsedAt}
                <span>最近使用: {formatTime(t.lastUsedAt)}</span>
              {/if}
            </div>
            {#if t.allowedModels.length > 0}
              <div class="flex flex-wrap gap-1 mt-1">
                {#each t.allowedModels as m}
                  <Badge variant="outline" class="text-xs font-mono">{m}</Badge>
                {/each}
              </div>
            {/if}
          </div>
          <div class="flex items-center gap-2 shrink-0">
            <Button size="icon" variant="ghost" class="cursor-pointer h-8 w-8" onclick={() => handleToggle(t)}>
              <Checkbox checked={t.active} class="pointer-events-none" />
            </Button>
            <Button size="icon" variant="ghost" class="cursor-pointer h-8 w-8 text-muted-foreground hover:text-destructive" onclick={() => handleDelete(t.id)}>
              <Trash2 class="h-4 w-4" />
            </Button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
