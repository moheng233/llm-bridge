<script lang="ts">
  import { createApiClient } from "$bindings/client";
  import type { ProviderResponse, CreateProviderRequest, UpdateProviderRequest } from "$bindings";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";
  import { Separator } from "$lib/components/ui/separator/index.js";

  const api = createApiClient({ baseUrl: "", credentials: "include" });

  let providers = $state<ProviderResponse[]>([]);
  let loading = $state(true);
  let error = $state("");
  let dialogOpen = $state(false);
  let editing = $state<string | null>(null);
  let formError = $state("");

  let form = $state<CreateProviderRequest>({
    providerName: "",
    providerType: "openai",
    baseUrl: "",
    apiKey: "",
  });

  async function load() {
    loading = true;
    error = "";
    try {
      providers = await api.providers.listProviders();
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  $effect(() => { load(); });

  function resetForm() {
    form = { providerName: "", providerType: "openai", baseUrl: "", apiKey: "" };
    dialogOpen = false;
    editing = null;
    formError = "";
  }

  function startEdit(p: ProviderResponse) {
    editing = p.provider_name;
    form = {
      providerName: p.provider_name,
      providerType: p.provider_type,
      baseUrl: p.base_url ?? "",
      apiKey: "",
    };
    dialogOpen = true;
    formError = "";
  }

  async function handleSubmit() {
    formError = "";
    try {
      if (editing) {
        await api.providers.updateProvider(editing, {
          providerType: form.providerType,
          baseUrl: form.baseUrl || null,
          apiKey: form.apiKey,
        });
      } else {
        await api.providers.createProvider({
          ...form,
          baseUrl: form.baseUrl || null,
        });
      }
      resetForm();
      await load();
    } catch (e: any) {
      formError = e.message;
    }
  }

  async function handleDelete(name: string) {
    try {
      await api.providers.deleteProvider(name);
      await load();
    } catch (e: any) {
      error = e.message;
    }
  }

  const typeLabels: Record<string, string> = { openai: "OpenAI", anthropic: "Anthropic", gemini: "Gemini" };
  const typeBadgeVariant: Record<string, string> = {
    openai: "bg-emerald-100 text-emerald-700 hover:bg-emerald-100",
    anthropic: "bg-orange-100 text-orange-700 hover:bg-orange-100",
    gemini: "bg-sky-100 text-sky-700 hover:bg-sky-100",
  };

  const providerTypeOptions = [
    { value: "openai", label: "OpenAI" },
    { value: "anthropic", label: "Anthropic" },
    { value: "gemini", label: "Gemini" },
  ];
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
  <section
    class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border bg-background shadow-sm"
  >
    <div
      class="flex shrink-0 flex-col gap-3 border-b px-4 py-3 lg:flex-row lg:items-center lg:justify-between"
    >
      <div class="flex items-center gap-3">
        <h2 class="text-xl font-semibold">提供者管理</h2>
        <Button onclick={() => { resetForm(); dialogOpen = true; }}>
          + 新建提供者
        </Button>
      </div>
    </div>

    <Dialog.Root bind:open={dialogOpen}>
      <Dialog.Content class="sm:max-w-lg">
        <Dialog.Header>
          <Dialog.Title>{editing ? "编辑提供者" : "新建提供者"}</Dialog.Title>
          <Dialog.Description>
            {editing ? "修改提供者的配置信息" : "添加一个新的 API 提供者"}
          </Dialog.Description>
        </Dialog.Header>
        {#if formError}
          <Alert variant="destructive">
            <AlertDescription>{formError}</AlertDescription>
          </Alert>
        {/if}
        <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }} class="space-y-4">
          <div class="grid grid-cols-2 gap-4">
            <div class="space-y-2">
              <Label for="provider-name">名称</Label>
              <Input id="provider-name" bind:value={form.providerName} disabled={!!editing} required />
            </div>
            <div class="space-y-2">
              <Label for="provider-type">协议类型</Label>
              <Select.Root type="single" bind:value={form.providerType}>
                <Select.Trigger class="w-full">
                  {providerTypeOptions.find((o) => o.value === form.providerType)?.label ?? "选择类型"}
                </Select.Trigger>
                <Select.Content>
                  {#each providerTypeOptions as opt}
                    <Select.Item value={opt.value}>{opt.label}</Select.Item>
                  {/each}
                </Select.Content>
              </Select.Root>
            </div>
            <div class="col-span-2 space-y-2">
              <Label for="base-url">自定义 Base URL（可选）</Label>
              <Input id="base-url" bind:value={form.baseUrl} placeholder="留空使用默认地址" />
            </div>
            <div class="col-span-2 space-y-2">
              <Label for="api-key">API Key</Label>
              <Input id="api-key" type="password" bind:value={form.apiKey} required />
            </div>
          </div>
          <Dialog.Footer>
            <Button type="button" variant="outline" onclick={resetForm}>取消</Button>
            <Button type="submit">{editing ? "保存修改" : "创建"}</Button>
          </Dialog.Footer>
        </form>
      </Dialog.Content>
    </Dialog.Root>

    <div class="min-h-0 flex-1 overflow-auto [scrollbar-gutter:stable]">
      {#if loading}
        <div class="flex items-center justify-center py-16">
          <Spinner class="size-8" />
        </div>
      {:else if error}
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      {:else if providers.length === 0}
        <div class="rounded-lg border border-dashed py-12 text-center text-sm text-muted-foreground">
          暂无提供者，点击上方按钮创建
        </div>
      {:else}
        <div class="grid grid-cols-1 gap-4 p-4 md:grid-cols-2">
          {#each providers as p, rowIndex}
            <Card.Root style="animation: row-enter 220ms ease both; animation-delay: {Math.min(rowIndex * 20, 480)}ms;">
              <Card.Header>
                <div class="flex items-start justify-between">
                  <div>
                    <Card.Title class="text-base">{p.provider_name}</Card.Title>
                    <Badge class="mt-1 {typeBadgeVariant[p.provider_type] ?? ''}">
                      {typeLabels[p.provider_type] ?? p.provider_type}
                    </Badge>
                  </div>
                  <div class="flex gap-1">
                    <Button variant="ghost" size="sm" onclick={() => startEdit(p)}>✏️</Button>
                    <Button variant="ghost" size="sm" onclick={() => handleDelete(p.provider_name)}>🗑️</Button>
                  </div>
                </div>
              </Card.Header>
              <Card.Content>
                <dl class="space-y-1.5 text-xs text-muted-foreground">
                  {#if p.base_url}
                    <div class="flex gap-2">
                      <dt class="shrink-0 font-medium">URL</dt>
                      <dd class="truncate font-mono">{p.base_url}</dd>
                    </div>
                  {/if}
                  <div class="flex gap-2">
                    <dt class="shrink-0 font-medium">密钥</dt>
                    <dd class="font-mono">已配置</dd>
                  </div>
                </dl>
              </Card.Content>
            </Card.Root>
          {/each}
        </div>
      {/if}
    </div>
  </section>
</div>

<style>
  @keyframes row-enter {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
