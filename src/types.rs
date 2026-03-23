use bitflags::bitflags;
use bincode_next::{Decode, Encode};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use serde_repr::{Deserialize_repr, Serialize_repr};

/// 消息角色，对应 LanguageModelChatMessageRole 枚举（User = 1, Assistant = 2）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum LanguageModelChatMessageRole {
    User = 1,
    Assistant = 2,
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
/// LanguageModelTextPart | LanguageModelToolCallPart | LanguageModelDataPart | LanguageModelThinkingPart | LanguageModelToolResultPart
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LMResponsePart {
    Text(LanguageModelTextPart),
    ToolCall(LanguageModelToolCallPart),
    Data(LanguageModelDataPart),
    Thinking(LanguageModelThinkingPart),
    ToolResult(LanguageModelToolResultPart),
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

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct EndpointEditToolName: u8 {
        const FIND_REPLACE = 1 << 0;
        const MULTI_FIND_REPLACE = 1 << 1;
        const APPLY_PATCH = 1 << 2;
        const CODE_REWRITE = 1 << 3;
    }
}

impl Serialize for EndpointEditToolName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut values = Vec::new();
        if self.contains(Self::FIND_REPLACE) {
            values.push("find-replace");
        }
        if self.contains(Self::MULTI_FIND_REPLACE) {
            values.push("multi-find-replace");
        }
        if self.contains(Self::APPLY_PATCH) {
            values.push("apply-patch");
        }
        if self.contains(Self::CODE_REWRITE) {
            values.push("code-rewrite");
        }
        values.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EndpointEditToolName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<String>::deserialize(deserializer)?;
        let mut flags = Self::empty();

        for value in values {
            match value.as_str() {
                "find-replace" => flags |= Self::FIND_REPLACE,
                "multi-find-replace" => flags |= Self::MULTI_FIND_REPLACE,
                "apply-patch" => flags |= Self::APPLY_PATCH,
                "code-rewrite" => flags |= Self::CODE_REWRITE,
                other => {
                    return Err(serde::de::Error::custom(format!(
                        "unknown EndpointEditToolName: {other}"
                    )));
                }
            }
        }

        Ok(flags)
    }
}

/// BYOK 模型能力，对应 BYOKModelCapabilities
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
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
    #[bincode(with_serde)]
    pub edit_tools: EndpointEditToolName,
}
