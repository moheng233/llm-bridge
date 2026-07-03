<script lang="ts">
  import { getApi, formatTokens, formatPrice } from "$lib/api";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Alert, AlertDescription } from "$lib/components/ui/alert/index.js";
  import { Checkbox } from "$lib/components/ui/checkbox/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { Skeleton } from "$lib/components/ui/skeleton/index.js";
  import { Search, Cpu, Eye, Wrench, Brain } from "@lucide/svelte";
  import type { ModelResponse } from "$bindings/ModelResponse";

  const api = getApi();

  let models = $state<ModelResponse[]>([]);
  let loading = $state(true);
  let error = $state("");
  let search = $state("");
  let onlyAvailable = $state(false);
  let sortField = $state<"name" | "maxInputTokens" | "maxOutputTokens">("name");
  let sortDir = $state<"asc" | "desc">("asc");

  async function load() {
    loading = true;
    error = "";
    try {
      models = onlyAvailable
        ? await api.models.listAvailableModels()
        : await api.models.listAllModels();
    } catch (e: any) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    onlyAvailable;
    load();
  });

  let filteredModels = $derived.by(() => {
    let result = models;
    if (search.trim()) {
      const q = search.toLowerCase();
      result = result.filter(m =>
        m.modelName.toLowerCase().includes(q) ||
        m.description?.toLowerCase().includes(q)
      );
    }
    result = [...result].sort((a, b) => {
      const dir = sortDir === "asc" ? 1 : -1;
      if (sortField === "name") return dir * a.modelName.localeCompare(b.modelName);
      if (sortField === "maxInputTokens") return dir * (a.maxInputTokens - b.maxInputTokens);
      if (sortField === "maxOutputTokens") return dir * (a.maxOutputTokens - b.maxOutputTokens);
      return 0;
    });
    return result;
  });

  function toggleSort(field: typeof sortField) {
    if (sortField === field) {
      sortDir = sortDir === "asc" ? "desc" : "asc";
    } else {
      sortField = field;
      sortDir = "desc";
    }
  }

  function sortIndicator(field: typeof sortField): string {
    if (sortField !== field) return "";
    return sortDir === "asc" ? " ↑" : " ↓";
  }
</script>

<div class="flex h-full min-h-0 flex-col gap-4">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-xl font-bold font-mono text-foreground">模型目录</h2>
      <p class="text-sm text-muted-foreground mt-1">浏览可用模型的能力与定价</p>
    </div>
    <Badge variant="secondary" class="font-mono">{models.length} 个模型</Badge>
  </div>

  <!-- Error -->
  {#if error}
    <Alert class="border-destructive/30 bg-destructive/10">
      <AlertDescription class="text-destructive text-sm">{error}</AlertDescription>
    </Alert>
  {/if}

  <!-- Filters -->
  <div class="flex items-center gap-3">
    <div class="relative flex-1 max-w-md">
      <Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
      <Input class="pl-9" placeholder="搜索模型名称或描述..." bind:value={search} />
    </div>
    <Label class="flex items-center gap-2 text-sm text-muted-foreground cursor-pointer whitespace-nowrap">
      <Checkbox bind:checked={onlyAvailable} class="cursor-pointer" />
      仅可用
    </Label>
  </div>

  <!-- Table -->
  {#if loading}
    <div class="flex flex-col gap-2">
      {#each Array(6) as _}
        <Skeleton class="h-12 w-full rounded-lg" />
      {/each}
    </div>
  {:else}
    <div class="flex-1 min-h-0 overflow-auto rounded-lg border border-border">
      <table class="w-full text-sm">
        <thead class="sticky top-0 bg-card border-b border-border">
          <tr>
            <th class="px-4 py-3 text-left font-mono text-xs text-muted-foreground cursor-pointer hover:text-foreground transition-colors" onclick={() => toggleSort("name")}>
              模型名称{sortIndicator("name")}
            </th>
            <th class="px-4 py-3 text-right font-mono text-xs text-muted-foreground cursor-pointer hover:text-foreground transition-colors" onclick={() => toggleSort("maxInputTokens")}>
              输入{sortIndicator("maxInputTokens")}
            </th>
            <th class="px-4 py-3 text-right font-mono text-xs text-muted-foreground cursor-pointer hover:text-foreground transition-colors" onclick={() => toggleSort("maxOutputTokens")}>
              输出{sortIndicator("maxOutputTokens")}
            </th>
            <th class="px-4 py-3 text-center font-mono text-xs text-muted-foreground">能力</th>
            <th class="px-4 py-3 text-right font-mono text-xs text-muted-foreground">输入价格</th>
            <th class="px-4 py-3 text-right font-mono text-xs text-muted-foreground">输出价格</th>
          </tr>
        </thead>
        <tbody>
          {#each filteredModels as model}
            <tr class="border-b border-border/50 hover:bg-accent/50 transition-colors cursor-pointer">
              <td class="px-4 py-3">
                <div class="flex flex-col gap-0.5">
                  <span class="font-mono font-medium text-foreground">{model.modelName}</span>
                  {#if model.description}
                    <span class="text-xs text-muted-foreground line-clamp-1">{model.description}</span>
                  {/if}
                  {#if model.providerIds.length > 0}
                    <div class="flex gap-1 mt-1">
                      {#each model.providerIds as pid}
                        <Badge variant="outline" class="text-xs font-mono px-1.5 py-0">{pid}</Badge>
                      {/each}
                    </div>
                  {/if}
                </div>
              </td>
              <td class="px-4 py-3 text-right font-mono tabular-nums text-foreground">{formatTokens(model.maxInputTokens)}</td>
              <td class="px-4 py-3 text-right font-mono tabular-nums text-foreground">{formatTokens(model.maxOutputTokens)}</td>
              <td class="px-4 py-3">
                <div class="flex items-center justify-center gap-1.5">
                  {#if model.toolCalling}
                    <Wrench class="h-3.5 w-3.5 text-[#22C55E]" title="工具调用" />
                  {/if}
                  {#if model.vision}
                    <Eye class="h-3.5 w-3.5 text-[#22C55E]" title="视觉" />
                  {/if}
                  {#if model.thinking}
                    <Brain class="h-3.5 w-3.5 text-[#22C55E]" title="推理" />
                  {/if}
                </div>
              </td>
              <td class="px-4 py-3 text-right font-mono tabular-nums text-foreground">{formatPrice(model.inputPricePer1m)}</td>
              <td class="px-4 py-3 text-right font-mono tabular-nums text-foreground">{formatPrice(model.outputPricePer1m)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if filteredModels.length === 0}
        <div class="flex items-center justify-center py-12 text-muted-foreground">
          <div class="flex flex-col items-center gap-2">
            <Cpu class="h-8 w-8 opacity-30" />
            <p class="text-sm">未找到匹配的模型</p>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

