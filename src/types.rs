use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};
use ts_rs::TS;

/// 消息角色，对应 LanguageModelChatMessageRole 枚举（User = 1, Assistant = 2, System = 3, Developer = 4）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr, TS)]
#[ts(export)]
#[repr(u8)]
pub enum LanguageModelChatMessageRole {
    User = 1,
    Assistant = 2,
    System = 3,
    Developer = 4,
}

/// 纯文本消息部分，对应 LanguageModelTextPart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageModelTextPart {
    pub value: String,
}

/// Thinking 内容的值类型，可以是单个字符串或字符串数组
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LanguageModelThinkingValue {
    String(String),
    Array(Vec<String>),
}

/// 思考/推理内容部分，对应 LanguageModelThinkingPart
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageModelThinkingPart {
    pub value: LanguageModelThinkingValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// Prompt TSX 消息部分，对应 LanguageModelPromptTsxPart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageModelPromptTsxPart {
    pub value: Value,
}

/// 协议无关的工具（函数）定义，对应 OpenAI `tools[].function` 的核心字段。
/// 各上游适配器负责将其序列化为各自的协议格式。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageModelTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema 参数定义，对应 OpenAI 的 `parameters` / Anthropic 的 `input_schema`
    #[serde(default)]
    pub input_schema: Value,
}

/// 协议无关的推理配置：effort（OpenAI）与 max_tokens（Anthropic budget_tokens）至少其一非空。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageModelReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// 协议无关的结构化输出配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LanguageModelResponseFormat {
    JsonObject,
    JsonSchema { json_schema: Value },
}

/// 协议无关的 token 用量统计，由适配器在上游响应末尾发出。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageModelUsagePart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
    /// 上游真实的 finish_reason（stop / length / tool_calls / content_filter / end_turn / max_tokens …）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// 二进制数据消息部分，对应 LanguageModelDataPart
/// `data` 对应 Uint8Array，序列化为数字数组
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageModelDataPart {
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// 工具调用部分，对应 LanguageModelToolCallPart
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageModelToolCallPart {
    pub call_id: String,
    pub name: String,
    /// 工具调用输入，对应 TypeScript 的 `object`
    pub input: Value,
}

/// 工具结果内容的各种可能类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LanguageModelToolResultContent {
    Text(LanguageModelTextPart),
    PromptTsx(LanguageModelPromptTsxPart),
    Data(LanguageModelDataPart),
    /// 对应 TypeScript 中的 `unknown`
    Unknown(Value),
}

/// 工具结果部分，对应 LanguageModelToolResultPart
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageModelToolResultPart {
    pub call_id: String,
    pub content: Vec<LanguageModelToolResultContent>,
}

/// 消息输入部分的联合类型，对应 LanguageModelInputPart:
/// LanguageModelTextPart | LanguageModelToolResultPart | LanguageModelToolCallPart | LanguageModelDataPart | LanguageModelThinkingPart
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LanguageModelInputPart {
    Text(LanguageModelTextPart),
    ToolResult(LanguageModelToolResultPart),
    ToolCall(LanguageModelToolCallPart),
    Data(LanguageModelDataPart),
    Thinking(LanguageModelThinkingPart),
}

/// 语言模型响应部分的联合类型，对应 LMResponsePart:
/// LanguageModelTextPart | LanguageModelToolCallPart | LanguageModelDataPart | LanguageModelThinkingPart | LanguageModelToolResultPart | LanguageModelUsagePart
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LMResponsePart {
    Text(LanguageModelTextPart),
    ToolCall(LanguageModelToolCallPart),
    Data(LanguageModelDataPart),
    Thinking(LanguageModelThinkingPart),
    ToolResult(LanguageModelToolResultPart),
    Usage(LanguageModelUsagePart),
}

/// 聊天消息，对应 LanguageModelChatMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageModelChatMessage {
    pub role: LanguageModelChatMessageRole,
    pub content: Vec<LanguageModelInputPart>,
    pub name: Option<String>,
}

impl LanguageModelChatMessage {
    /// 创建用户消息，对应 LanguageModelChatMessage.User(...)
    pub fn user(content: Vec<LanguageModelInputPart>, name: Option<String>) -> Self {
        Self {
            role: LanguageModelChatMessageRole::User,
            content,
            name,
        }
    }

    /// 创建助手消息，对应 LanguageModelChatMessage.Assistant(...)
    pub fn assistant(content: Vec<LanguageModelInputPart>, name: Option<String>) -> Self {
        Self {
            role: LanguageModelChatMessageRole::Assistant,
            content,
            name,
        }
    }
}

/// 编辑工具名称集合，序列化为字符串数组
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, TS)]
#[ts(export)]
pub struct EndpointEditToolName {
    pub find_replace: bool,
    pub multi_find_replace: bool,
    pub apply_patch: bool,
    pub code_rewrite: bool,
}

impl EndpointEditToolName {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        !self.find_replace && !self.multi_find_replace && !self.apply_patch && !self.code_rewrite
    }
}

impl Serialize for EndpointEditToolName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut values = Vec::new();
        if self.find_replace {
            values.push("find-replace");
        }
        if self.multi_find_replace {
            values.push("multi-find-replace");
        }
        if self.apply_patch {
            values.push("apply-patch");
        }
        if self.code_rewrite {
            values.push("code-rewrite");
        }
        values.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EndpointEditToolName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<String>::deserialize(deserializer)?;
        let mut result = Self::empty();

        for value in values {
            match value.as_str() {
                "find-replace" => result.find_replace = true,
                "multi-find-replace" => result.multi_find_replace = true,
                "apply-patch" => result.apply_patch = true,
                "code-rewrite" => result.code_rewrite = true,
                other => {
                    return Err(serde::de::Error::custom(format!(
                        "unknown EndpointEditToolName: {other}"
                    )));
                }
            }
        }

        Ok(result)
    }
}

/// BYOK 模型能力，对应 BYOKModelCapabilities
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LMModelInfo {
    pub name: String,
    pub max_input_tokens: u32,
    pub max_output_tokens: u32,
    pub tool_calling: bool,
    pub vision: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adaptive_thinking: Option<bool>,
    #[serde(default = "EndpointEditToolName::empty")]
    #[serde(skip_serializing_if = "EndpointEditToolName::is_empty")]
    pub edit_tools: EndpointEditToolName,
}
