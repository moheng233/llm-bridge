<script setup lang="ts">
import { type TraceDetail } from "@bindings/TraceDetail";
import {
  ArrowLeft,
  Bot,
  Brain,
  Check,
  Copy,
  FileJson,
  Info,
  ScrollText,
  Terminal,
  User,
  Wrench,
} from "@lucide/vue";

import { useApiCall } from "~/composables/useApiCall";
import { formatTokens, getApi } from "~/lib/api";
import {
  ROLE_ASSISTANT,
  ROLE_DEVELOPER,
  ROLE_SYSTEM,
  isDataPart,
  isTextPart,
  isThinkingPart,
  isToolCallPart,
  isToolResultPart,
  isUsagePart,
  type LanguageModelToolResultPart,
} from "~/lib/trace-parts";
import { statusBadgeFor } from "~/lib/trace-status";

const api = getApi();
const route = useRoute();
const router = useRouter();

// 按 requestId 查询（路由参数即 request_id）
const trace = ref<TraceDetail | null>(null);
const { loading, error, execute: fetchTrace } = useApiCall((id: string) => api.usage.getTrace(id));

async function load() {
  // 路由为 /traces/[id]，params.id 必为 string（typed-router 在非精确匹配时可能宽化为 never）
  const id = (route.params as { id?: string }).id ?? "";
  const t = await fetchTrace(id);
  trace.value = t ?? null;
}
watchEffect(load);

/** 内容快照。untagged union 在 Vue 模板深层类型推断会触发 TS2589，
 * 故以 any 透传，运行时由 trace-parts 类型守卫保证正确性。 */
const requestMessages = computed<any[] | null>(() => trace.value?.requestMessages ?? null);
const responseParts = computed<any[] | null>(() => trace.value?.responseParts ?? null);

const copiedField = ref<string | null>(null);
const copyFailure = ref<string | null>(null);
async function copyText(key: string, text: string) {
  try {
    if (!navigator.clipboard) throw Error();
    await navigator.clipboard.writeText(text);
    copiedField.value = key;
    copyFailure.value = null;
  } catch {
    copyFailure.value = text;
  }
}

// ── 状态与角色映射 ──

function statusBadge(status: string): { label: string; cls: string } {
  return statusBadgeFor(status);
}

function roleMeta(role: unknown): { label: string; icon: any; cls: string } {
  // role 恒为字符串字面量（后端字符串序列化），unknown 仅因快照字段 any 透传
  switch (role) {
    case ROLE_SYSTEM:
    case ROLE_DEVELOPER:
      return {
        label: role === ROLE_SYSTEM ? "System" : "Developer",
        icon: Terminal,
        cls: "text-chart-4",
      };
    case ROLE_ASSISTANT:
      return { label: "Assistant", icon: Bot, cls: "text-primary" };
    default:
      return { label: "User", icon: User, cls: "text-chart-2" };
  }
}

// ── 时间线 ──

const timeline = computed(() => {
  const t = trace.value;
  if (!t) return [];
  const items: Array<{ label: string; value: string }> = [
    { label: "创建", value: fmtTime(t.createdAt) },
  ];
  if (t.firstChunkAt != null)
    items.push({ label: "首块", value: t.ttftMs === null ? "—" : `+${t.ttftMs} ms` });
  if (t.completedAt != null)
    items.push({
      label: "完成",
      value: t.latencyMs === null ? "—" : `+${(t.latencyMs / 1000).toFixed(1)} s`,
    });
  return items;
});

function fmtTime(ts: number): string {
  return new Date(ts * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function fmtCost(usd: number | null): string {
  if (usd == null) return "—";
  return `$${usd.toFixed(4)}`;
}

function thinkingText(v: string | string[]): string {
  return Array.isArray(v) ? v.join("\n") : v;
}

function toolResultText(p: LanguageModelToolResultPart): string {
  return p.content
    .map((c) => (isTextPart(c) ? c.value : isDataPart(c) ? `[${c.mimeType}]` : JSON.stringify(c)))
    .join("\n");
}

// 折叠的长内容默认展开条数
const expandedThinking = ref<Set<number>>(new Set());
function toggleThinking(i: number) {
  const s = new Set(expandedThinking.value);
  if (s.has(i)) s.delete(i);
  else s.add(i);
  expandedThinking.value = s;
}
</script>

<template>
  <!-- Loading -->
  <PageShell v-if="loading">
    <div class="flex flex-col gap-4">
      <Skeleton class="h-10 w-full rounded-lg" />
      <Skeleton class="h-24 w-full rounded-xl" />
      <Skeleton class="h-48 w-full rounded-xl" />
    </div>
  </PageShell>

  <PageShell v-else-if="trace">
    <div class="flex shrink-0 flex-wrap items-center gap-3">
      <Button
        variant="ghost"
        size="icon"
        class="cursor-pointer"
        aria-label="返回请求记录"
        title="返回请求记录"
        @click="router.push('/traces')"
      >
        <ArrowLeft class="h-4 w-4" />
      </Button>
      <div class="flex min-w-0 flex-1 flex-wrap items-center gap-2">
        <h1 class="break-all">{{ trace.model }}</h1>
        <Badge variant="outline" :class="['shrink-0 text-xs', statusBadge(trace.status).cls]">
          {{ statusBadge(trace.status).label }}
        </Badge>
        <Badge v-if="trace.interface === 'ws_rpc'" variant="secondary" class="shrink-0 text-xs">
          WS RPC
        </Badge>
      </div>
      <div class="flex min-w-0 items-center gap-1 text-xs text-muted-foreground">
        <code class="break-all">{{ trace.requestId }}</code>
        <Button
          variant="ghost"
          size="icon"
          class="cursor-pointer"
          aria-label="复制 Request ID"
          title="复制 Request ID"
          @click="copyText('rid', trace.requestId)"
        >
          <Check v-if="copiedField === 'rid'" class="h-3 w-3 text-primary" />
          <Copy v-else class="h-3 w-3" />
        </Button>
      </div>
    </div>

    <p class="text-sm text-muted-foreground">
      {{ fmtTime(trace.createdAt) }} · 估算成本不是上游账单；缺价格或 usage 不代表免费。
    </p>
    <div v-if="copyFailure !== null" class="space-y-2 rounded border p-3">
      <p role="alert" class="text-destructive">复制失败，请手动选择以下文本。</p>
      <pre class="break-all whitespace-pre-wrap select-all">{{ copyFailure }}</pre>
    </div>
    <div class="flex min-w-0 flex-col gap-4">
      <!-- 元数据网格 -->
      <div class="grid shrink-0 grid-cols-2 gap-3 md:grid-cols-4">
        <Card class="gap-1 py-3">
          <CardContent class="px-4">
            <div class="text-xs text-muted-foreground">访问令牌</div>
            <div class="font-mono text-sm">{{ trace.tokenPrefix }}</div>
          </CardContent>
        </Card>
        <Card class="gap-1 py-3">
          <CardContent class="px-4">
            <div class="text-xs text-muted-foreground">实际路由（提供者 / 协议）</div>
            <div class="font-mono text-sm break-all">{{ trace.providerId ?? "—" }}</div>
            <div class="font-mono text-xs text-muted-foreground">{{ trace.protocol ?? "—" }}</div>
          </CardContent>
        </Card>
        <Card class="gap-1 py-3">
          <CardContent class="px-4">
            <div class="text-xs text-muted-foreground">时间线</div>
            <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5">
              <template v-for="(item, i) in timeline" :key="item.label">
                <span v-if="i > 0" class="text-muted-foreground">→</span>
                <span class="font-mono text-xs">
                  <span class="text-muted-foreground">{{ item.label }}</span> {{ item.value }}
                </span>
              </template>
            </div>
          </CardContent>
        </Card>
        <Card class="gap-1 py-3">
          <CardContent class="px-4">
            <div class="text-xs text-muted-foreground">估算成本 / 结束原因</div>
            <div class="font-mono text-sm">{{ fmtCost(trace.costUsd) }}</div>
            <div class="font-mono text-xs text-muted-foreground">
              {{ trace.finishReason ?? "—" }}
            </div>
          </CardContent>
        </Card>
      </div>

      <!-- 错误信息 -->
      <Alert v-if="trace.errorMessage" class="shrink-0 border-destructive/30 bg-destructive/5">
        <AlertDescription class="flex flex-col gap-1 text-sm">
          <div class="flex items-center gap-2">
            <Badge variant="destructive" class="text-xs">{{ trace.errorType }}</Badge>
            <span v-if="trace.upstreamStatus" class="font-mono text-xs text-muted-foreground">
              上游 HTTP {{ trace.upstreamStatus }}
            </span>
          </div>
          <span class="font-mono text-xs">{{ trace.errorMessage }}</span>
        </AlertDescription>
      </Alert>

      <!-- Token 用量 -->
      <Card class="shrink-0 gap-3 py-4">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-lg font-semibold">Token 用量</CardTitle>
          <CardDescription class="text-xs">
            预扣 {{ formatTokens(trace.estimatedTokens) }} · 实际结算以五元组为准
          </CardDescription>
        </CardHeader>
        <CardContent class="px-4 pt-1">
          <div class="grid grid-cols-2 gap-3 sm:grid-cols-5">
            <div
              v-for="u in [
                { label: '输入', value: trace.inputTokens },
                { label: '输出', value: trace.outputTokens },
                { label: '推理', value: trace.reasoningTokens },
                { label: '缓存命中', value: trace.cachedTokens },
                { label: '总计', value: trace.totalTokens },
              ]"
              :key="u.label"
              class="flex flex-col gap-0.5 rounded-lg border border-border/60 bg-muted/30 px-3 py-2"
            >
              <span class="text-xs text-muted-foreground">{{ u.label }}</span>
              <span class="font-mono text-sm font-semibold">
                {{ u.value != null ? formatTokens(u.value) : "—" }}
              </span>
            </div>
          </div>
        </CardContent>
      </Card>

      <!-- 请求消息 -->
      <Card v-if="requestMessages" class="shrink-0 gap-3 py-4">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-lg font-semibold">请求消息</CardTitle>
          <CardDescription class="text-xs">
            {{ requestMessages?.length ?? 0 }} 条消息（内容快照，Opt-In 采集）
          </CardDescription>
        </CardHeader>
        <CardContent class="flex flex-col gap-3 px-4 pt-1">
          <div
            v-for="(msg, i) in requestMessages ?? []"
            :key="i"
            class="rounded-lg border border-border/60 bg-muted/20 px-4 py-3"
          >
            <div class="mb-2 flex items-center gap-2">
              <component
                :is="roleMeta(msg.role).icon"
                :class="['h-3.5 w-3.5', roleMeta(msg.role).cls]"
              />
              <span :class="['font-mono text-xs font-semibold', roleMeta(msg.role).cls]">
                {{ roleMeta(msg.role).label }}
              </span>
              <span v-if="msg.name" class="font-mono text-xs text-muted-foreground">
                ({{ msg.name }})
              </span>
            </div>
            <div class="flex flex-col gap-2">
              <template v-for="(part, j) in msg.content" :key="j">
                <MarkdownText v-if="isTextPart(part)" :text="part.value" />
                <details
                  v-else-if="isThinkingPart(part)"
                  class="rounded-md border border-chart-4/30 bg-chart-4/5 px-3 py-2"
                >
                  <summary class="flex cursor-pointer items-center gap-1.5 text-xs text-chart-4">
                    <Brain class="h-3 w-3" /> Thinking
                  </summary>
                  <pre class="mt-2 text-xs whitespace-pre-wrap text-muted-foreground">{{
                    thinkingText(part.value)
                  }}</pre>
                </details>
                <div
                  v-else-if="isToolCallPart(part)"
                  class="rounded-md border border-border/60 bg-background px-3 py-2"
                >
                  <div class="mb-1 flex items-center gap-1.5 text-xs">
                    <Wrench class="h-3 w-3 text-muted-foreground" />
                    <span class="font-mono font-semibold">{{ part.name }}</span>
                    <code class="text-xs break-all text-muted-foreground">{{ part.callId }}</code>
                  </div>
                  <pre class="overflow-x-auto font-mono text-xs text-muted-foreground">{{
                    JSON.stringify(part.input, null, 2)
                  }}</pre>
                </div>
                <div
                  v-else-if="isToolResultPart(part)"
                  class="rounded-md border border-border/60 bg-background px-3 py-2"
                >
                  <div class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                    <FileJson class="h-3 w-3" /> 工具结果
                    <code class="text-xs break-all">{{ part.callId }}</code>
                  </div>
                  <pre class="text-xs whitespace-pre-wrap">{{ toolResultText(part) }}</pre>
                </div>
                <div v-else-if="isDataPart(part)" class="text-xs text-muted-foreground">
                  [二进制数据 {{ part.mimeType }}，{{ part.data.length }} 字节]
                </div>
              </template>
            </div>
          </div>
        </CardContent>
      </Card>

      <!-- 响应 parts -->
      <Card v-if="responseParts" class="shrink-0 gap-3 py-4">
        <CardHeader class="px-4 pb-0">
          <CardTitle class="text-lg font-semibold">响应内容</CardTitle>
          <CardDescription class="text-xs">
            聚合后的 LMResponsePart 序列（{{ responseParts?.length ?? 0 }} 个 part）
          </CardDescription>
        </CardHeader>
        <CardContent class="flex flex-col gap-2 px-4 pt-1">
          <template v-for="(part, i) in responseParts ?? []" :key="i">
            <!-- Text -->
            <div
              v-if="isTextPart(part)"
              class="rounded-lg border border-border/60 bg-muted/20 px-4 py-3"
            >
              <MarkdownText :text="part.value" />
            </div>
            <!-- Thinking -->
            <div
              v-else-if="isThinkingPart(part)"
              class="rounded-lg border border-chart-4/30 bg-chart-4/5 px-4 py-3"
            >
              <button
                class="flex min-h-9 cursor-pointer items-center gap-1.5 text-xs font-medium text-chart-4"
                :aria-expanded="expandedThinking.has(i)"
                @click="toggleThinking(i)"
              >
                <Brain class="h-3.5 w-3.5" /> Thinking
                <span class="text-muted-foreground"
                  >（点击{{ expandedThinking.has(i) ? "折叠" : "展开" }}）</span
                >
              </button>
              <pre
                v-if="expandedThinking.has(i)"
                class="mt-2 text-xs whitespace-pre-wrap text-muted-foreground"
                >{{ thinkingText(part.value) }}</pre>
            </div>
            <!-- ToolCall -->
            <div
              v-else-if="isToolCallPart(part)"
              class="rounded-lg border border-chart-2/30 bg-chart-2/5 px-4 py-3"
            >
              <div class="mb-2 flex items-center gap-2">
                <Wrench class="h-3.5 w-3.5 text-chart-2" />
                <span class="font-mono text-xs font-semibold">{{ part.name }}</span>
                <code class="text-xs break-all text-muted-foreground">{{ part.callId }}</code>
                <Button
                  variant="ghost"
                  size="icon"
                  class="ml-auto cursor-pointer"
                  aria-label="复制工具调用参数"
                  title="复制工具调用参数"
                  @click="copyText(`tc-${i}`, JSON.stringify(part.input, null, 2))"
                >
                  <Check v-if="copiedField === `tc-${i}`" class="h-3 w-3 text-primary" />
                  <Copy v-else class="h-3 w-3" />
                </Button>
              </div>
              <pre class="overflow-x-auto rounded-md bg-background px-3 py-2 font-mono text-xs">{{
                JSON.stringify(part.input, null, 2)
              }}</pre>
            </div>
            <!-- ToolResult -->
            <div
              v-else-if="isToolResultPart(part)"
              class="rounded-lg border border-border/60 bg-muted/20 px-4 py-3"
            >
              <div class="mb-1 flex items-center gap-1.5 text-xs text-muted-foreground">
                <FileJson class="h-3.5 w-3.5" /> 工具结果
                <code class="text-xs break-all">{{ part.callId }}</code>
              </div>
              <pre class="text-xs whitespace-pre-wrap">{{ toolResultText(part) }}</pre>
            </div>
            <!-- Usage -->
            <div
              v-else-if="isUsagePart(part)"
              class="flex flex-wrap items-center gap-x-4 gap-y-1 rounded-lg border border-primary/30 bg-primary/5 px-4 py-2.5"
            >
              <span class="flex items-center gap-1.5 text-xs font-medium text-primary">
                <Info class="h-3.5 w-3.5" /> Usage
              </span>
              <span v-if="part.inputTokens != null" class="font-mono text-xs">
                输入 {{ formatTokens(part.inputTokens) }}
              </span>
              <span v-if="part.outputTokens != null" class="font-mono text-xs">
                输出 {{ formatTokens(part.outputTokens) }}
              </span>
              <span v-if="part.reasoningTokens != null" class="font-mono text-xs">
                推理 {{ formatTokens(part.reasoningTokens) }}
              </span>
              <span v-if="part.finishReason" class="font-mono text-xs text-muted-foreground">
                finish: {{ part.finishReason }}
              </span>
            </div>
            <!-- Data -->
            <div
              v-else-if="isDataPart(part)"
              class="rounded-lg border border-border/60 bg-muted/20 px-4 py-3 text-xs text-muted-foreground"
            >
              [二进制数据 {{ part.mimeType }}，{{ part.data.length }} 字节]
            </div>
          </template>
        </CardContent>
      </Card>

      <!-- 无快照提示 -->
      <Alert
        v-if="requestMessages === null || responseParts === null"
        class="shrink-0 border-border bg-muted/20"
      >
        <AlertDescription class="text-sm text-muted-foreground">
          {{ requestMessages === null ? "请求内容" : ""
          }}{{ requestMessages === null && responseParts === null ? "与" : ""
          }}{{ responseParts === null ? "响应内容" : "" }}未采集或已不保留。 启用
          <code class="font-mono">LLM_BRIDGE_OBS_CAPTURE_CONTENT=true</code>
          只会影响之后的新请求，不恢复历史内容；采集包含敏感信息，需明确选择。
        </AlertDescription>
      </Alert>
    </div>
  </PageShell>

  <!-- 未找到 / 加载失败 -->
  <PageShell v-else>
    <ErrorState v-if="error" :error="error" inline @retry="load" />
    <EmptyState v-else :icon="ScrollText" title="未找到该请求记录" />
    <div class="flex justify-center">
      <Button variant="outline" class="cursor-pointer" @click="router.push('/traces')">
        <ArrowLeft class="mr-1 h-4 w-4" /> 返回请求追踪
      </Button>
    </div>
  </PageShell>
</template>
