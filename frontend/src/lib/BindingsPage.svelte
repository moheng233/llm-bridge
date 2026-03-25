<script lang="ts">
  import { createApiClient } from "$bindings/client";
  import type { ProviderResponse, ProviderModelResponse, CatalogModelResponse, CreateProviderModelRequest } from "$bindings";
  import * as Table from "$lib/components/ui/table/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";

  const api = createApiClient({ baseUrl: "", credentials: "include" });

  let providers = $state<ProviderResponse[]>([]);
  let catalogModels = $state<CatalogModelResponse[]>([]);
  let selectedProvider = $state("");
  let bindings = $state<ProviderModelResponse[]>([]);
  let loading = $state(true);
  let bindingsLoading = $state(false);
  let error = $state("");
  let dialogOpen = $state(false);
  let formError = $state("");

  let form = $state<CreateProviderModelRequest>({
    modelName: "",
    providerModelName: "",
    priority: 0,
  });

  async function load() {
    loading = true;
    error = "";
    try {
      [providers, catalogModels] = await Promise.all([api.providers.listProviders(), api.models.listCatalogModels()]);
      if (providers.length > 0 && !selectedProvider) {
        selectedProvider = providers[0].provider_name;
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
      bindings = await api.providers.listProviderModels(selectedProvider);
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
      await api.providers.createProviderModel(selectedProvider, form);
      resetForm();
      await loadBindings();
    } catch (e: any) {
      formError = e.message;
    }
  }

  async function handleDelete(modelName: string) {
    try {
      await api.providers.deleteProviderModelBinding(selectedProvider, modelName);
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
    catalogModels.filter((m) => !bindings.some((b) => b.model_name === m.model_name)),
  );
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
  <section
    class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border bg-background shadow-sm"
  >
    <div
      class="flex shrink-0 flex-col gap-3 border-b px-4 py-3 lg:flex-row lg:items-center lg:justify-between"
    >
      <div class="flex items-center gap-3">
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
            <Select.Trigger class="w-70">
              {providers.find((p) => p.provider_name === selectedProvider)?.provider_name ?? "选择提供者"}
            </Select.Trigger>
            <Select.Content>
              {#each providers as p}
                <Select.Item value={p.provider_name}>{p.provider_name} ({p.provider_type})</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
      {/if}
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
                      <Select.Item value={m.model_name}>{m.model_name}</Select.Item>
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

    <div class="min-h-0 flex-1 overflow-auto [scrollbar-gutter:stable]">
      {#if bindingsLoading}
        <div class="flex items-center justify-center py-12">
          <Spinner class="size-6" />
        </div>
      {:else if bindings.length === 0}
        <div class="rounded-lg border border-dashed py-12 text-center text-sm text-muted-foreground">
          该提供者暂无模型绑定
        </div>
      {:else}
        <table class="w-full table-fixed border-separate border-spacing-0 text-sm">
          <colgroup>
            <col />
            <col />
            <col style="width: 6rem" />
            <col style="width: 5rem" />
          </colgroup>
          <Table.Header>
            <Table.Row class="hover:bg-transparent">
              <Table.Head class="sticky top-0 z-20 border-b bg-background/95 backdrop-blur supports-backdrop-filter:bg-background/80">
                目录模型名
              </Table.Head>
              <Table.Head class="sticky top-0 z-20 border-b bg-background/95 backdrop-blur supports-backdrop-filter:bg-background/80">
                提供者模型名
              </Table.Head>
              <Table.Head class="sticky top-0 z-20 border-b bg-background/95 backdrop-blur supports-backdrop-filter:bg-background/80 text-right">
                优先级
              </Table.Head>
              <Table.Head class="sticky top-0 z-20 border-b bg-background/95 backdrop-blur supports-backdrop-filter:bg-background/80 text-center">
                操作
              </Table.Head>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each bindings as b, rowIndex}
              <Table.Row style="animation: row-enter 220ms ease both; animation-delay: {Math.min(rowIndex * 20, 480)}ms;">
                <Table.Cell class="font-mono text-xs">{b.model_name}</Table.Cell>
                <Table.Cell class="font-mono text-xs">{b.provider_model_name}</Table.Cell>
                <Table.Cell class="text-right">
                  <Badge variant="secondary">{b.priority}</Badge>
                </Table.Cell>
                <Table.Cell class="text-center">
                  <Button variant="ghost" size="sm" onclick={() => handleDelete(b.model_name)}>🗑️</Button>
                </Table.Cell>
              </Table.Row>
            {/each}
          </Table.Body>
        </table>
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
