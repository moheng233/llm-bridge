// ─────────────────────────────────────────────────────────────────────────────
// 可观察性示例数据（O4/O5 后端 API 落地前的样式确认用 mock）
//
// 类型命名与 Rust 侧 `LlmRequestTrace` / `UsageDaily`（src/db/models.rs）对齐，
// 后端 API 就绪后此文件将被删除、由 bindings 类型 + API 调用替换。
// ─────────────────────────────────────────────────────────────────────────────

/** 追踪状态（对齐 Rust `TraceStatus` Embed 枚举序列化形态） */
export type TraceStatus = "Pending" | "Streaming" | "Success" | "Error" | "Cancelled";

/** 请求来源接口（对齐 Rust `TraceInterface`） */
export type TraceInterface = "OpenAiHttp" | "WsRpc";

// ── 内容快照类型（对齐 src/types.rs 的 serde 序列化形态）──

/** `LanguageModelChatMessageRole`：serde_repr u8 */
export const ROLE_USER = 1;
export const ROLE_ASSISTANT = 2;
export const ROLE_SYSTEM = 3;
export const ROLE_DEVELOPER = 4;
export type ChatMessageRole =
  | typeof ROLE_USER
  | typeof ROLE_ASSISTANT
  | typeof ROLE_SYSTEM
  | typeof ROLE_DEVELOPER;

export interface TextPart {
  value: string;
}

export interface ThinkingPart {
  value: string | string[];
  id?: string;
  metadata?: unknown;
}

export interface ToolCallPart {
  callId: string;
  name: string;
  input: Record<string, unknown>;
}

export interface DataPart {
  mimeType: string;
  /** 序列化为数字数组（Uint8Array） */
  data: number[];
}

export interface ToolResultPart {
  callId: string;
  content: Array<TextPart | DataPart | unknown>;
}

export interface UsagePart {
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
  reasoningTokens?: number;
  cachedTokens?: number;
  finishReason?: string;
}

/** `LanguageModelInputPart`（untagged union） */
export type InputPart = TextPart | ThinkingPart | ToolCallPart | ToolResultPart | DataPart;

/** `LMResponsePart`（untagged union） */
export type ResponsePart =
  | TextPart
  | ThinkingPart
  | ToolCallPart
  | ToolResultPart
  | DataPart
  | UsagePart;

export interface ChatMessage {
  role: ChatMessageRole;
  content: InputPart[];
  name?: string | null;
}

/** 请求追踪记录（对齐 `llm_request_traces` 表） */
export interface RequestTrace {
  id: number;
  requestId: string;
  traceId: string | null;
  interface: TraceInterface;
  tokenId: number;
  userId: number;
  tokenPrefix: string;
  model: string;
  providerId: string;
  providerModelId: string;
  protocol: string;
  status: TraceStatus;
  errorType: string | null;
  errorMessage: string | null;
  upstreamStatus: number | null;
  finishReason: string | null;
  estimatedTokens: number;
  inputTokens: number | null;
  outputTokens: number | null;
  reasoningTokens: number | null;
  cachedTokens: number | null;
  totalTokens: number | null;
  costUsd: number | null;
  upstreamRequestId: string | null;
  createdAt: number; // unix 秒
  firstChunkAt: number | null;
  completedAt: number | null;
  ttftMs: number | null;
  latencyMs: number | null;
  /** 内容快照（Opt-In，可能为 null） */
  requestMessages: ChatMessage[] | null;
  responseParts: ResponsePart[] | null;
}

/** 日度用量 rollup（对齐 `usage_daily` 表） */
export interface UsageDailyRow {
  day: string; // YYYY-MM-DD
  tokenId: number;
  model: string;
  requestCount: number;
  inputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  cachedTokens: number;
  totalTokens: number;
  costUsd: number;
}

// ── 示例数据生成 ──

const DAY = 86400;
/** 以「今天」为基准的演示时间锚点 */
const NOW = Math.floor(Date.now() / 1000);

function at(daysAgo: number, hour: number, minute = 0, second = 0): number {
  const d = new Date(NOW * 1000);
  d.setDate(d.getDate() - daysAgo);
  d.setHours(hour, minute, second, 0);
  return Math.floor(d.getTime() / 1000);
}

export const MOCK_MODELS = [
  "openai/gpt-4o",
  "openai/gpt-4o-mini",
  "anthropic/claude-sonnet-4",
  "anthropic/claude-haiku-4",
  "deepseek/deepseek-chat",
  "google/gemini-2.5-pro",
] as const;

export const MOCK_TOKENS = [
  { id: 1, name: "dev-laptop", prefix: "sk-a1b2..." },
  { id: 2, name: "ci-pipeline", prefix: "sk-c3d4..." },
  { id: 3, name: "vscode:9f2e71ab", prefix: "sk-e5f6..." },
] as const;

/** 14 天日度用量 rollup（仪表盘趋势图数据源） */
export const MOCK_USAGE_DAILY: UsageDailyRow[] = (() => {
  const rows: UsageDailyRow[] = [];
  // 手工调参的曲线，让图表有真实感（工作日高、周末低、有波动）
  const shape = [42, 55, 61, 58, 49, 22, 18, 47, 63, 71, 66, 52, 25, 20];
  for (let i = 13; i >= 0; i--) {
    const dayIdx = 13 - i;
    const d = new Date(NOW * 1000 - i * DAY * 1000);
    const day = d.toISOString().slice(0, 10);
    const base = shape[dayIdx];
    // 按模型拆分
    const splits: Array<[string, number, number, number]> = [
      // [model, 请求占比, 输入tok/req, 输出tok/req]
      ["openai/gpt-4o", 0.3, 1840, 620],
      ["anthropic/claude-sonnet-4", 0.24, 2350, 940],
      ["deepseek/deepseek-chat", 0.2, 1120, 480],
      ["openai/gpt-4o-mini", 0.14, 760, 310],
      ["anthropic/claude-haiku-4", 0.08, 540, 260],
      ["google/gemini-2.5-pro", 0.04, 3200, 1250],
    ];
    for (const [model, share, inPer, outPer] of splits) {
      const count = Math.max(1, Math.round(base * share));
      const input = count * inPer + Math.round(Math.random() * 500);
      const output = count * outPer + Math.round(Math.random() * 200);
      const reasoning = model.includes("gemini") ? Math.round(output * 0.4) : 0;
      const cached = model.includes("claude") ? Math.round(input * 0.15) : 0;
      rows.push({
        day,
        tokenId: 1 + (dayIdx % MOCK_TOKENS.length),
        model,
        requestCount: count,
        inputTokens: input,
        outputTokens: output,
        reasoningTokens: reasoning,
        cachedTokens: cached,
        totalTokens: input + output + reasoning,
        costUsd: (input * 2.5 + output * 10) / 1_000_000,
      });
    }
  }
  return rows;
})();

/** 构造一条 trace 的便捷工厂 */
function trace(
  partial: Partial<RequestTrace> & Pick<RequestTrace, "id" | "createdAt">,
): RequestTrace {
  return {
    requestId: crypto.randomUUID(),
    traceId: null,
    interface: "OpenAiHttp",
    tokenId: 1,
    userId: 1,
    tokenPrefix: "sk-a1b2...",
    model: "openai/gpt-4o",
    providerId: "openai-main",
    providerModelId: "gpt-4o-2026-05-13",
    protocol: "openai",
    status: "Success",
    errorType: null,
    errorMessage: null,
    upstreamStatus: null,
    finishReason: "stop",
    estimatedTokens: 2000,
    inputTokens: 1200,
    outputTokens: 350,
    reasoningTokens: null,
    cachedTokens: null,
    totalTokens: 1550,
    costUsd: 0.0065,
    upstreamRequestId: "chatcmpl-demo",
    firstChunkAt: partial.createdAt + 1,
    completedAt: partial.createdAt + 3,
    ttftMs: 820,
    latencyMs: 2640,
    requestMessages: null,
    responseParts: null,
    ...partial,
  };
}

// 一段真实感的多轮对话快照（详情页展示用）
const DEMO_MESSAGES: ChatMessage[] = [
  {
    role: ROLE_SYSTEM,
    content: [{ value: "You are a helpful coding assistant embedded in VS Code." }],
    name: null,
  },
  {
    role: ROLE_USER,
    content: [
      {
        value:
          "帮我看一下这个 Rust 函数为什么编译不过：\n\n```rust\nfn longest(a: &str, b: &str) -> &str {\n    if a.len() > b.len() { a } else { b }\n}\n```",
      },
    ],
    name: null,
  },
  {
    role: ROLE_ASSISTANT,
    content: [
      {
        value:
          "这个函数缺少生命周期标注。编译器无法判断返回的引用借用自 `a` 还是 `b`，需要显式声明两者与返回值共享同一生命周期：\n\n```rust\nfn longest<'a>(a: &'a str, b: &'a str) -> &'a str {\n    if a.len() > b.len() { a } else { b }\n}\n```",
      },
    ],
    name: null,
  },
  {
    role: ROLE_USER,
    content: [{ value: "如果两个参数生命周期不同呢？比如一个是临时 String？" }],
    name: null,
  },
];

const DEMO_RESPONSE_PARTS: ResponsePart[] = [
  {
    value: "问得好。当两个参数生命周期不同时，返回引用的有效期不能超过",
  },
  { value: "其中**较短**的那个。" },
  {
    callId: "call_9f3k2",
    name: "rust_analyzer_check",
    input: { code: "fn longest<'a>(a: &'a str, b: &'a str) -> &'a str", edition: "2024" },
  },
  {
    value:
      "\n\n如果 `b` 是临时 `String`，`longest(s, &tmp)` 的返回值不能逃逸出 `tmp` 的作用域——这正是借用检查器要拦下的悬垂引用。两种解法：\n\n1. 让调用方持有 `tmp`，保证它活得比返回值久；\n2. 返回 `String`（所有权）而不是 `&str`：\n\n```rust\nfn longest(a: &str, b: &str) -> String {\n    if a.len() > b.len() { a.to_owned() } else { b.to_owned() }\n}\n```",
  },
  {
    inputTokens: 418,
    outputTokens: 276,
    totalTokens: 694,
    reasoningTokens: 128,
    finishReason: "tool_calls",
  },
];

/** 请求追踪示例数据（列表页 + 详情页数据源） */
export const MOCK_TRACES: RequestTrace[] = [
  // ── 今天 ──
  trace({
    id: 1042,
    createdAt: at(0, 14, 32, 11),
    model: "anthropic/claude-sonnet-4",
    providerId: "anthropic-main",
    providerModelId: "claude-sonnet-4-20250514",
    protocol: "anthropic",
    tokenId: 3,
    tokenPrefix: "sk-e5f6...",
    inputTokens: 418,
    outputTokens: 276,
    reasoningTokens: 128,
    cachedTokens: 512,
    totalTokens: 822,
    costUsd: 0.0118,
    finishReason: "tool_calls",
    ttftMs: 640,
    latencyMs: 3120,
    requestMessages: DEMO_MESSAGES,
    responseParts: DEMO_RESPONSE_PARTS,
  }),
  trace({
    id: 1041,
    createdAt: at(0, 14, 28, 3),
    model: "openai/gpt-4o",
    status: "Error",
    errorType: "provider_error",
    errorMessage: "upstream returned 429: rate limit exceeded for org (requests per min)",
    upstreamStatus: 429,
    finishReason: null,
    inputTokens: null,
    outputTokens: null,
    totalTokens: null,
    costUsd: null,
    ttftMs: null,
    latencyMs: 412,
    firstChunkAt: null,
    completedAt: at(0, 14, 28, 4),
    requestMessages: DEMO_MESSAGES.slice(0, 2),
    responseParts: null,
  }),
  trace({
    id: 1040,
    createdAt: at(0, 13, 55, 44),
    model: "deepseek/deepseek-chat",
    providerId: "deepseek-cn",
    providerModelId: "deepseek-chat",
    protocol: "openai",
    tokenId: 2,
    tokenPrefix: "sk-c3d4...",
    inputTokens: 2310,
    outputTokens: 118,
    totalTokens: 2428,
    costUsd: 0.0012,
    ttftMs: 388,
    latencyMs: 1230,
  }),
  trace({
    id: 1039,
    createdAt: at(0, 11, 12, 58),
    model: "google/gemini-2.5-pro",
    providerId: "vertex-main",
    providerModelId: "gemini-2.5-pro-preview-06-05",
    protocol: "openai",
    interface: "WsRpc",
    inputTokens: 5120,
    outputTokens: 2048,
    reasoningTokens: 1024,
    totalTokens: 8192,
    costUsd: 0.032,
    finishReason: "length",
    ttftMs: 1520,
    latencyMs: 12480,
    requestMessages: DEMO_MESSAGES.slice(0, 2),
    responseParts: [
      { value: "（输出达到 max_tokens 上限被截断…）" },
      { inputTokens: 5120, outputTokens: 2048, reasoningTokens: 1024, finishReason: "length" },
    ],
  }),
  trace({
    id: 1038,
    createdAt: at(0, 9, 41, 20),
    model: "openai/gpt-4o-mini",
    providerModelId: "gpt-4o-mini-2024-07-18",
    status: "Cancelled",
    finishReason: "cancelled",
    inputTokens: 890,
    outputTokens: 45,
    totalTokens: 935,
    costUsd: 0.0004,
    ttftMs: 290,
    latencyMs: 960,
  }),
  trace({
    id: 1037,
    createdAt: at(0, 9, 2, 8),
    model: "anthropic/claude-haiku-4",
    providerId: "anthropic-main",
    providerModelId: "claude-haiku-4-20250514",
    protocol: "anthropic",
    tokenId: 2,
    tokenPrefix: "sk-c3d4...",
    inputTokens: 640,
    outputTokens: 210,
    cachedTokens: 320,
    totalTokens: 850,
    costUsd: 0.0011,
    ttftMs: 310,
    latencyMs: 1450,
  }),
  // ── 昨天 ──
  trace({
    id: 1021,
    createdAt: at(1, 18, 47, 33),
    model: "anthropic/claude-sonnet-4",
    providerId: "anthropic-main",
    providerModelId: "claude-sonnet-4-20250514",
    protocol: "anthropic",
    tokenId: 3,
    tokenPrefix: "sk-e5f6...",
    inputTokens: 1890,
    outputTokens: 720,
    reasoningTokens: 256,
    totalTokens: 2866,
    costUsd: 0.0164,
    ttftMs: 720,
    latencyMs: 5430,
    requestMessages: DEMO_MESSAGES,
    responseParts: DEMO_RESPONSE_PARTS,
  }),
  trace({
    id: 1018,
    createdAt: at(1, 16, 20, 15),
    model: "openai/gpt-4o",
    inputTokens: 3240,
    outputTokens: 910,
    totalTokens: 4150,
    costUsd: 0.0172,
    ttftMs: 980,
    latencyMs: 6210,
  }),
  trace({
    id: 1012,
    createdAt: at(1, 10, 5, 41),
    model: "deepseek/deepseek-chat",
    providerId: "deepseek-cn",
    providerModelId: "deepseek-chat",
    tokenId: 1,
    status: "Error",
    errorType: "quota_exceeded",
    errorMessage: "token quota exceeded: 500000/500000 tokens used in current period",
    finishReason: null,
    inputTokens: null,
    outputTokens: null,
    totalTokens: null,
    costUsd: null,
    ttftMs: null,
    latencyMs: 8,
    firstChunkAt: null,
    completedAt: at(1, 10, 5, 41),
  }),
  trace({
    id: 1008,
    createdAt: at(1, 8, 30, 2),
    model: "openai/gpt-4o-mini",
    providerModelId: "gpt-4o-mini-2024-07-18",
    tokenId: 2,
    tokenPrefix: "sk-c3d4...",
    inputTokens: 420,
    outputTokens: 128,
    totalTokens: 548,
    costUsd: 0.0002,
    ttftMs: 245,
    latencyMs: 890,
  }),
  // ── 更早 ──
  trace({
    id: 990,
    createdAt: at(2, 21, 14, 50),
    model: "google/gemini-2.5-pro",
    providerId: "vertex-main",
    providerModelId: "gemini-2.5-pro-preview-06-05",
    tokenId: 3,
    tokenPrefix: "sk-e5f6...",
    interface: "WsRpc",
    inputTokens: 8900,
    outputTokens: 3200,
    reasoningTokens: 1600,
    totalTokens: 13700,
    costUsd: 0.054,
    ttftMs: 1890,
    latencyMs: 21300,
  }),
  trace({
    id: 985,
    createdAt: at(2, 15, 48, 26),
    model: "anthropic/claude-haiku-4",
    providerId: "anthropic-main",
    providerModelId: "claude-haiku-4-20250514",
    protocol: "anthropic",
    status: "Streaming",
    finishReason: null,
    inputTokens: 512,
    outputTokens: null,
    totalTokens: null,
    costUsd: null,
    ttftMs: 380,
    latencyMs: null,
    firstChunkAt: at(2, 15, 48, 27),
    completedAt: null,
  }),
  trace({
    id: 970,
    createdAt: at(3, 11, 22, 5),
    model: "openai/gpt-4o",
    inputTokens: 1560,
    outputTokens: 480,
    totalTokens: 2040,
    costUsd: 0.0087,
    ttftMs: 860,
    latencyMs: 3480,
  }),
  // ── 批量生成（滚动演示用，确定性伪随机） ──
  ...generateBulkTraces(60),
];

/** 确定性伪随机（LCG），保证每次加载数据一致 */
function lcg(seed: number): () => number {
  let s = seed;
  return () => {
    s = (s * 1103515245 + 12345) % 2147483648;
    return s / 2147483648;
  };
}

function generateBulkTraces(count: number): RequestTrace[] {
  const rand = lcg(42);
  const models: Array<[string, string, string, string]> = [
    // [model, providerId, providerModelId, protocol]
    ["openai/gpt-4o", "openai-main", "gpt-4o-2026-05-13", "openai"],
    ["openai/gpt-4o-mini", "openai-main", "gpt-4o-mini-2024-07-18", "openai"],
    ["anthropic/claude-sonnet-4", "anthropic-main", "claude-sonnet-4-20250514", "anthropic"],
    ["anthropic/claude-haiku-4", "anthropic-main", "claude-haiku-4-20250514", "anthropic"],
    ["deepseek/deepseek-chat", "deepseek-cn", "deepseek-chat", "openai"],
    ["google/gemini-2.5-pro", "vertex-main", "gemini-2.5-pro-preview-06-05", "openai"],
  ];
  const rows: RequestTrace[] = [];
  for (let i = 0; i < count; i++) {
    const [model, providerId, providerModelId, protocol] =
      models[Math.floor(rand() * models.length)];
    const tok = MOCK_TOKENS[Math.floor(rand() * MOCK_TOKENS.length)];
    const daysAgo = 3 + Math.floor(rand() * 10); // 3~12 天前
    const createdAt = at(
      daysAgo,
      Math.floor(rand() * 24),
      Math.floor(rand() * 60),
      Math.floor(rand() * 60),
    );
    const failed = rand() < 0.08;
    const cancelled = !failed && rand() < 0.05;
    const input = 300 + Math.floor(rand() * 4500);
    const output = failed ? null : 60 + Math.floor(rand() * 1800);
    const reasoning = model.includes("gemini") && output != null ? Math.round(output * 0.4) : null;
    const cached = model.includes("claude") ? Math.round(input * 0.15) : null;
    const total = failed ? null : input + (output ?? 0) + (reasoning ?? 0);
    const ttft = failed ? null : 200 + Math.floor(rand() * 1600);
    const latency = failed
      ? 100 + Math.floor(rand() * 500)
      : (ttft ?? 0) + 500 + Math.floor(rand() * 9000);
    rows.push(
      trace({
        id: 960 - i,
        createdAt,
        model,
        providerId,
        providerModelId,
        protocol,
        tokenId: tok.id,
        tokenPrefix: tok.prefix,
        interface: rand() < 0.15 ? "WsRpc" : "OpenAiHttp",
        status: failed ? "Error" : cancelled ? "Cancelled" : "Success",
        errorType: failed ? "provider_error" : null,
        errorMessage: failed ? "upstream returned 502: bad gateway" : null,
        upstreamStatus: failed ? 502 : null,
        finishReason: failed ? null : cancelled ? "cancelled" : rand() < 0.9 ? "stop" : "length",
        inputTokens: input,
        outputTokens: output,
        reasoningTokens: reasoning,
        cachedTokens: cached,
        totalTokens: total,
        costUsd: total != null ? ((input + (output ?? 0) * 4) / 1_000_000) * 2.5 : null,
        ttftMs: ttft,
        latencyMs: latency,
      }),
    );
  }
  return rows;
}

// ── 派生选择器（仪表盘用）──

export interface DashboardKpis {
  totalRequests: number;
  totalTokens: number;
  totalCostUsd: number;
  errorRate: number; // 0..1
  avgTtftMs: number;
  requestsDelta: number; // 环比（前一周期同长度）
}

/** 汇总 usage_daily 得到 KPI（range = 最近 N 天） */
export function summarizeUsage(rows: UsageDailyRow[], traces: RequestTrace[]): DashboardKpis {
  const totalRequests = rows.reduce((s, r) => s + r.requestCount, 0);
  const totalTokens = rows.reduce((s, r) => s + r.totalTokens, 0);
  const totalCostUsd = rows.reduce((s, r) => s + r.costUsd, 0);
  const finals = traces.filter((t) => t.status !== "Pending" && t.status !== "Streaming");
  const errors = finals.filter((t) => t.status === "Error").length;
  const ttfts = traces.map((t) => t.ttftMs).filter((v): v is number => v != null);
  return {
    totalRequests,
    totalTokens,
    totalCostUsd,
    errorRate: finals.length > 0 ? errors / finals.length : 0,
    avgTtftMs: ttfts.length > 0 ? Math.round(ttfts.reduce((a, b) => a + b, 0) / ttfts.length) : 0,
    requestsDelta: 0.12, // 演示用固定环比
  };
}

export interface ModelRanking {
  model: string;
  requests: number;
  totalTokens: number;
  costUsd: number;
}

export function rankModels(rows: UsageDailyRow[]): ModelRanking[] {
  const map = new Map<string, ModelRanking>();
  for (const r of rows) {
    const e = map.get(r.model) ?? { model: r.model, requests: 0, totalTokens: 0, costUsd: 0 };
    e.requests += r.requestCount;
    e.totalTokens += r.totalTokens;
    e.costUsd += r.costUsd;
    map.set(r.model, e);
  }
  return [...map.values()].sort((a, b) => b.totalTokens - a.totalTokens);
}

export interface DailyPoint {
  day: string;
  label: string; // MM-DD
  inputTokens: number;
  outputTokens: number;
  requests: number;
  costUsd: number;
}

export function aggregateByDay(rows: UsageDailyRow[]): DailyPoint[] {
  const map = new Map<string, DailyPoint>();
  for (const r of rows) {
    const e =
      map.get(r.day) ??
      ({
        day: r.day,
        label: r.day.slice(5),
        inputTokens: 0,
        outputTokens: 0,
        requests: 0,
        costUsd: 0,
      } satisfies DailyPoint);
    e.inputTokens += r.inputTokens;
    e.outputTokens += r.outputTokens + r.reasoningTokens;
    e.requests += r.requestCount;
    e.costUsd += r.costUsd;
    map.set(r.day, e);
  }
  return [...map.values()].sort((a, b) => a.day.localeCompare(b.day));
}

// ── Part 类型守卫（untagged union 判别）──

export function isTextPart(p: unknown): p is TextPart {
  return (
    typeof p === "object" &&
    p != null &&
    typeof (p as TextPart).value === "string" &&
    !("callId" in p) &&
    !("mimeType" in p)
  );
}

export function isThinkingPart(p: unknown): p is ThinkingPart {
  if (typeof p !== "object" || p == null || "callId" in p || "mimeType" in p) return false;
  const v = (p as ThinkingPart).value;
  return typeof v === "string" || (Array.isArray(v) && v.every((x) => typeof x === "string"));
}

export function isToolCallPart(p: unknown): p is ToolCallPart {
  return (
    typeof p === "object" &&
    p != null &&
    typeof (p as ToolCallPart).callId === "string" &&
    typeof (p as ToolCallPart).input === "object"
  );
}

export function isToolResultPart(p: unknown): p is ToolResultPart {
  return (
    typeof p === "object" &&
    p != null &&
    typeof (p as ToolResultPart).callId === "string" &&
    Array.isArray((p as ToolResultPart).content)
  );
}

export function isDataPart(p: unknown): p is DataPart {
  return (
    typeof p === "object" &&
    p != null &&
    typeof (p as DataPart).mimeType === "string" &&
    Array.isArray((p as DataPart).data)
  );
}

export function isUsagePart(p: unknown): p is UsagePart {
  return (
    typeof p === "object" &&
    p != null &&
    ("inputTokens" in p || "outputTokens" in p || "finishReason" in p) &&
    !("value" in p) &&
    !("callId" in p)
  );
}
