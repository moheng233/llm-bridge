// Provider 相关共享工具 — 从 ProvidersPage 抽出，供创建对话框与列表展开共用。
// 见 PLAN.md §10 Phase B B.3。

import type { ProtocolInput } from "$bindings/ProtocolInput";

/** 创建一个空的 ProtocolInput（用于新增） */
export function emptyProtocol(): ProtocolInput {
  return {
    protocol: "openAiChatCompletions",
    baseUrl: "",
    enabled: true,
    priority: 100,
  };
}

/**
 * 将适配器配置字段拼接为后端期望的 JSON 字符串。
 * 两个字段都为空时返回 null，表示该 Provider 不带适配器配置（使用内置默认值）。
 */
export function buildQuotaConfigString(
  baseUrl: string,
  keyLabelFilter: string,
): string | null {
  const cfg: Record<string, string> = {};
  if (baseUrl.trim()) cfg.baseUrl = baseUrl.trim();
  if (keyLabelFilter.trim()) cfg.keyLabelFilter = keyLabelFilter.trim();
  if (Object.keys(cfg).length === 0) return null;
  return JSON.stringify(cfg);
}

/**
 * 解析后端返回的 `quotaAdapterConfig` JSON 字符串回到字段对象。
 * 用于编辑对话框回填表单值。
 */
export function parseQuotaConfigString(s: string | null): {
  baseUrl: string;
  keyLabelFilter: string;
} | null {
  if (!s || !s.trim()) return null;
  try {
    const cfg = JSON.parse(s) as Record<string, string>;
    return {
      baseUrl: cfg.baseUrl ?? "",
      keyLabelFilter: cfg.keyLabelFilter ?? "",
    };
  } catch {
    return null;
  }
}

/**
 * 将 ProtocolView（后端返回）转为 ProtocolInput（请求体）。
 * 用于编辑现有协议时回填表单，或全量替换协议列表时转换格式。
 */
export function protocolViewToInput(proto: {
  id?: number;
  protocol: string;
  baseUrl: string;
  compatSettings?: string | null;
  enabled: boolean;
  priority: number;
}): ProtocolInput {
  return {
    id: proto.id,
    protocol: proto.protocol as ProtocolInput["protocol"],
    baseUrl: proto.baseUrl,
    compatSettings: proto.compatSettings ?? undefined,
    enabled: proto.enabled,
    priority: proto.priority,
  };
}
