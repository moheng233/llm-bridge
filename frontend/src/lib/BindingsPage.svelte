<script lang="ts">
  import { api, type Provider, type ProviderModel, type CatalogModel, type CreateBindingRequest } from "./api";
  import * as Table from "$lib/components/ui/table/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";

  let providers = $state<Provider[]>([]);
  let catalogModels = $state<CatalogModel[]>([]);
  let selectedProvider = $state("");
  let bindings = $state<ProviderModel[]>([]);
  let loading = $state(true);
  let bindingsLoading = $state(false);
  let error = $state("");
  let dialogOpen = $state(false);
  let formError = $state("");

  let form = $state<CreateBindingRequest>({
    modelName: "",
    providerModelName: "",
    priority: 0,
  });

  async function load() {
    loading = true;
    error = "";
    try {
      [providers, catalogModels] = await Promise.all([api.listProviders(), api.listModels()]);
      if (providers.length > 0 && !selectedProvider) {
        selectedProvider = providers[0].providerName;
      }
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function loadBindings() {
    if (!selectedProvider) { bindings = []; return; }
    bindingsLoading = true;
    try {
      bindings = await api.listBindings(selectedProvider);
    } catch (e: any) {
      error = e.message;
    } finally {
      bindingsLoading = false;
    }
  }

  $effect(() => { load(); });
  $effect(() => { selectedProvider; loadBindings(); });

  function resetForm() {
    form = { modelName: "", providerModelName: "", priority: 0 };
    dialogOpen = false;
    formError = "";
  }

  async function handleSubmit() {
    formError = "";
    try {
      await api.createBinding(selectedProvider, form);
      resetForm();
      await loadBindings();
    } catch (e: any) {
      formError = e.message;
    }
  }

  async function handleDelete(modelName: string) {
    try {
      await api.deleteBinding(selectedProvider, modelName);
      await loadBindings();
    } catch (e: any) {
      error = e.message;
    }
  }

  function onModelSelect(value: string) {
    form.modelName = value;
    if (value && !form.providerModelName) {
      form.providerModelName = value.split("/").pop() ?? value;
    }
  }

  let availableModels = $derived(
    catalogModels.filter((m) => !bindings.some((b) => b.modelName === m.modelName)),
  );
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h2 class="text-xl font-semibold">模型绑定</h2>
    {#if selectedProvider}
      <Button onclick={() => { resetForm(); dialogOpen = true; }}>
        + 新建绑定
      </Button>
    {/if}
  </div>

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
      请先在「提供者管理」中创建至少一个提供者
    </div>
  {:else}
    <div class="flex items-center gap-3">
      <Label class="text-sm font-medium">选择提供者：</Label>
      <Select.Root type="single" bind:value={selectedProvider}>
        <Select.Trigger class="w-[280px]">
          {providers.find((p) => p.providerName === selectedProvider)?.providerName ?? "选择提供者"}
        </Select.Trigger>
        <Select.Content>
          {#each providers as p}
            <Select.Item value={p.providerName}>{p.providerName} ({p.providerType})</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </div>

    <Dialog.Root bind:open={dialogOpen}>
      <Dialog.Content class="sm:max-w-lg">
        <Dialog.Header>
          <Dialog.Title>新建模型绑定</Dialog.Title>
          <Dialog.Description>将目录模型绑定到当前提供者</Dialog.Description>
        </Dialog.Header>
        {#if formError}
          <Alert variant="destructive">
            <AlertDescription>{formError}</AlertDescription>
          </Alert>
        {/if}
        {#if availableModels.length === 0}
          <p class="py-4 text-sm text-muted-foreground">所有目录模型都已绑定到此提供者</p>
        {:else}
          <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }} class="space-y-4">
            <div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
              <div class="space-y-2">
                <Label>目录模型</Label>
                <Select.Root type="single" value={form.modelName} onValueChange={onModelSelect}>
                  <Select.Trigger class="w-full">
                    {form.modelName || "选择模型"}
                  </Select.Trigger>
                  <Select.Content>
                    {#each availableModels as m}
                      <Select.Item value={m.modelName}>{m.modelName}</Select.Item>
                    {/each}
                  </Select.Content>
                </Select.Root>
              </div>
              <div class="space-y-2">
                <Label for="provider-model-name">提供者侧模型名</Label>
                <Input id="provider-model-name" bind:value={form.providerModelName} required placeholder="gpt-4o" />
              </div>
              <div class="space-y-2">
                <Label for="priority">优先级（越小越优先）</Label>
                <Input id="priority" type="number" bind:value={form.priority} min="0" />
              </div>
            </div>
            <Dialog.Footer>
              <Button type="button" variant="outline" onclick={resetForm}>取消</Button>
              <Button type="submit">创建绑定</Button>
            </Dialog.Footer>
          </form>
        {/if}
      </Dialog.Content>
    </Dialog.Root>

    {#if bindingsLoading}
      <div class="flex items-center justify-center py-12">
        <Spinner class="size-6" />
      </div>
    {:else if bindings.length === 0}
      <div class="rounded-lg border border-dashed py-12 text-center text-sm text-muted-foreground">
        该提供者暂无模型绑定
      </div>
    {:else}
      <Table.Root>
        <Table.Header>
          <Table.Row>
            <Table.Head>目录模型名</Table.Head>
            <Table.Head>提供者模型名</Table.Head>
            <Table.Head class="text-right">优先级</Table.Head>
            <Table.Head class="text-center">操作</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each bindings as b}
            <Table.Row>
              <Table.Cell class="font-mono text-xs">{b.modelName}</Table.Cell>
              <Table.Cell class="font-mono text-xs">{b.providerModelName}</Table.Cell>
              <Table.Cell class="text-right">
                <Badge variant="secondary">{b.priority}</Badge>
              </Table.Cell>
              <Table.Cell class="text-center">
                <Button variant="ghost" size="sm" onclick={() => handleDelete(b.modelName)}>🗑️</Button>
              </Table.Cell>
            </Table.Row>
          {/each}
        </Table.Body>
      </Table.Root>
    {/if}
  {/if}
</div>
