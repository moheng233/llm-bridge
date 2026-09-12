import { type AddModelProviderRequest } from "@bindings/AddModelProviderRequest";
import { type ModelLinkView } from "@bindings/ModelLinkView";
import { type ProviderResponse } from "@bindings/ProviderResponse";

import { parseTokens } from "~/lib/api";
export interface ConnectionDraft {
  providerId: number | null;
  protocolId: number | null;
  providerModelId: string;
  displayName: string;
  maxInput: string;
  maxOutput: string;
  inputPrice: string;
  outputPrice: string;
  cachePrice: string;
  priority: string;
  toolCalling: boolean | null;
  vision: boolean | null;
  thinking: boolean | null;
  adaptiveThinking: boolean | null;
  enabled: boolean;
}
export function connectionToDraft(link: ModelLinkView): ConnectionDraft {
  return {
    providerId: link.providerId,
    protocolId: link.protocolId,
    providerModelId: link.providerModelId,
    displayName: link.displayName,
    maxInput: link.maxInputTokens == null ? "" : String(link.maxInputTokens),
    maxOutput: link.maxOutputTokens == null ? "" : String(link.maxOutputTokens),
    inputPrice: link.inputPricePer1m == null ? "" : String(link.inputPricePer1m),
    outputPrice: link.outputPricePer1m == null ? "" : String(link.outputPricePer1m),
    cachePrice: link.cacheReadPricePer1m == null ? "" : String(link.cacheReadPricePer1m),
    priority: String(link.priority),
    toolCalling: link.toolCalling,
    vision: link.vision,
    thinking: link.thinking,
    adaptiveThinking: link.adaptiveThinking,
    enabled: link.enabled,
  };
}
export function validateConnectionDraft(
  draft: ConnectionDraft,
  provider: ProviderResponse,
): Record<string, string> {
  const errors: Record<string, string> = {};
  if (draft.providerId !== provider.id) errors.providerId = "提供者上下文已改变，请重新选择";
  if (!provider.protocols.some((protocol) => protocol.id === draft.protocolId))
    errors.protocolId = "请选择当前提供者的协议";
  if (!draft.providerModelId.trim()) errors.providerModelId = "上游模型 ID 必填";
  for (const field of ["maxInput", "maxOutput"] as const) {
    if (!draft[field].trim()) continue;
    const value = parseTokens(draft[field]);
    if (value === null || value < 1 || value > 4294967295 || !Number.isInteger(value))
      errors[field] = "须为 1–4294967295；留空继承";
  }
  for (const field of ["inputPrice", "outputPrice", "cachePrice"] as const) {
    if (draft[field].trim() && (!Number.isFinite(Number(draft[field])) || Number(draft[field]) < 0))
      errors[field] = "价格须为有限非负数；留空未知，0 免费";
  }
  const priority = Number(draft.priority);
  if (
    !draft.priority.trim() ||
    !Number.isInteger(priority) ||
    priority < 0 ||
    priority > 4294967295
  )
    errors.priority = "优先级须为 0–4294967295 整数";
  return errors;
}
export function connectionDraftToInput(draft: ConnectionDraft): AddModelProviderRequest {
  return {
    providerId: draft.providerId!,
    protocolId: draft.protocolId!,
    providerModelId: draft.providerModelId.trim(),
    displayName: draft.displayName.trim(),
    maxInputTokens: draft.maxInput.trim() ? parseTokens(draft.maxInput) : null,
    maxOutputTokens: draft.maxOutput.trim() ? parseTokens(draft.maxOutput) : null,
    inputPricePer1m: draft.inputPrice.trim() ? Number(draft.inputPrice) : null,
    outputPricePer1m: draft.outputPrice.trim() ? Number(draft.outputPrice) : null,
    cacheReadPricePer1m: draft.cachePrice.trim() ? Number(draft.cachePrice) : null,
    priority: Number(draft.priority),
    toolCalling: draft.toolCalling,
    vision: draft.vision,
    thinking: draft.thinking,
    adaptiveThinking: draft.adaptiveThinking,
    enabled: draft.enabled,
  };
}
