<script setup lang="ts">
import { type TokenListItem } from "@bindings/TokenListItem";
import { type TraceSummary } from "@bindings/TraceSummary";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  ScrollText,
  Search,
} from "@lucide/vue";

import { Button } from "~/components/ui/button";
import { useApiCall } from "~/composables/useApiCall";
import { getApi, formatTokens } from "~/lib/api";
import { statusBadgeFor } from "~/lib/trace-status";
import { useAuthStore } from "~/stores/auth";
const auth = useAuthStore();
const localModels = useApiCall(() => getApi().models.listAllModels());

const api = getApi();

// ── 筛选状态（服务端筛选）──

const search = ref("");
const statusFilter = ref("all");
const modelFilter = ref("");
const tokenFilter = ref("all"); // "all" | TokenListItem.id 字符串
// 日期筛选（本地时区 yyyy-mm-dd → 当日 00:00 / 24:00 unix 秒；空串 = 不限）
const dateFrom = ref("");
const dateTo = ref("");

const STATUS_OPTIONS = [
  { value: "all", label: "全部状态" },
  { value: "success", label: "成功" },
  { value: "error", label: "失败" },
  { value: "cancelled", label: "已取消" },
  { value: "streaming", label: "进行中" },
  { value: "pending", label: "等待中" },
];

const PAGE_SIZE = 50;
const page = ref(0);
const traces = ref<TraceSummary[]>([]);
const total = ref(0);

/** 本地 yyyy-mm-dd → 当日起止 unix 秒 */
function dayRange(dateStr: string, endOfDay: boolean): number | null {
  if (!dateStr) return null;
  const d = new Date(`${dateStr}T00:00:00`);
  if (Number.isNaN(d.getTime())) return null;
  return Math.floor((endOfDay ? d.getTime() + 24 * 3600_000 - 1 : d.getTime()) / 1000);
}

const {
  loading,
  error,
  execute: fetchTraces,
} = useApiCall(() => {
  const from = dayRange(dateFrom.value, false);
  const to = dayRange(dateTo.value, true);
  return api.usage.listTraces({
    status: statusFilter.value === "all" ? null : statusFilter.value,
    model: modelFilter.value.trim() || null,
    tokenId: tokenFilter.value === "all" ? null : Number(tokenFilter.value),
    interface: null,
    search: search.value.trim() || null,
    dateFrom: from,
    dateTo: to,
    page: page.value,
    pageSize: PAGE_SIZE,
  });
});

async function load() {
  const r = await fetchTraces();
  if (r) {
    // 翻页越界回退到最后一页
    const pages = Math.max(1, Math.ceil(r.total / PAGE_SIZE));
    if (page.value >= pages) {
      page.value = pages - 1;
      return;
    }
    traces.value = r.items;
    total.value = r.total;
  }
}

// 筛选变更立即重置到第一页并重新查询；搜索输入防抖 300ms
watch([statusFilter, modelFilter, tokenFilter, dateFrom, dateTo], () => {
  if (page.value !== 0) page.value = 0;
  else load();
});
let searchTimer: ReturnType<typeof setTimeout> | undefined;
watch(search, () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    if (page.value !== 0) page.value = 0;
    else load();
  }, 300);
});
watch(page, load);
onMounted(() => {
  load();
  localModels.execute();
});
onBeforeUnmount(() => clearTimeout(searchTimer));

const totalPages = computed(() => Math.max(1, Math.ceil(total.value / PAGE_SIZE)));
const pageFrom = computed(() => (total.value === 0 ? 0 : page.value * PAGE_SIZE + 1));
const pageTo = computed(() => Math.min(total.value, (page.value + 1) * PAGE_SIZE));

function goPage(p: number) {
  if (p < 0 || p >= totalPages.value || p === page.value) return;
  page.value = p;
}

const modelOptions = computed(() =>
  (localModels.data.value ?? []).map((model) => model.modelName).sort(),
);
const filtered = computed(
  () =>
    !!(
      search.value ||
      modelFilter.value ||
      dateFrom.value ||
      dateTo.value ||
      statusFilter.value !== "all" ||
      tokenFilter.value !== "all"
    ),
);
function clearFilters() {
  search.value = "";
  modelFilter.value = "";
  dateFrom.value = "";
  dateTo.value = "";
  statusFilter.value = "all";
  tokenFilter.value = "all";
}

// Token 下拉选项：当前用户全部 Token（本人数据，后端再按 token_id 过滤）
const tokens = ref<TokenListItem[]>([]);
const { execute: fetchTokens } = useApiCall(() => api.tokens.listTokens());
watchEffect(async () => {
  const list = await fetchTokens();
  if (list) tokens.value = list;
});

function statusBadge(t: TraceSummary): { label: string; cls: string } {
  return statusBadgeFor(t.status);
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
</script>

<template>
  <PageShell>
    <SectionHeader
      title="请求记录"
      :description="auth.isAdmin ? '全站请求与用量 · 管理员视角' : '我的请求与用量 · 仅当前用户'"
      :icon="ScrollText"
      :count="total"
      count-label="条"
    />

    <!-- 筛选栏 -->
    <div class="flex flex-wrap items-center gap-2">
      <div class="relative min-w-52 flex-1">
        <Search class="absolute top-1/2 left-2.5 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          v-model="search"
          aria-label="搜索请求"
          placeholder="搜索 Request ID / 模型 / 错误信息…"
          class="pl-8"
        />
      </div>
      <Select v-model="statusFilter">
        <SelectTrigger class="w-28" aria-label="筛选请求状态">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="opt in STATUS_OPTIONS" :key="opt.value" :value="opt.value">
            {{ opt.label }}
          </SelectItem>
        </SelectContent>
      </Select>
      <Input
        v-model="modelFilter"
        list="trace-model-options"
        class="w-full sm:w-60"
        placeholder="精确模型 ID（含已删除模型）"
        aria-label="筛选精确模型 ID"
      />
      <datalist id="trace-model-options">
        <option v-for="model in modelOptions" :key="model" :value="model" />
      </datalist>
      <Select v-model="tokenFilter">
        <SelectTrigger class="w-48" aria-label="筛选我的令牌">
          <SelectValue placeholder="不限令牌（候选为我的令牌）" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="all">不限令牌</SelectItem>
          <SelectItem v-for="t in tokens" :key="t.id" :value="String(t.id)">
            {{ t.name }}（{{ t.tokenPrefix }}）
          </SelectItem>
        </SelectContent>
      </Select>
      <Input v-model="dateFrom" type="date" class="w-36" aria-label="起始日期" />
      <span class="text-xs text-muted-foreground">至</span>
      <Input v-model="dateTo" type="date" class="w-36" aria-label="结束日期" />
    </div>
    <p class="text-sm text-muted-foreground">
      模型可直接输入历史 ID；下拉令牌候选仅包含我的令牌。成本为基于记录的估算，不是上游账单。
    </p>

    <ErrorState v-if="error" :error="error" inline @retry="load" />

    <!-- Loading -->
    <div v-if="loading" class="flex flex-col gap-3">
      <Skeleton v-for="i in 6" :key="i" class="h-12 w-full rounded-lg" />
    </div>

    <!-- 空态 -->
    <div v-else-if="!error && traces.length === 0" class="space-y-3 rounded border bg-card p-6">
      <p>{{ filtered ? "筛选无结果" : "尚无请求记录" }}</p>
      <Button v-if="filtered" variant="outline" @click="clearFilters">清除筛选</Button
      ><Button v-else as-child variant="outline"
        ><RouterLink to="/models">查看模型接入方式</RouterLink></Button
      >
    </div>

    <Card v-else-if="!error" class="gap-0 overflow-hidden py-0">
      <Table>
        <TableHeader>
          <TableRow class="hover:bg-transparent">
            <TableHead class="sticky top-0 z-10 w-20 bg-card">状态</TableHead>
            <TableHead class="sticky top-0 z-10 w-40 bg-card">时间</TableHead>
            <TableHead class="sticky top-0 z-10 bg-card">模型</TableHead>
            <TableHead class="sticky top-0 z-10 w-28 bg-card">访问令牌</TableHead>
            <TableHead class="sticky top-0 z-10 w-24 bg-card text-right">Token 用量</TableHead>
            <TableHead class="sticky top-0 z-10 w-20 bg-card text-right">TTFT</TableHead>
            <TableHead class="sticky top-0 z-10 w-20 bg-card text-right">延迟</TableHead>
            <TableHead class="sticky top-0 z-10 w-20 bg-card text-right">估算成本</TableHead>
            <TableHead class="sticky top-0 z-10 w-8 bg-card" />
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow
            v-for="t in traces"
            :key="t.requestId"
            class="cursor-pointer transition-colors"
            @click="$router.push(`/traces/${t.requestId}`)"
          >
            <TableCell>
              <Badge variant="outline" :class="['text-xs', statusBadge(t).cls]">
                {{ statusBadge(t).label }}
              </Badge>
            </TableCell>
            <TableCell class="font-mono text-xs text-muted-foreground">
              {{ formatDateTime(t.createdAt) }}
            </TableCell>
            <TableCell>
              <div class="flex items-center gap-1.5">
                <RouterLink
                  :to="`/traces/${t.requestId}`"
                  class="font-mono text-sm break-all text-primary underline"
                  >{{ t.model }}</RouterLink
                >
                <Badge
                  v-if="t.interface === 'ws_rpc'"
                  variant="secondary"
                  class="shrink-0 px-1 text-xs"
                  >WS</Badge
                >
                <TooltipProvider v-if="t.hasSnapshot">
                  <Tooltip>
                    <TooltipTrigger as-child>
                      <span class="text-xs text-muted-foreground">有快照</span>
                    </TooltipTrigger>
                    <TooltipContent class="text-xs">含内容快照</TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              </div>
              <div v-if="t.errorMessage" class="mt-0.5 truncate text-xs text-destructive">
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
            <TableCell class="text-right font-mono text-xs">
              {{ formatCost(t.costUsd) }}
            </TableCell>
            <TableCell>
              <ChevronRight class="h-4 w-4 text-muted-foreground" />
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
    </Card>
    <!-- 分页 -->
    <div
      v-if="!loading && !error && total > 0"
      class="flex shrink-0 flex-wrap items-center justify-between gap-2 text-sm text-muted-foreground"
    >
      <span>第 {{ pageFrom }}–{{ pageTo }} 条，共 {{ total }} 条</span>
      <div class="flex items-center gap-1">
        <Button
          variant="outline"
          size="icon"
          class="cursor-pointer"
          title="第一页"
          :disabled="page === 0"
          aria-label="第一页"
          @click="goPage(0)"
        >
          <ChevronsLeft class="h-3.5 w-3.5" />
        </Button>
        <Button
          variant="outline"
          size="icon"
          class="cursor-pointer"
          title="上一页"
          :disabled="page === 0"
          aria-label="上一页"
          @click="goPage(page - 1)"
        >
          <ChevronLeft class="h-3.5 w-3.5" />
        </Button>
        <span class="px-2 font-mono">{{ page + 1 }} / {{ totalPages }}</span>
        <Button
          variant="outline"
          size="icon"
          class="cursor-pointer"
          title="下一页"
          :disabled="page >= totalPages - 1"
          aria-label="下一页"
          @click="goPage(page + 1)"
        >
          <ChevronRight class="h-3.5 w-3.5" />
        </Button>
        <Button
          variant="outline"
          size="icon"
          class="cursor-pointer"
          title="最后一页"
          :disabled="page >= totalPages - 1"
          aria-label="最后一页"
          @click="goPage(totalPages - 1)"
        >
          <ChevronsRight class="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  </PageShell>
</template>
