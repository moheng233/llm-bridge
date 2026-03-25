<script lang="ts">
  import { createApiClient } from "$bindings/client";
  import type { CatalogModelResponse } from "$bindings";
  import {
    stockFeatures,
    createSortedRowModel,
    createFilteredRowModel,
  } from "@tanstack/table-core";
  import { createTable, FlexRender } from "$lib/components/table";
  import * as Table from "$lib/components/ui/table/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Label } from "$lib/components/ui/label/index.js";

  const api = createApiClient({ baseUrl: "", credentials: "include" });

  let models = $state<CatalogModelResponse[]>([]);
  let loading = $state(true);
  let error = $state("");
  let search = $state("");
  let onlyAvailable = $state(false);
  let renderKey = $state(0);
  const loadingRows = Array.from({ length: 8 }, (_, index) => index);

  async function load() {
    loading = true;
    error = "";
    try {
      models = onlyAvailable
        ? await api.models.listAvailableModels()
        : await api.models.listCatalogModels();
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
      renderKey++;
    }
  }

  $effect(() => {
    onlyAvailable;
    load();
  });

  function formatTokens(n: number): string {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`;
    return n.toString();
  }

  const columns = [
    {
      accessorFn: (row: CatalogModelResponse) => row.capabilities.name,
      id: "name",
      header: "模型名称",
      cell: (info: any) => info.getValue(),
      enableSorting: true,
      enableColumnFilter: true,
      filterFn: "includesString" as const,
    },
    {
      accessorFn: (row: CatalogModelResponse) =>
        row.capabilities.maxInputTokens,
      id: "inputTokens",
      header: () => "输入 / 输出",
      cell: (info: any) => {
        const output = info.row.original.capabilities.maxOutputTokens;
        return `${formatTokens(info.getValue())} / ${formatTokens(output)}`;
      },
      enableSorting: true,
      sortDescFirst: true,
    },
    {
      accessorFn: (row: CatalogModelResponse) => row.capabilities.toolCalling,
      id: "toolCalling",
      header: "工具",
      cell: (info: any) => info.getValue(),
      enableSorting: true,
    },
    {
      accessorFn: (row: CatalogModelResponse) => row.capabilities.vision,
      id: "vision",
      header: "视觉",
      cell: (info: any) => info.getValue(),
      enableSorting: true,
    },
    {
      accessorFn: (row: CatalogModelResponse) => row.capabilities.thinking,
      id: "thinking",
      header: "推理",
      cell: (info: any) => info.getValue(),
      enableSorting: true,
    },
  ];

  let sorting = $state<Array<{ id: string; desc: boolean }>>([]);
  let columnFilters = $state<Array<{ id: string; value: unknown }>>([]);

  let table = $derived(
    createTable({
      _features: { ...stockFeatures },
      _rowModels: {
        sortedRowModel: createSortedRowModel({}),
        filteredRowModel: createFilteredRowModel({}),
      },
      columns,
      data: models,
      state: {
        get sorting() {
          return sorting;
        },
        get columnFilters() {
          return columnFilters;
        },
      },
      onSortingChange: (updater: any) => {
        sorting = updater instanceof Function ? updater(sorting) : updater;
      },
      onColumnFiltersChange: (updater: any) => {
        columnFilters =
          updater instanceof Function ? updater(columnFilters) : updater;
      },
    }),
  );

  $effect(() => {
    if (search) {
      columnFilters = [{ id: "name", value: search }];
    } else {
      columnFilters = [];
    }
  });
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
  <section
    class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border bg-background shadow-sm"
  >
    <div
      class="flex shrink-0 flex-col gap-3 border-b px-4 py-3 lg:flex-row lg:items-center lg:justify-between"
    >
      <div class="flex items-center gap-3">
        <h2 class="text-xl font-semibold">模型目录</h2>
        <Badge variant="secondary">{models.length} 个模型</Badge>
        <span
          class="inline-flex items-center gap-2 text-sm text-muted-foreground transition-opacity duration-200 {loading
            ? 'opacity-100'
            : 'opacity-0 pointer-events-none'}"
        >
          <span
            class="size-3 rounded-full border-2 border-muted-foreground/25 border-t-muted-foreground animate-spin"
          ></span>
          正在同步
        </span>
      </div>

      <div
        class="flex w-full flex-col gap-3 lg:w-auto lg:flex-row lg:items-center"
      >
        <!-- 固定最小宽度，防止 checkbox 切换时整行收缩跳动 -->
        <Label
          class="flex min-w-26 shrink-0 cursor-pointer items-center gap-2 text-sm font-normal text-muted-foreground"
        >
          <Checkbox
            bind:checked={onlyAvailable}
            class="data-checked:bg-primary data-checked:border-primary"
          />
          只显示可用
        </Label>
        <Input
          type="text"
          placeholder="搜索模型名称..."
          bind:value={search}
          class="lg:min-w-[24rem]"
        />
        <!-- tabular-nums + min-w 保证数字宽度稳定，不因位数变化而闪烁 -->
        <Badge
          variant="outline"
          class="w-30 shrink-0 justify-center tabular-nums"
          >显示 {loading ? "--" : table.getRowModel().rows.length} 条</Badge
        >
      </div>
    </div>

    <div class="min-h-0 flex-1 overflow-auto [scrollbar-gutter:stable]">
      <table
        class="w-full table-fixed border-separate border-spacing-0 text-sm"
      >
        <colgroup>
          <!-- name: 占满剩余宽度 -->
          <col />
          <!-- 输入/输出 -->
          <col style="width: 9rem" />
          <!-- 工具 -->
          <col style="width: 5rem" />
          <!-- 视觉 -->
          <col style="width: 5rem" />
          <!-- 推理 -->
          <col style="width: 5rem" />
        </colgroup>
        <Table.Header>
          {#each table.getHeaderGroups() as headerGroup}
            <Table.Row class="hover:bg-transparent">
              {#each headerGroup.headers as header}
                <Table.Head
                  class="sticky top-0 z-20 border-b bg-background/95 backdrop-blur supports-backdrop-filter:bg-background/80 {header.column.getCanSort()
                    ? 'cursor-pointer select-none'
                    : ''} {header.column.id === 'inputTokens' ||
                  header.column.id === 'name'
                    ? ''
                    : 'text-center'} {header.column.id === 'inputTokens'
                    ? 'text-right'
                    : ''}"
                  onclick={header.column.getToggleSortingHandler()}
                >
                  <div
                    class="flex items-center gap-1 {header.column.id ===
                    'inputTokens'
                      ? 'justify-end'
                      : header.column.id === 'name'
                        ? ''
                        : 'justify-center'}"
                  >
                    <FlexRender
                      content={header.column.columnDef.header}
                      context={header.getContext()}
                    />
                    {#if header.column.getIsSorted() === "asc"}
                      <span>↑</span>
                    {:else if header.column.getIsSorted() === "desc"}
                      <span>↓</span>
                    {/if}
                  </div>
                </Table.Head>
              {/each}
            </Table.Row>
          {/each}
        </Table.Header>

        <Table.Body>
          {#if loading}
            {#each loadingRows as skeletonRow}
              <Table.Row class="animate-pulse">
                <Table.Cell class="font-mono text-xs">
                  <div
                    class="h-4 w-40 rounded bg-muted/80 transition-opacity duration-300"
                    style={`animation-delay: ${skeletonRow * 60}ms;`}
                  ></div>
                </Table.Cell>
                <Table.Cell class="text-right font-mono text-xs">
                  <div
                    class="ml-auto h-4 w-24 rounded bg-muted/80 transition-opacity duration-300"
                    style={`animation-delay: ${skeletonRow * 60 + 40}ms;`}
                  ></div>
                </Table.Cell>
                <Table.Cell class="text-center">
                  <div
                    class="mx-auto h-5 w-8 rounded-full bg-muted/70 transition-opacity duration-300"
                    style={`animation-delay: ${skeletonRow * 60 + 80}ms;`}
                  ></div>
                </Table.Cell>
                <Table.Cell class="text-center">
                  <div
                    class="mx-auto h-5 w-8 rounded-full bg-muted/70 transition-opacity duration-300"
                    style={`animation-delay: ${skeletonRow * 60 + 120}ms;`}
                  ></div>
                </Table.Cell>
                <Table.Cell class="text-center">
                  <div
                    class="mx-auto h-5 w-8 rounded-full bg-muted/70 transition-opacity duration-300"
                    style={`animation-delay: ${skeletonRow * 60 + 160}ms;`}
                  ></div>
                </Table.Cell>
              </Table.Row>
            {/each}
          {:else if error}
            <Table.Row class="hover:bg-transparent">
              <Table.Cell colspan={columns.length} class="p-4">
                <Alert variant="destructive">
                  <AlertDescription>{error}</AlertDescription>
                </Alert>
              </Table.Cell>
            </Table.Row>
          {:else if table.getRowModel().rows.length === 0}
            <Table.Row class="hover:bg-transparent">
              <Table.Cell
                colspan={columns.length}
                class="py-12 text-center text-sm text-muted-foreground"
              >
                {search
                  ? "没有匹配的模型"
                  : onlyAvailable
                    ? "暂无可用模型"
                    : "暂无模型数据"}
              </Table.Cell>
            </Table.Row>
          {:else}
            {#key renderKey}
              {#each table.getRowModel().rows as row, rowIndex}
                <Table.Row
                  style="animation: row-enter 220ms ease both; animation-delay: {Math.min(
                    rowIndex * 20,
                    480,
                  )}ms;"
                >
                  {#each row.getVisibleCells() as cell}
                    <Table.Cell
                      class="{cell.column.id === 'name'
                        ? 'font-mono text-xs'
                        : ''} {cell.column.id === 'inputTokens'
                        ? 'text-right font-mono text-xs'
                        : ''} {!['name', 'inputTokens'].includes(cell.column.id)
                        ? 'text-center'
                        : ''}"
                    >
                      {#if cell.column.id === "toolCalling"}
                        {#if cell.getValue()}
                          <Badge
                            variant="default"
                            class="bg-emerald-100 text-emerald-700 hover:bg-emerald-100"
                            >✓</Badge
                          >
                        {:else}
                          <span class="text-muted-foreground">—</span>
                        {/if}
                      {:else if cell.column.id === "vision"}
                        {#if cell.getValue()}
                          <Badge
                            variant="default"
                            class="bg-blue-100 text-blue-700 hover:bg-blue-100"
                            >✓</Badge
                          >
                        {:else}
                          <span class="text-muted-foreground">—</span>
                        {/if}
                      {:else if cell.column.id === "thinking"}
                        {#if cell.getValue()}
                          <Badge
                            variant="default"
                            class="bg-purple-100 text-purple-700 hover:bg-purple-100"
                            >✓</Badge
                          >
                        {:else}
                          <span class="text-muted-foreground">—</span>
                        {/if}
                      {:else}
                        <FlexRender
                          content={cell.column.columnDef.cell}
                          context={cell.getContext()}
                        />
                      {/if}
                    </Table.Cell>
                  {/each}
                </Table.Row>
              {/each}
            {/key}
          {/if}
        </Table.Body>
      </table>
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
