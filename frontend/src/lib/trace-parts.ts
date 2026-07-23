// ─────────────────────────────────────────────────────────────────────────────
// 内容快照渲染守卫（trace 详情页用）
//
// `LMResponsePart` / `LanguageModelInputPart` 为 untagged union（src/types.rs 的
// serde untagged），生成绑定无判别字段，需按字段存在性做类型守卫以窄化渲染。
// 类型直接复用 ts-rs 生成的 @bindings，保证与后端序列化形态严格一致。
// ─────────────────────────────────────────────────────────────────────────────

import { type LanguageModelChatMessage } from "@bindings/LanguageModelChatMessage";
import { type LanguageModelChatMessageRole } from "@bindings/LanguageModelChatMessageRole";
import { type LanguageModelDataPart } from "@bindings/LanguageModelDataPart";
import { type LanguageModelTextPart } from "@bindings/LanguageModelTextPart";
import { type LanguageModelThinkingPart } from "@bindings/LanguageModelThinkingPart";
import { type LanguageModelToolCallPart } from "@bindings/LanguageModelToolCallPart";
import { type LanguageModelToolResultPart } from "@bindings/LanguageModelToolResultPart";
import { type LanguageModelUsagePart } from "@bindings/LanguageModelUsagePart";
import { type LMResponsePart } from "@bindings/LMResponsePart";

export type {
  LMResponsePart,
  LanguageModelChatMessage,
  LanguageModelChatMessageRole,
  LanguageModelDataPart,
  LanguageModelTextPart,
  LanguageModelThinkingPart,
  LanguageModelToolCallPart,
  LanguageModelToolResultPart,
  LanguageModelUsagePart,
};

/** `LanguageModelChatMessageRole`：后端 JSON 序列化为小写字符串字面量，
 * 与 ts-rs 生成的联合类型严格一致（lowercase）。 */
export const ROLE_USER = "user";
export const ROLE_ASSISTANT = "assistant";
export const ROLE_SYSTEM = "system";
export const ROLE_DEVELOPER = "developer";

// ── Part 类型守卫（untagged union 判别）──

export function isTextPart(p: unknown): p is LanguageModelTextPart {
  return (
    typeof p === "object" &&
    p != null &&
    typeof (p as LanguageModelTextPart).value === "string" &&
    !("callId" in p) &&
    !("mimeType" in p)
  );
}

export function isThinkingPart(p: unknown): p is LanguageModelThinkingPart {
  if (typeof p !== "object" || p == null || "callId" in p || "mimeType" in p) return false;
  const v = (p as LanguageModelThinkingPart).value;
  return typeof v === "string" || (Array.isArray(v) && v.every((x) => typeof x === "string"));
}

export function isToolCallPart(p: unknown): p is LanguageModelToolCallPart {
  return (
    typeof p === "object" &&
    p != null &&
    typeof (p as LanguageModelToolCallPart).callId === "string" &&
    "input" in p
  );
}

export function isToolResultPart(p: unknown): p is LanguageModelToolResultPart {
  return (
    typeof p === "object" &&
    p != null &&
    typeof (p as LanguageModelToolResultPart).callId === "string" &&
    Array.isArray((p as LanguageModelToolResultPart).content)
  );
}

export function isDataPart(p: unknown): p is LanguageModelDataPart {
  return (
    typeof p === "object" &&
    p != null &&
    typeof (p as LanguageModelDataPart).mimeType === "string" &&
    Array.isArray((p as LanguageModelDataPart).data)
  );
}

export function isUsagePart(p: unknown): p is LanguageModelUsagePart {
  return (
    typeof p === "object" &&
    p != null &&
    ("inputTokens" in p || "outputTokens" in p || "finishReason" in p) &&
    !("value" in p) &&
    !("callId" in p)
  );
}
