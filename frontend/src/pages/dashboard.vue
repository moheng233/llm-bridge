<script setup lang="ts">
import {
  Activity,
  AlertTriangle,
  ArrowDownRight,
  ArrowUpRight,
  Clock3,
  Coins,
  LayoutDashboard,
  Zap,
} from "@lucide/vue";

import { formatTokens } from "~/lib/api";
import {
  MOCK_TRACES,
  MOCK_USAGE_DAILY,
  aggregateByDay,
  rankModels,
  summarizeUsage,
} from "~/lib/observability-mock";

// ── 数据源（示例数据，O4 API 就绪后替换）──

const RANGE_OPTIONS = [
  { value: "7", label: "近 7 天" },
  { value: "14", label: "近 14 天" },
  { value: "30", label: "近 30 天" },
] as const;

const rangeDays = ref("14");

const filteredDaily = computed(() => MOCK_USAGE_DAILY.slice(-Number(rangeDays.value) * 6));
const daily = computed(() => aggregateByDay(filteredDaily.value));
const kpis = computed(() => summarizeUsage(filteredDaily.value, MOCK_TRACES));
const ranking = computed(() => rankModels(filteredDaily.value).slice(0, 6));
const recentTraces = computed(() => MOCK_TRACES.slice(0, 5));

const maxTokens = computed(() =>
  Math.max(...daily.value.map((d) => d.inputTokens + d.outputTokens + d.cachedTokens), 1),
);
const maxRankTokens = computed(() => Math.max(...ranking.value.map((r) => r.totalTokens), 1));

function formatCost(usd: number): string {
  if (usd >= 100) return `$${usd.toFixed(0)}`;
  if (usd >= 1) return `$${usd.toFixed(2)}`;
  return `$${usd.toFixed(3)}`;
}

function statusBadge(status: string): { label: string; cls: string } {
  switch (status) {
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

/** 缓存率 = 缓存 tokens / 输入 tokens（缓存命中是输入前缀的重用） */
function cacheRate(d: { inputTokens: number; cachedTokens: number }): string {
  if (d.inputTokens <= 0) return "0.0";
  return ((d.cachedTokens / d.inputTokens) * 100).toFixed(1);
}
</script>

<template>
  <PageShell>
    <SectionHeader
      title="用量仪表盘"
      description="请求量、Token 消耗与成本总览（示例数据）"
      :icon="LayoutDashboard"
    >
      <template #actions>
        <Select v-model="rangeDays">
          <SelectTrigger class="w-32">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="opt in RANGE_OPTIONS" :key="opt.value" :value="opt.value">
              {{ opt.label }}
            </SelectItem>
          </SelectContent>
        </Select>
      </template>
    </SectionHeader>

    <!-- KPI 卡片 -->
    <div class="grid grid-cols-2 gap-4 xl:grid-cols-4">
      <Card class="gap-2 py-4">
        <CardHeader class="px-4 pb-0">
          <div class="flex items-center justify-between">
            <CardDescription class="text-xs">总请求数</CardDescription>
            <Activity class="h-4 w-4 text-muted-foreground" />
          </div>
          <CardTitle class="font-mono text-2xl">{{
            kpis.totalRequests.toLocaleString()
          }}</CardTitle>
        </CardHeader>
        <CardContent class="px-4 pt-0">
          <span class="flex items-center gap-1 text-xs text-cta">
            <ArrowUpRight class="h-3 w-3" /> +12.0% 环比
          </span>
        </CardContent>
      </Card>

      <Card class="gap-2 py-4">
        <CardHeader class="px-4 pb-0">
          <div class="flex items-center justify-between">
            <CardDescription class="text-xs">Token 消耗</CardDescription>
            <Zap class="h-4 w-4 text-muted-foreground" />
          </div>
          <CardTitle class="font-mono text-2xl">{{ formatTokens(kpis.totalTokens) }}</CardTitle>
        </CardHeader>
        <CardContent class="px-4 pt-0">
          <span class="flex items-center gap-1 text-xs text-cta">
            <ArrowUpRight class="h-3 w-3" /> +8.4% 环比
          </span>
        </CardContent>
      </Card>

      <Card class="gap-2 py-4">
        <CardHeader class="px-4 pb-0">
          <div class="flex items-center justify-between">
            <CardDescription class="text-xs">估算成本</CardDescription>
            <Coins class="h-4 w-4 text-muted-foreground" />
          </div>
          <CardTitle class="font-mono text-2xl">{{ formatCost(kpis.totalCostUsd) }}</CardTitle>
        </CardHeader>
        <CardContent class="px-4 pt-0">
          <span class="flex items-center gap-1 text-xs text-muted-foreground">
            <ArrowDownRight class="h-3 w-3 text-cta" /> -3.1% 环比
          </span>
        </CardContent>
      </Card>

      <Card class="gap-2 py-4">
        <CardHeader class="px-4 pb-0">
          <div class="flex items-center justify-between">
            <CardDescription class="text-xs">错误率 / 平均 TTFT</CardDescription>
            <AlertTriangle class="h-4 w-4 text-muted-foreground" />
          </div>
          <CardTitle class="font-mono text-2xl">
            {{ (kpis.errorRate * 100).toFixed(1) }}%
            <span class="text-sm font-normal text-muted-foreground">/ {{ kpis.avgTtftMs }}ms</span>
          </CardTitle>
        </CardHeader>
        <CardContent class="px-4 pt-0">
          <span class="flex items-center gap-1 text-xs text-muted-foreground">
            <Clock3 class="h-3 w-3" /> 首 token 延迟
          </span>
        </CardContent>
      </Card>
    </div>

    <!-- Token 消耗趋势（自绘柱状图） -->
    <Card class="gap-3 py-4">
      <CardHeader class="px-4 pb-0">
        <div class="flex items-center justify-between">
          <div>
            <CardTitle class="text-sm font-medium">Token 消耗趋势</CardTitle>
            <CardDescription class="text-xs"
              >按日聚合输入 / 输出（含推理）/ 缓存 tokens</CardDescription
            >
          </div>
          <div class="flex items-center gap-4 text-xs text-muted-foreground">
            <span class="flex items-center gap-1.5">
              <span class="h-2.5 w-2.5 rounded-sm bg-cta" /> 输入
            </span>
            <span class="flex items-center gap-1.5">
              <span class="h-2.5 w-2.5 rounded-sm bg-chart-2" /> 输出
            </span>
            <span class="flex items-center gap-1.5">
              <span class="h-2.5 w-2.5 rounded-sm bg-chart-4" /> 缓存
            </span>
          </div>
        </div>
      </CardHeader>
      <CardContent class="px-4 pt-1">
        <div class="flex h-44 items-end gap-1.5">
          <TooltipProvider v-for="d in daily" :key="d.day">
            <Tooltip>
              <TooltipTrigger as-child>
                <div class="group flex h-full min-w-0 flex-1 flex-col justify-end gap-0.5">
                  <!-- 堆叠顺序（自下而上）：缓存 → 输入 → 输出 -->
                  <div
                    class="w-full rounded-t-sm bg-chart-2 transition-opacity group-hover:opacity-80"
                    :style="{ height: `${(d.outputTokens / maxTokens) * 100}%` }"
                  />
                  <div
                    class="w-full bg-cta transition-opacity group-hover:opacity-80"
                    :style="{ height: `${(d.inputTokens / maxTokens) * 100}%` }"
                  />
                  <div
                    class="w-full bg-chart-4 transition-opacity group-hover:opacity-80"
                    :style="{ height: `${(d.cachedTokens / maxTokens) * 100}%` }"
                  />
                </div>
              </TooltipTrigger>
              <TooltipContent>
                <div class="flex flex-col gap-0.5 font-mono text-xs">
                  <span class="font-semibold">{{ d.day }}</span>
                  <span>输入 {{ formatTokens(d.inputTokens) }}</span>
                  <span>输出 {{ formatTokens(d.outputTokens) }}</span>
                  <span> 缓存 {{ formatTokens(d.cachedTokens) }}（{{ cacheRate(d) }}%） </span>
                  <span>{{ d.requests }} 次请求 · {{ formatCost(d.costUsd) }}</span>
                </div>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
        <div class="mt-2 flex gap-1.5">
          <span
            v-for="(d, i) in daily"
            :key="d.day"
            class="min-w-0 flex-1 truncate text-center font-mono text-[10px] text-muted-foreground"
            >{{ i % 2 === 0 ? d.label : "" }}</span
          >
        </div>
      </CardContent>
    </Card>

    <!-- 模型排行 + 最近请求 -->
    <div class="grid gap-4 lg:grid-cols-2">
      <Card class="gap-3 py-4">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-sm font-medium">模型用量排行</CardTitle>
          <CardDescription class="text-xs">按总 token 消耗排序</CardDescription>
        </CardHeader>
        <CardContent class="flex flex-col gap-3 px-4 pt-1">
          <div v-for="r in ranking" :key="r.model" class="flex flex-col gap-1">
            <div class="flex items-center justify-between gap-2 text-xs">
              <span class="truncate font-mono">{{ r.model }}</span>
              <span class="shrink-0 font-mono text-muted-foreground">
                {{ formatTokens(r.totalTokens) }} · {{ formatCost(r.costUsd) }}
              </span>
            </div>
            <div class="h-1.5 w-full overflow-hidden rounded-full bg-muted">
              <div
                class="h-full rounded-full bg-cta"
                :style="{ width: `${(r.totalTokens / maxRankTokens) * 100}%` }"
              />
            </div>
          </div>
        </CardContent>
      </Card>

      <Card class="gap-3 py-4">
        <CardHeader class="px-4 pb-0">
          <div class="flex items-center justify-between">
            <div>
              <CardTitle class="text-sm font-medium">最近请求</CardTitle>
              <CardDescription class="text-xs">最新的 5 条请求追踪</CardDescription>
            </div>
            <Button
              variant="ghost"
              size="sm"
              class="h-7 cursor-pointer px-2 text-xs"
              @click="$router.push('/traces')"
            >
              查看全部 →
            </Button>
          </div>
        </CardHeader>
        <CardContent class="flex flex-col px-4 pt-1">
          <button
            v-for="t in recentTraces"
            :key="t.id"
            class="-mx-2 flex cursor-pointer items-center justify-between gap-2 rounded-md px-2 py-2 text-left transition-colors hover:bg-accent/50"
            @click="$router.push(`/traces/${t.id}`)"
          >
            <div class="flex min-w-0 items-center gap-2">
              <Badge variant="outline" :class="['shrink-0 text-[10px]', statusBadge(t.status).cls]">
                {{ statusBadge(t.status).label }}
              </Badge>
              <span class="truncate font-mono text-xs">{{ t.model }}</span>
            </div>
            <span class="shrink-0 font-mono text-[11px] text-muted-foreground">
              {{ t.latencyMs != null ? `${(t.latencyMs / 1000).toFixed(1)}s` : "—" }}
            </span>
          </button>
        </CardContent>
      </Card>
    </div>
  </PageShell>
</template>
