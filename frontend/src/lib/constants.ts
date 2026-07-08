// 全站枚举与常量集中化 — 见 PLAN.md §10 Phase B B.2（解决 B7）。
// 删除散落在各页面的 hardcode 中文映射，统一从此处导入。

import { type ProviderCompatibility } from "@bindings/ProviderCompatibility";
import { type ProviderQuotaAdapter } from "@bindings/ProviderQuotaAdapter";

// ── 协议（ProviderCompatibility） ──

export const PROTOCOL_OPTIONS: ReadonlyArray<{
  value: ProviderCompatibility;
  label: string;
}> = [
  { value: "openAiChatCompletions", label: "OpenAI Chat Completions" },
  { value: "openAiResponses", label: "OpenAI Responses" },
  { value: "anthropicMessages", label: "Anthropic Messages" },
];

const PROTOCOL_LABELS: Record<ProviderCompatibility, string> = {
  openAiChatCompletions: "OpenAI Chat Completions",
  openAiResponses: "OpenAI Responses",
  anthropicMessages: "Anthropic Messages",
};

export function protocolLabel(p: ProviderCompatibility): string {
  return PROTOCOL_LABELS[p] ?? p;
}

// ── 配额周期（QuotaPeriod） ──

export type QuotaPeriod = "daily" | "monthly" | "unlimited";

export const QUOTA_PERIOD_OPTIONS: ReadonlyArray<{
  value: QuotaPeriod;
  label: string;
}> = [
  { value: "daily", label: "每天" },
  { value: "monthly", label: "每月" },
  { value: "unlimited", label: "不限制" },
];

const QUOTA_PERIOD_LABELS: Record<string, string> = {
  daily: "每天",
  monthly: "每月",
  unlimited: "不限制",
};

export function quotaPeriodLabel(period: string): string {
  return QUOTA_PERIOD_LABELS[period] ?? period;
}

// ── 额度适配器（ProviderQuotaAdapter） ──
// select value 用字符串，`"none"` 哨兵表示不配置（对应 null）。

export const QUOTA_ADAPTER_NONE = "none" as const;

export const QUOTA_ADAPTER_OPTIONS: ReadonlyArray<{
  value: string;
  label: string;
}> = [
  { value: QUOTA_ADAPTER_NONE, label: "不查询上游额度" },
  { value: "umans", label: "Umans" },
];

const QUOTA_ADAPTER_LABELS: Record<string, string> = {
  umans: "Umans",
};

export function quotaAdapterLabel(a: ProviderQuotaAdapter | null | undefined): string {
  if (!a) return "未配置";
  return QUOTA_ADAPTER_LABELS[a] ?? a;
}

/** 适配器下拉字符串值 → ProviderQuotaAdapter | null */
export function quotaAdapterFromSelect(v: string): ProviderQuotaAdapter | null {
  if (v === QUOTA_ADAPTER_NONE) return null;
  return v as ProviderQuotaAdapter;
}

/** ProviderQuotaAdapter | null → 适配器下拉字符串值 */
export function quotaAdapterToSelect(a: ProviderQuotaAdapter | null): string {
  return a ?? QUOTA_ADAPTER_NONE;
}

// ── 模型状态（status） ──

export const MODEL_STATUS_OPTIONS: ReadonlyArray<{ value: string; label: string }> = [
  { value: "stable", label: "稳定" },
  { value: "beta", label: "测试" },
  { value: "deprecated", label: "已废弃" },
];

const MODEL_STATUS_LABELS: Record<string, string> = {
  stable: "稳定",
  beta: "测试",
  deprecated: "已废弃",
};

export function modelStatusLabel(status: string | null | undefined): string {
  if (!status) return "";
  return MODEL_STATUS_LABELS[status] ?? status;
}

// ── Skeleton 占位行数（解决 B8 magic number） ──
// 各列表页 loading 时渲染的骨架屏行数，集中管理避免散落 Array(N)。

export const SKELETON_ROWS = {
  models: 6, // ModelsPage 表格
  tokens: 3, // TokensPage 卡片
  providers: 4, // ProvidersPage 列表
  adminModels: 4, // AdminModelsPage 列表
  users: 4, // UsersPage 列表
} as const;
