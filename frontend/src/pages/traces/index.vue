<script setup lang="ts">
import { ChevronRight, ListFilter, ScrollText, Search } from "@lucide/vue";

import { formatTokens } from "~/lib/api";
import { MOCK_MODELS, MOCK_TOKENS, MOCK_TRACES, type RequestTrace } from "~/lib/observability-mock";

// ── 筛选状态（示例数据，O4 API 就绪后替换为服务端筛选）──

const search = ref("");
const statusFilter = ref("all");
const modelFilter = ref("all");
const tokenFilter = ref("all");

const STATUS_OPTIONS = [
  { value: "all", label: "全部状态" },
  { value: "Success", label: "成功" },
  { value: "Error", label: "失败" },
  { value: "Cancelled", label: "已取消" },
  { value: "Streaming", label: "进行中" },
  { value: "Pending", label: "等待中" },
];

const filtered = computed(() => {
  return MOCK_TRACES.filter((t) => {
    if (statusFilter.value !== "all" && t.status !== statusFilter.value) return false;
    if (modelFilter.value !== "all" && t.model !== modelFilter.value) return false;
    if (tokenFilter.value !== "all" && t.tokenId !== Number(tokenFilter.value)) return false;
    if (search.value.trim()) {
      const q = search.value.trim().toLowerCase();
      return (
        t.requestId.toLowerCase().includes(q) ||
        t.model.toLowerCase().includes(q) ||
        (t.errorMessage ?? "").toLowerCase().includes(q)
      );
    }
    return true;
  });
});

function statusBadge(t: RequestTrace): { label: string; cls: string } {
  switch (t.status) {
    case "Success":
      return { label: "成功", cls: "text-cta border-cta/30 bg-cta/10" };
    case "Error":
      return { label: "失败", cls: "text-destructive border-destructive/30 bg-destructive/10" };
    case "Cancelled":
      return { label: "已取消", cls: "text-muted-foreground border-border bg-muted" };
    case "Streaming":
      return { label: "进行中", cls: "text-chart-2 border-chart-2/30 bg-chart-2/10" };
    default:
      return { label: "等待中", cls: "text-chart-4 border-chart-4/30 bg-chart-4/10" };
  }
}

function formatDateTime(ts: number): string {
  return new Date(ts * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function formatCost(usd: number | null): string {
  if (usd == null) return "—";
  return usd >= 0.01 ? `$${usd.toFixed(3)}` : `$${usd.toFixed(4)}`;
}

function hasSnapshot(t: RequestTrace): boolean {
  return t.requestMessages != null || t.responseParts != null;
}
</script>

<template>
  <PageShell>
    <SectionHeader
      title="请求追踪"
      description="每次请求的完整生命周期记录（示例数据）"
      :icon="ScrollText"
      :count="filtered.length"
      count-label="条"
    />

    <!-- 筛选栏 -->
    <div class="flex flex-wrap items-center gap-2">
      <div class="relative min-w-52 flex-1">
        <Search class="absolute top-1/2 left-2.5 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <Input v-model="search" placeholder="搜索 request ID / 模型 / 错误信息…" class="pl-8" />
      </div>
      <Select v-model="statusFilter">
        <SelectTrigger class="w-28">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="opt in STATUS_OPTIONS" :key="opt.value" :value="opt.value">
            {{ opt.label }}
          </SelectItem>
        </SelectContent>
      </Select>
      <Select v-model="modelFilter">
        <SelectTrigger class="w-52">
          <SelectValue placeholder="全部模型" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="all">全部模型</SelectItem>
          <SelectItem v-for="m in MOCK_MODELS" :key="m" :value="m">{{ m }}</SelectItem>
        </SelectContent>
      </Select>
      <Select v-model="tokenFilter">
        <SelectTrigger class="w-40">
          <SelectValue placeholder="全部 Token" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="all">全部 Token</SelectItem>
          <SelectItem v-for="t in MOCK_TOKENS" :key="t.id" :value="String(t.id)">
            {{ t.name }}
          </SelectItem>
        </SelectContent>
      </Select>
      <Button variant="outline" size="icon" class="cursor-pointer" title="更多筛选（待后端支持）">
        <ListFilter class="h-4 w-4" />
      </Button>
    </div>

    <!-- 空态 -->
    <EmptyState v-if="filtered.length === 0" :icon="ScrollText" title="没有匹配的请求记录" />

    <!-- 追踪表格：撑满剩余高度，表格内部滚动，表头吸顶 -->
    <Card v-else class="min-h-0 flex-1 gap-0 overflow-hidden py-0">
      <Table>
        <TableHeader>
          <TableRow class="hover:bg-transparent">
            <TableHead class="sticky top-0 z-10 w-20 bg-card">状态</TableHead>
            <TableHead class="sticky top-0 z-10 w-40 bg-card">时间</TableHead>
            <TableHead class="sticky top-0 z-10 bg-card">模型</TableHead>
            <TableHead class="sticky top-0 z-10 w-28 bg-card">Token</TableHead>
            <TableHead class="sticky top-0 z-10 w-24 bg-card text-right">Tokens</TableHead>
            <TableHead class="sticky top-0 z-10 w-20 bg-card text-right">TTFT</TableHead>
            <TableHead class="sticky top-0 z-10 w-20 bg-card text-right">延迟</TableHead>
            <TableHead class="sticky top-0 z-10 w-20 bg-card text-right">成本</TableHead>
            <TableHead class="sticky top-0 z-10 w-8 bg-card" />
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow
            v-for="t in filtered"
            :key="t.id"
            class="cursor-pointer transition-colors"
            @click="$router.push(`/traces/${t.id}`)"
          >
            <TableCell>
              <Badge variant="outline" :class="['text-[10px]', statusBadge(t).cls]">
                {{ statusBadge(t).label }}
              </Badge>
            </TableCell>
            <TableCell class="font-mono text-xs text-muted-foreground">
              {{ formatDateTime(t.createdAt) }}
            </TableCell>
            <TableCell>
              <div class="flex items-center gap-1.5">
                <span class="truncate font-mono text-xs">{{ t.model }}</span>
                <Badge
                  v-if="t.interface === 'WsRpc'"
                  variant="secondary"
                  class="shrink-0 px-1 text-[9px]"
                  >WS</Badge
                >
                <TooltipProvider v-if="hasSnapshot(t)">
                  <Tooltip>
                    <TooltipTrigger as-child>
                      <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-chart-4" />
                    </TooltipTrigger>
                    <TooltipContent class="text-xs">含内容快照</TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              </div>
              <div v-if="t.errorMessage" class="mt-0.5 truncate text-[11px] text-destructive">
                {{ t.errorMessage }}
              </div>
            </TableCell>
            <TableCell class="font-mono text-xs text-muted-foreground">{{
              t.tokenPrefix
            }}</TableCell>
            <TableCell class="text-right font-mono text-xs">
              {{ t.totalTokens != null ? formatTokens(t.totalTokens) : "—" }}
            </TableCell>
            <TableCell class="text-right font-mono text-xs text-muted-foreground">
              {{ t.ttftMs != null ? `${t.ttftMs}ms` : "—" }}
            </TableCell>
            <TableCell class="text-right font-mono text-xs text-muted-foreground">
              {{ t.latencyMs != null ? `${(t.latencyMs / 1000).toFixed(1)}s` : "—" }}
            </TableCell>
            <TableCell class="text-right font-mono text-xs">{{ formatCost(t.costUsd) }}</TableCell>
            <TableCell>
              <ChevronRight class="h-4 w-4 text-muted-foreground" />
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
    </Card>
  </PageShell>
</template>
