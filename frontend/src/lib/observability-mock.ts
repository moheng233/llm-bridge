// ─────────────────────────────────────────────────────────────────────────────
// 内容快照渲染守卫（trace 详情页用）
//
// 对齐 src/types.rs 的 serde 序列化形态：`LanguageModelInputPart` / `LMResponsePart`
// 为 untagged union，TS 侧无法靠判别字段区分，需按字段存在性做类型守卫。
// 后端 API 的 requestMessages / responseParts 即此结构（详情页按 unknown 收窄后渲染）。
// ─────────────────────────────────────────────────────────────────────────────

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
