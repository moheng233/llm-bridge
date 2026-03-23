<script lang="ts">
  import { api, type CatalogModel } from "./api";
  import * as Table from "$lib/components/ui/table/index.js";
  import * as Tabs from "$lib/components/ui/tabs/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Spinner } from "$lib/components/ui/spinner/index.js";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";

  let models = $state<CatalogModel[]>([]);
  let loading = $state(true);
  let error = $state("");
  let search = $state("");
  let tab = $state<"catalog" | "available">("catalog");

  async function load() {
    loading = true;
    error = "";
    try {
      models = tab === "catalog" ? await api.listModels() : await api.listAvailableModels();
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    tab;
    load();
  });

let filtered = $derived(
  search
    ? models.filter((m) => m.capabilities.name.toLowerCase().includes(search.toLowerCase()))
    : models,
);

  function formatTokens(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`;
    return n.toString();
  }
</script>

<div class="flex flex-col h-full gap-4">
  <div class="shrink-0 space-y-4">
    <div class="flex items-center justify-between">
      <h2 class="text-xl font-semibold">模型目录</h2>
      <Badge variant="secondary">{models.length} 个模型</Badge>
    </div>

    <Tabs.Root value={tab} onValueChange={(v) => (tab = v as "catalog" | "available")}>
      <Tabs.List>
        <Tabs.Trigger value="catalog">全部模型</Tabs.Trigger>
        <Tabs.Trigger value="available">已绑定可用</Tabs.Trigger>
      </Tabs.List>
    </Tabs.Root>

    <Input type="text" placeholder="搜索模型名称..." bind:value={search} />
  </div>

  {#if loading}
    <div class="flex items-center justify-center py-16">
      <Spinner class="size-8" />
    </div>
  {:else if error}
    <Alert variant="destructive">
      <AlertDescription>{error}</AlertDescription>
    </Alert>
  {:else if filtered.length === 0}
    <div class="rounded-lg border border-dashed py-12 text-center text-sm text-muted-foreground">
      {search ? "没有匹配的模型" : "暂无模型数据"}
    </div>
  {:else}
    <div class="flex-1 min-h-0 overflow-auto rounded-md border">
    <Table.Root>
      <Table.Header>
        <Table.Row>
          <Table.Head class="sticky top-0 bg-background z-10">模型名称</Table.Head>
          <Table.Head class="text-right sticky top-0 bg-background z-10">输入 / 输出</Table.Head>
          <Table.Head class="text-center sticky top-0 bg-background z-10">工具</Table.Head>
          <Table.Head class="text-center sticky top-0 bg-background z-10">视觉</Table.Head>
          <Table.Head class="text-center sticky top-0 bg-background z-10">推理</Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each filtered as m}
          <Table.Row>
            <Table.Cell class="font-mono text-xs">{m.capabilities.name}</Table.Cell>
            <Table.Cell class="text-right font-mono text-xs">{formatTokens(m.capabilities.maxInputTokens)} / {formatTokens(m.capabilities.maxOutputTokens)}</Table.Cell>
            <Table.Cell class="text-center">
              {#if m.capabilities.toolCalling}
                <Badge variant="default" class="bg-emerald-100 text-emerald-700 hover:bg-emerald-100">✓</Badge>
              {:else}
                <span class="text-muted-foreground">—</span>
              {/if}
            </Table.Cell>
            <Table.Cell class="text-center">
              {#if m.capabilities.vision}
                <Badge variant="default" class="bg-blue-100 text-blue-700 hover:bg-blue-100">✓</Badge>
              {:else}
                <span class="text-muted-foreground">—</span>
              {/if}
            </Table.Cell>
            <Table.Cell class="text-center">
              {#if m.capabilities.thinking}
                <Badge variant="default" class="bg-purple-100 text-purple-700 hover:bg-purple-100">✓</Badge>
              {:else}
                <span class="text-muted-foreground">—</span>
              {/if}
            </Table.Cell>
          </Table.Row>
        {/each}
      </Table.Body>
    </Table.Root>
    </div>
  {/if}
</div>
