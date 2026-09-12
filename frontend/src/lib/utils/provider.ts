// Provider 相关共享工具 — 从 ProvidersPage 抽出，供创建对话框与列表展开共用。
// 见 PLAN.md §10 Phase B B.3。

import { type CreateProviderRequest } from "@bindings/CreateProviderRequest";
import { type ProtocolInput } from "@bindings/ProtocolInput";
import { type ProviderResponse } from "@bindings/ProviderResponse";

export function providerToDraft(provider: ProviderResponse | null): CreateProviderRequest {
  if (!provider)
    return {
      providerId: "",
      displayName: "",
      apiKeys: [{ label: "key-1", key: "", weight: 1 }],
      protocols: [emptyProtocol()],
      enabled: true,
      priority: 100,
      quotaAdapter: null,
      quotaAdapterConfig: null,
    };
  return {
    providerId: provider.providerId,
    displayName: provider.displayName,
    apiKeys: provider.apiKeys.map((key) => ({ label: key.label, key: "", weight: key.weight })),
    protocols: provider.protocols.map(protocolViewToInput),
    enabled: provider.enabled,
    priority: provider.priority,
    quotaAdapter: provider.quotaAdapter,
    quotaAdapterConfig: provider.quotaAdapterConfig,
  };
}

export function validateProviderDraft(
  input: CreateProviderRequest,
  existing: ProviderResponse | null,
): Record<string, string> {
  const errors: Record<string, string> = {};
  const uint = (value: number, min = 0) =>
    Number.isInteger(value) && value >= min && value <= 4294967295;
  if (!input.providerId.trim()) errors.providerId = "实例 ID 必填";
  if (existing && input.providerId !== existing.providerId)
    errors.providerId = "已保存的实例 ID 不可修改";
  if (!uint(input.priority)) errors.priority = "提供者优先级须为 0–4294967295 整数";
  const labels = new Set<string>();
  input.apiKeys.forEach((key, index) => {
    const prefix = `apiKeys.${index}`;
    if (!key.label.trim() || key.label !== key.label.trim() || labels.has(key.label))
      errors[`${prefix}.label`] = "label 必填、不能重复或包含首尾空白";
    labels.add(key.label);
    if (!key.key.trim() && !existing?.apiKeys.some((old) => old.label === key.label))
      errors[`${prefix}.key`] = "新增 Key 必填；稍后配置请移除该条目";
    if (!uint(key.weight, 1)) errors[`${prefix}.weight`] = "权重须为 1–4294967295 整数";
  });
  input.protocols.forEach((protocol, index) => {
    const prefix = `protocols.${index}`;
    try {
      if (!["http:", "https:"].includes(new URL(protocol.baseUrl).protocol)) throw new Error();
    } catch {
      errors[`${prefix}.baseUrl`] = "请输入合法 HTTP(S) Base URL";
    }
    if (!uint(protocol.priority)) errors[`${prefix}.priority`] = "协议优先级须为 0–4294967295 整数";
    if (protocol.compatSettings?.trim()) {
      try {
        JSON.parse(protocol.compatSettings);
      } catch {
        errors[`${prefix}.compatSettings`] = "兼容配置必须是合法 JSON";
      }
    }
  });
  if (input.quotaAdapterConfig?.trim()) {
    try {
      JSON.parse(input.quotaAdapterConfig);
    } catch {
      errors.quotaAdapterConfig = "额度配置必须是合法 JSON";
    }
  }
  return errors;
}

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
export function buildQuotaConfigString(baseUrl: string, keyLabelFilter: string): string | null {
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
