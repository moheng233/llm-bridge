<script setup lang="ts">
import { type TraceSummary } from "@bindings/TraceSummary";
import { type UsageScope } from "@bindings/UsageScope";
import { type UsageSummaryResponse } from "@bindings/UsageSummaryResponse";
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
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from "reka-ui";

import { formatTokens, getApi } from "~/lib/api";
import { statusBadgeFor } from "~/lib/trace-status";
import { formatApiError } from "~/lib/utils/error";
import { useAuthStore } from "~/stores/auth";
const auth = useAuthStore();

const api = getApi();

// ── 数据源（O4 后端 API）──

const RANGE_OPTIONS = [
  { value: "7", label: "近 7 天" },
  { value: "14", label: "近 14 天" },
  { value: "30", label: "近 30 天" },
] as const;

const rangeDays = ref("14");
const scope = ref<UsageScope>(auth.isAdmin ? "all" : "mine");
const effectiveScope = computed<UsageScope>(() => (auth.isAdmin ? scope.value : "mine"));
const summary = ref<UsageSummaryResponse | null>(null);
const recentTraces = ref<TraceSummary[]>([]);
const loading = ref(false);
const error = ref("");
const recentError = ref("");
let loadSequence = 0;

async function load() {
  const sequence = ++loadSequence;
  const selectedScope = effectiveScope.value;
  const days = Number(rangeDays.value);
  const userId = auth.user?.userId;
  summary.value = null;
  recentTraces.value = [];
  error.value = "";
  recentError.value = "";
  loading.value = auth.isAuthenticated;
  if (!auth.isAuthenticated) return;
  const [summaryResponse, traceResponse] = await Promise.allSettled([
    api.usage.getUsageSummary({ days, scope: selectedScope }),
    api.usage.listTraces({
      scope: selectedScope,
      status: null,
      model: null,
      tokenId: null,
      interface: null,
      search: null,
      dateFrom: null,
      dateTo: null,
      page: 0,
      pageSize: 5,
    }),
  ]);
  if (
    sequence !== loadSequence ||
    selectedScope !== effectiveScope.value ||
    days !== Number(rangeDays.value) ||
    userId !== auth.user?.userId
  )
    return;
  if (summaryResponse.status === "fulfilled") summary.value = summaryResponse.value;
  else error.value = formatApiError(summaryResponse.reason).title;
  if (traceResponse.status === "fulfilled") recentTraces.value = traceResponse.value.items;
  else recentError.value = formatApiError(traceResponse.reason).title;
  loading.value = false;
}

watch([rangeDays, effectiveScope, () => auth.user?.userId], load, { immediate: true });
onBeforeUnmount(() => {
  loadSequence++;
});

const daily = computed(() => summary.value?.daily ?? []);
const ranking = computed(() => (summary.value?.modelRanking ?? []).slice(0, 6));
const kpis = computed(
  () =>
    summary.value ?? {
      totalRequests: 0,
      totalTokens: 0,
      totalCostUsd: 0,
      errorRate: 0,
      avgTtftMs: null,
    },
);

/** 同长度上一周期汇总（后端 prevSummary；缺数据/无请求时为 null，前端不得伪造百分比）。 */
type PrevTotals = { totalRequests: number; totalTokens: number; totalCostUsd: number } | null;
const prev = computed<PrevTotals>(() => {
  const p = (summary.value as UsageSummaryResponse & { prevSummary?: PrevTotals })?.prevSummary;
  if (!p) return null;
  // 上一周期全为 0 时无法计算有意义百分比，按缺数据处理
  if (p.totalRequests === 0 && p.totalTokens === 0 && p.totalCostUsd === 0) return null;
  return p;
});

/** 真实环比变化：返回 null 表示上周期缺数据（UI 显示"上周期无数据"）。 */
function delta(cur: number, prevVal: number | undefined | null): number | null {
  if (prevVal == null) return null;
  if (prevVal === 0) return cur === 0 ? 0 : null;
  return ((cur - prevVal) / prevVal) * 100;
}

const deltas = computed(() => ({
  requests: delta(kpis.value.totalRequests, prev.value?.totalRequests),
  tokens: delta(kpis.value.totalTokens, prev.value?.totalTokens),
  cost: delta(kpis.value.totalCostUsd, prev.value?.totalCostUsd),
}));

const maxTokens = computed(() =>
  Math.max(...daily.value.map((d) => d.inputTokens + d.outputTokens + d.cachedTokens), 1),
);
const maxRankTokens = computed(() => Math.max(...ranking.value.map((r) => r.totalTokens), 1));

function formatCost(usd: number): string {
  if (usd >= 100) return `$${usd.toFixed(0)}`;
  if (usd >= 1) return `$${usd.toFixed(2)}`;
  return `$${usd.toFixed(3)}`;
}

/** 缓存率 = 缓存 tokens / 输入 tokens（缓存命中是输入前缀的重用） */
function cacheRate(d: { inputTokens: number; cachedTokens: number }): string {
  if (d.inputTokens <= 0) return "0.0";
  return ((d.cachedTokens / d.inputTokens) * 100).toFixed(1);
}
</script>

<template>
  <TabsRoot
    :model-value="effectiveScope"
    @update:model-value="(value) => (scope = value as UsageScope)"
    class="h-full min-h-0"
  >
    <PageShell>
      <template #header>
        <SectionHeader title="概览" :icon="LayoutDashboard" />
        <Teleport to="#page-header-actions" defer :disabled="!auth.isAuthenticated">
          <TabsList
            v-if="auth.isAdmin"
            aria-label="统计范围"
            class="grid w-24 shrink-0 grid-cols-2 rounded-md border bg-muted p-0.5 md:w-22"
          >
            <TabsTrigger
              value="all"
              class="min-h-11 rounded-sm px-1 text-xs font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-xs md:min-h-8"
              >全站</TabsTrigger
            >
            <TabsTrigger
              value="mine"
              class="min-h-11 rounded-sm px-1 text-xs font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-xs md:min-h-8"
              >我的</TabsTrigger
            >
          </TabsList>
          <Select v-model="rangeDays">
            <SelectTrigger
              class="w-24 gap-1 px-2 sm:w-32 sm:gap-2 sm:px-3"
              aria-label="统计时间范围"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="opt in RANGE_OPTIONS" :key="opt.value" :value="opt.value">
                {{ opt.label }}
              </SelectItem>
            </SelectContent>
          </Select>
        </Teleport>
      </template>
      <TabsContent
        :key="effectiveScope"
        :value="effectiveScope"
        :aria-label="effectiveScope === 'all' ? '全站指标' : '我的指标'"
        v-bind="auth.isAdmin ? {} : { role: 'region', 'aria-labelledby': undefined }"
        class="space-y-4"
        :aria-busy="loading"
      >
        <ErrorState v-if="error" :error="error" inline @retry="load" />

        <div
          v-if="loading && !summary"
          role="status"
          aria-label="加载指标"
          class="grid grid-cols-2 gap-4 xl:grid-cols-4"
        >
          <Skeleton v-for="i in 4" :key="i" class="h-24 rounded-xl" />
        </div>
        <template v-if="summary && !error">
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
                <span v-if="deltas.requests == null" class="text-xs text-muted-foreground">
                  上周期无数据
                </span>
                <span
                  v-else-if="deltas.requests === 0"
                  class="flex items-center gap-1 text-xs text-muted-foreground"
                >
                  持平（与上周期相比）
                </span>
                <span v-else class="flex items-center gap-1 text-xs text-muted-foreground">
                  <ArrowUpRight v-if="deltas.requests > 0" class="h-3 w-3" />
                  <ArrowDownRight v-else class="h-3 w-3" />
                  {{ deltas.requests > 0 ? "+" : "" }}{{ deltas.requests.toFixed(1) }}% 环比
                </span>
              </CardContent>
            </Card>

            <Card class="gap-2 py-4">
              <CardHeader class="px-4 pb-0">
                <div class="flex items-center justify-between">
                  <CardDescription class="text-xs">Token 消耗</CardDescription>
                  <Zap class="h-4 w-4 text-muted-foreground" />
                </div>
                <CardTitle class="font-mono text-2xl">{{
                  formatTokens(kpis.totalTokens)
                }}</CardTitle>
              </CardHeader>
              <CardContent class="px-4 pt-0">
                <span v-if="deltas.tokens == null" class="text-xs text-muted-foreground">
                  上周期无数据
                </span>
                <span
                  v-else-if="deltas.tokens === 0"
                  class="flex items-center gap-1 text-xs text-muted-foreground"
                >
                  持平（与上周期相比）
                </span>
                <span v-else class="flex items-center gap-1 text-xs text-muted-foreground">
                  <ArrowUpRight v-if="deltas.tokens > 0" class="h-3 w-3" />
                  <ArrowDownRight v-else class="h-3 w-3" />
                  {{ deltas.tokens > 0 ? "+" : "" }}{{ deltas.tokens.toFixed(1) }}% 环比
                </span>
              </CardContent>
            </Card>

            <Card class="gap-2 py-4">
              <CardHeader class="px-4 pb-0">
                <div class="flex items-center justify-between">
                  <CardDescription class="text-xs">已记录估算成本</CardDescription>
                  <Coins class="h-4 w-4 text-muted-foreground" />
                </div>
                <CardTitle class="font-mono text-2xl">{{
                  formatCost(kpis.totalCostUsd)
                }}</CardTitle>
              </CardHeader>
              <CardContent class="px-4 pt-0">
                <span v-if="deltas.cost == null" class="text-xs text-muted-foreground">
                  上周期无数据
                </span>
                <span
                  v-else-if="deltas.cost === 0"
                  class="flex items-center gap-1 text-xs text-muted-foreground"
                >
                  持平（与上周期相比）
                </span>
                <span v-else class="flex items-center gap-1 text-xs text-muted-foreground">
                  <ArrowUpRight v-if="deltas.cost > 0" class="h-3 w-3" />
                  <ArrowDownRight v-else class="h-3 w-3" />
                  {{ deltas.cost > 0 ? "+" : "" }}{{ deltas.cost.toFixed(1) }}% 环比
                </span>
              </CardContent>
            </Card>

            <Card class="gap-2 py-4">
              <CardHeader class="px-4 pb-0">
                <div class="flex items-center justify-between">
                  <CardDescription class="text-xs">错误率</CardDescription>
                  <AlertTriangle class="h-4 w-4 text-muted-foreground" />
                </div>
                <CardTitle class="font-mono text-2xl">
                  {{ (kpis.errorRate * 100).toFixed(1) }}%
                </CardTitle>
              </CardHeader>
              <CardContent class="px-4 pt-0">
                <span class="flex items-center gap-1 text-xs text-muted-foreground">
                  <Clock3 class="h-3 w-3" /> 平均首 Token 延迟：{{
                    kpis.avgTtftMs === null ? "—" : `${kpis.avgTtftMs} ms`
                  }}
                </span>
              </CardContent>
            </Card>
          </div>
          <p class="text-sm text-muted-foreground">
            成本基于已有 usage 与配置价格估算，非上游账单；缺价格或缺 usage 不代表免费。
          </p>

          <!-- Token 消耗趋势（自绘柱状图） -->
          <Card class="gap-3 py-4">
            <CardHeader class="px-4 pb-0">
              <div class="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <CardTitle class="text-lg font-semibold">Token 用量趋势</CardTitle>
                  <CardDescription class="text-xs"
                    >按日聚合输入 / 输出（含推理）/ 缓存 tokens</CardDescription
                  >
                </div>
                <div class="flex items-center gap-4 text-xs text-muted-foreground">
                  <span class="flex items-center gap-1.5">
                    <span class="h-2.5 w-2.5 rounded-sm bg-primary" /> 输入
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
              <div class="flex h-44 items-end gap-1.5 border-b border-border">
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
                          class="w-full bg-primary transition-opacity group-hover:opacity-80"
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
                        <span>
                          缓存 {{ formatTokens(d.cachedTokens) }}（{{ cacheRate(d) }}%）
                        </span>
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
                  class="min-w-0 flex-1 truncate text-center font-mono text-xs text-muted-foreground"
                  >{{ i % 2 === 0 ? d.day.slice(5) : "" }}</span
                >
              </div>
            </CardContent>
          </Card>

          <!-- 模型排行 + 最近请求 -->
          <div class="grid gap-4 lg:grid-cols-2">
            <Card class="gap-3 py-4">
              <CardHeader class="px-4 pb-0">
                <CardTitle class="text-lg font-semibold">模型用量排行</CardTitle>
                <CardDescription class="text-xs">按总 token 消耗排序</CardDescription>
              </CardHeader>
              <CardContent class="flex min-h-44 flex-col gap-3 px-4 pt-1">
                <div v-for="r in ranking" :key="r.model" class="flex flex-col gap-1">
                  <div class="flex items-center justify-between gap-2 text-xs">
                    <span class="truncate font-mono">{{ r.model }}</span>
                    <span class="shrink-0 font-mono text-muted-foreground">
                      {{ formatTokens(r.totalTokens) }} · {{ formatCost(r.costUsd) }}
                    </span>
                  </div>
                  <div class="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                    <div
                      class="h-full rounded-full bg-primary"
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
                    <CardTitle class="text-lg font-semibold">最近请求</CardTitle>
                    <CardDescription class="text-xs">最新的 5 条请求追踪</CardDescription>
                  </div>
                  <Button
                    variant="ghost"
                    size="sm"
                    class="cursor-pointer px-2 text-sm"
                    @click="$router.push({ path: '/traces', query: { scope: effectiveScope } })"
                  >
                    查看全部 →
                  </Button>
                </div>
              </CardHeader>
              <CardContent class="flex min-h-44 flex-col px-4 pt-1">
                <ErrorState v-if="recentError" :error="recentError" @retry="load" />
                <button
                  v-for="t in recentTraces"
                  :key="t.requestId"
                  class="-mx-2 flex cursor-pointer items-center justify-between gap-2 rounded-md px-2 py-2 text-left transition-colors hover:bg-accent/50"
                  @click="$router.push(`/traces/${t.requestId}`)"
                >
                  <div class="flex min-w-0 items-center gap-2">
                    <Badge
                      variant="outline"
                      :class="['shrink-0 text-xs', statusBadgeFor(t.status).cls]"
                    >
                      {{ statusBadgeFor(t.status).label }}
                    </Badge>
                    <span class="truncate font-mono text-xs">{{ t.model }}</span>
                  </div>
                  <span class="shrink-0 text-xs text-muted-foreground tabular-nums">
                    {{ t.latencyMs != null ? `${(t.latencyMs / 1000).toFixed(1)}s` : "—" }}
                  </span>
                </button>
              </CardContent>
            </Card>
          </div>
        </template>
      </TabsContent>
    </PageShell>
  </TabsRoot>
</template>
