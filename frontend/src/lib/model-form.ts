import { type ModelInput } from "@bindings/ModelInput";

import { parseTokens } from "~/lib/api";
export interface ModelDraft {
  modelName: string;
  displayName: string;
  description: string;
  maxInput: string;
  maxOutput: string;
  status: string;
  toolCalling: boolean;
  vision: boolean;
  thinking: boolean;
  adaptiveThinking: boolean;
}
export function modelToDraft(input: ModelInput): ModelDraft {
  return {
    modelName: input.modelName,
    displayName: input.displayName,
    description: input.description ?? "",
    maxInput: String(input.maxInputTokens),
    maxOutput: String(input.maxOutputTokens),
    status: input.status ?? "",
    toolCalling: input.toolCalling,
    vision: input.vision,
    thinking: input.thinking,
    adaptiveThinking: input.adaptiveThinking,
  };
}
export function validateModelDraft(draft: ModelDraft): Record<string, string> {
  const errors: Record<string, string> = {};
  if (!draft.modelName.trim()) errors.modelName = "网关模型 ID 必填";
  for (const field of ["maxInput", "maxOutput"] as const) {
    const value = parseTokens(draft[field]);
    if (value === null || value < 1 || value > 4294967295 || !Number.isInteger(value))
      errors[field] = "Token 上限须为 1–4294967295，手动创建必须确认填写";
  }
  return errors;
}
export function modelDraftToInput(draft: ModelDraft): ModelInput {
  return {
    modelName: draft.modelName.trim(),
    displayName: draft.displayName.trim(),
    description: draft.description || null,
    maxInputTokens: parseTokens(draft.maxInput)!,
    maxOutputTokens: parseTokens(draft.maxOutput)!,
    status: draft.status || null,
    toolCalling: draft.toolCalling,
    vision: draft.vision,
    thinking: draft.thinking,
    adaptiveThinking: draft.adaptiveThinking,
  };
}
