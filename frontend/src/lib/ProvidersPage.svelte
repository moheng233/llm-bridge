<script lang="ts">
  import { api, type Provider, type CreateProviderRequest } from "./api";
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

  let providers = $state<Provider[]>([]);
  let loading = $state(true);
  let error = $state("");
  let dialogOpen = $state(false);
  let editing = $state<string | null>(null);
  let formError = $state("");

  let form = $state<CreateProviderRequest>({
    providerName: "",
    providerType: "openai",
    baseUrl: "",
    keyringService: "llm-bridge",
    keyringAccount: "",
  });

  async function load() {
    loading = true;
    error = "";
    try {
      providers = await api.listProviders();
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  $effect(() => { load(); });

  function resetForm() {
    form = { providerName: "", providerType: "openai", baseUrl: "", keyringService: "llm-bridge", keyringAccount: "" };
    dialogOpen = false;
    editing = null;
    formError = "";
  }

  function startEdit(p: Provider) {
    editing = p.providerName;
    form = {
      providerName: p.providerName,
      providerType: p.providerType,
      baseUrl: p.baseUrl ?? "",
      keyringService: p.keyringService,
      keyringAccount: p.keyringAccount,
    };
    dialogOpen = true;
    formError = "";
  }

  async function handleSubmit() {
    formError = "";
    try {
      if (editing) {
        await api.updateProvider(editing, {
          providerType: form.providerType,
          baseUrl: form.baseUrl || undefined,
          keyringService: form.keyringService,
          keyringAccount: form.keyringAccount,
        });
      } else {
        await api.createProvider({
          ...form,
          baseUrl: form.baseUrl || undefined,
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
      await api.deleteProvider(name);
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

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h2 class="text-xl font-semibold">提供者管理</h2>
    <Button onclick={() => { resetForm(); dialogOpen = true; }}>
      + 新建提供者
    </Button>
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
          <div class="space-y-2">
            <Label for="keyring-service">Keyring Service</Label>
            <Input id="keyring-service" bind:value={form.keyringService} required />
          </div>
          <div class="space-y-2">
            <Label for="keyring-account">Keyring Account</Label>
            <Input id="keyring-account" bind:value={form.keyringAccount} required />
          </div>
        </div>
        <Dialog.Footer>
          <Button type="button" variant="outline" onclick={resetForm}>取消</Button>
          <Button type="submit">{editing ? "保存修改" : "创建"}</Button>
        </Dialog.Footer>
      </form>
    </Dialog.Content>
  </Dialog.Root>

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
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
      {#each providers as p}
        <Card.Root>
          <Card.Header>
            <div class="flex items-start justify-between">
              <div>
                <Card.Title class="text-base">{p.providerName}</Card.Title>
                <Badge class="mt-1 {typeBadgeVariant[p.providerType] ?? ''}">
                  {typeLabels[p.providerType] ?? p.providerType}
                </Badge>
              </div>
              <div class="flex gap-1">
                <Button variant="ghost" size="sm" onclick={() => startEdit(p)}>✏️</Button>
                <Button variant="ghost" size="sm" onclick={() => handleDelete(p.providerName)}>🗑️</Button>
              </div>
            </div>
          </Card.Header>
          <Card.Content>
            <dl class="space-y-1.5 text-xs text-muted-foreground">
              {#if p.baseUrl}
                <div class="flex gap-2">
                  <dt class="shrink-0 font-medium">URL</dt>
                  <dd class="truncate font-mono">{p.baseUrl}</dd>
                </div>
              {/if}
              <div class="flex gap-2">
                <dt class="shrink-0 font-medium">Keyring</dt>
                <dd class="font-mono">{p.keyringService}/{p.keyringAccount}</dd>
              </div>
            </dl>
          </Card.Content>
        </Card.Root>
      {/each}
    </div>
  {/if}
</div>
