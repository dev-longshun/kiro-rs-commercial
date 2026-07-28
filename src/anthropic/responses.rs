//! OpenAI Responses API 兼容层（Codex / Codex CLI）
//!
//! 对外暴露 `POST /v1/responses`，将 OpenAI Responses 协议请求转换为现有
//! Anthropic Messages → Kiro 链路，再把上游事件流映射回 Responses SSE。
//!
//! 设计目标：让用户在 Codex 中填写 base_url + api_key 后，能直接以
//! `gpt-5.6-sol/terra/luna` 等模型调用本服务。

use std::collections::HashMap;
use std::convert::Infallible;

use axum::{
    Extension, Json as JsonExtractor,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Json, Response},
};
use bytes::Bytes;
use futures::{Stream, StreamExt, stream};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::kiro::model::events::Event;
use crate::kiro::model::requests::kiro::KiroRequest;
use crate::kiro::parser::decoder::EventStreamDecoder;
use crate::token;

use super::converter::{ConversionError, convert_request};
use super::middleware::{ApiKeyContext, AppState};
use super::types::{Message, MessagesRequest, OutputConfig, SystemMessage, Thinking, Tool};

/// OpenAI Responses 请求体（宽松解析，兼容 Codex 额外字段）
#[derive(Debug, Clone, Deserialize)]
pub struct ResponsesRequest {
    pub model: String,
    /// string 或 input item 数组
    #[serde(default)]
    pub input: Value,
    #[serde(default)]
    pub instructions: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<Value>>,
    #[serde(default)]
    pub tool_choice: Option<Value>,
    #[serde(default = "default_stream")]
    pub stream: bool,
    #[serde(default)]
    pub max_output_tokens: Option<i32>,
    #[serde(default)]
    pub max_tokens: Option<i32>,
    /// Codex 会传 reasoning: { effort, summary }
    #[serde(default)]
    pub reasoning: Option<Value>,
    /// 其余字段（store / include / text / parallel_tool_calls 等）忽略
    #[serde(flatten)]
    pub _extra: HashMap<String, Value>,
}

fn default_stream() -> bool {
    // Codex 默认 stream=true；未指定时按流式处理更安全
    true
}

/// 转换错误
#[derive(Debug)]
pub enum ResponsesConvertError {
    EmptyInput,
    InvalidInput(String),
}

impl std::fmt::Display for ResponsesConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "input is empty"),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
        }
    }
}

/// 将 OpenAI Responses 请求转换为内部 Anthropic MessagesRequest
pub fn convert_responses_to_messages(
    req: &ResponsesRequest,
) -> Result<MessagesRequest, ResponsesConvertError> {
    let mut system_parts: Vec<String> = Vec::new();
    if let Some(ref instructions) = req.instructions {
        let trimmed = instructions.trim();
        if !trimmed.is_empty() {
            system_parts.push(trimmed.to_string());
        }
    }

    let mut messages: Vec<Message> = Vec::new();

    match &req.input {
        Value::String(s) => {
            let text = s.trim();
            if !text.is_empty() {
                messages.push(Message {
                    role: "user".to_string(),
                    content: Value::String(text.to_string()),
                });
            }
        }
        Value::Array(items) => {
            convert_input_items(items, &mut messages, &mut system_parts)?;
        }
        Value::Null => {}
        other => {
            return Err(ResponsesConvertError::InvalidInput(format!(
                "input must be string or array, got {}",
                value_type_name(other)
            )));
        }
    }

    if messages.is_empty() {
        return Err(ResponsesConvertError::EmptyInput);
    }

    // 确保以 user 消息结尾（Kiro 要求）；若末尾是 assistant，追加占位 user
    if messages.last().is_some_and(|m| m.role != "user") {
        messages.push(Message {
            role: "user".to_string(),
            content: Value::String("Please continue.".to_string()),
        });
    }

    let max_tokens = req
        .max_output_tokens
        .or(req.max_tokens)
        .unwrap_or(32_000)
        .max(1);

    let tools = convert_tools(req.tools.as_ref());

    let thinking = req.reasoning.as_ref().and_then(|r| {
        // 有 reasoning 字段时开启 thinking（Codex high effort 场景）
        let effort = r
            .get("effort")
            .and_then(|v| v.as_str())
            .unwrap_or("medium");
        if effort == "none" || effort == "minimal" {
            None
        } else {
            Some(Thinking {
                thinking_type: "enabled".to_string(),
                budget_tokens: 20_000,
            })
        }
    });

    let system = if system_parts.is_empty() {
        None
    } else {
        Some(
            system_parts
                .into_iter()
                .map(|text| SystemMessage { text })
                .collect(),
        )
    };

    Ok(MessagesRequest {
        model: req.model.clone(),
        max_tokens,
        messages,
        stream: req.stream,
        system,
        tools,
        tool_choice: req.tool_choice.clone(),
        thinking,
        output_config: None,
        metadata: None,
    })
}

fn value_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// 解析 Responses input item 数组
fn convert_input_items(
    items: &[Value],
    messages: &mut Vec<Message>,
    system_parts: &mut Vec<String>,
) -> Result<(), ResponsesConvertError> {
    // 累积同一 assistant 回合内的 tool_use，遇到下一条非 function_call 时 flush
    let mut pending_assistant_blocks: Vec<Value> = Vec::new();
    let mut pending_user_blocks: Vec<Value> = Vec::new();

    let flush_assistant = |blocks: &mut Vec<Value>, messages: &mut Vec<Message>| {
        if blocks.is_empty() {
            return;
        }
        messages.push(Message {
            role: "assistant".to_string(),
            content: Value::Array(std::mem::take(blocks)),
        });
    };
    let flush_user = |blocks: &mut Vec<Value>, messages: &mut Vec<Message>| {
        if blocks.is_empty() {
            return;
        }
        messages.push(Message {
            role: "user".to_string(),
            content: Value::Array(std::mem::take(blocks)),
        });
    };

    for item in items {
        // 纯字符串 item
        if let Some(s) = item.as_str() {
            flush_assistant(&mut pending_assistant_blocks, messages);
            let text = s.trim();
            if !text.is_empty() {
                pending_user_blocks.push(json!({"type": "text", "text": text}));
            }
            continue;
        }

        let obj = match item.as_object() {
            Some(o) => o,
            None => continue,
        };

        let item_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // 带 role 的 message 项（type 可能是 "message" 或省略）
        let role = obj.get("role").and_then(|v| v.as_str());

        if role == Some("system") || role == Some("developer") {
            flush_assistant(&mut pending_assistant_blocks, messages);
            flush_user(&mut pending_user_blocks, messages);
            let text = extract_text_from_content(obj.get("content"));
            if !text.is_empty() {
                system_parts.push(text);
            }
            continue;
        }

        if item_type == "function_call" || item_type == "custom_tool_call" {
            flush_user(&mut pending_user_blocks, messages);
            let call_id = obj
                .get("call_id")
                .or_else(|| obj.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let name = obj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let arguments = obj
                .get("arguments")
                .or_else(|| obj.get("input"))
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| "{}".to_string());
            let input_value: Value =
                serde_json::from_str(&arguments).unwrap_or_else(|_| json!({}));
            let id = if call_id.is_empty() {
                format!("call_{}", Uuid::new_v4().to_string().replace('-', ""))
            } else {
                call_id.to_string()
            };
            pending_assistant_blocks.push(json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": input_value
            }));
            continue;
        }

        if item_type == "function_call_output" || item_type == "custom_tool_call_output" {
            flush_assistant(&mut pending_assistant_blocks, messages);
            let call_id = obj
                .get("call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let output_text = extract_tool_output(obj.get("output"));
            pending_user_blocks.push(json!({
                "type": "tool_result",
                "tool_use_id": call_id,
                "content": output_text
            }));
            continue;
        }

        if item_type == "reasoning" {
            // reasoning 历史不回灌给上游（Kiro 不需要），跳过
            continue;
        }

        // message / 默认按 role 处理
        if let Some(role) = role {
            if role == "assistant" {
                flush_user(&mut pending_user_blocks, messages);
                let content = content_to_anthropic_blocks(obj.get("content"), "assistant");
                if !content.is_empty() {
                    // 若已有 pending tool_use，合并到同一 assistant 消息
                    if pending_assistant_blocks.is_empty() {
                        messages.push(Message {
                            role: "assistant".to_string(),
                            content: Value::Array(content),
                        });
                    } else {
                        let mut merged = content;
                        merged.append(&mut pending_assistant_blocks);
                        messages.push(Message {
                            role: "assistant".to_string(),
                            content: Value::Array(merged),
                        });
                    }
                } else if !pending_assistant_blocks.is_empty() {
                    flush_assistant(&mut pending_assistant_blocks, messages);
                }
                continue;
            }

            if role == "user" {
                flush_assistant(&mut pending_assistant_blocks, messages);
                let content = content_to_anthropic_blocks(obj.get("content"), "user");
                for block in content {
                    pending_user_blocks.push(block);
                }
                continue;
            }
        }

        // 无 type/role 的兜底：当作用户文本
        let mut text = extract_text_from_content(obj.get("content"));
        if text.is_empty() {
            text = obj
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }
        if !text.is_empty() {
            flush_assistant(&mut pending_assistant_blocks, messages);
            pending_user_blocks.push(json!({"type": "text", "text": text}));
        }
    }

    flush_assistant(&mut pending_assistant_blocks, messages);
    flush_user(&mut pending_user_blocks, messages);

    Ok(())
}

fn extract_text_from_content(content: Option<&Value>) -> String {
    match content {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => {
            let mut texts = Vec::new();
            for part in parts {
                if let Some(t) = part.as_str() {
                    texts.push(t.to_string());
                    continue;
                }
                let ptype = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if matches!(
                    ptype,
                    "input_text" | "output_text" | "text" | "input_text_delta"
                ) {
                    if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                        texts.push(t.to_string());
                    }
                }
            }
            texts.join("\n")
        }
        Some(Value::Object(obj)) => obj
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        Some(other) => other.to_string(),
    }
}

fn extract_tool_output(output: Option<&Value>) -> String {
    match output {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => {
            let mut texts = Vec::new();
            for part in parts {
                if let Some(t) = part.as_str() {
                    texts.push(t.to_string());
                    continue;
                }
                let ptype = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if matches!(ptype, "input_text" | "output_text" | "text") {
                    if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                        texts.push(t.to_string());
                    }
                }
            }
            if texts.is_empty() {
                serde_json::to_string(parts).unwrap_or_default()
            } else {
                texts.join("\n")
            }
        }
        Some(other) => other.to_string(),
    }
}

fn content_to_anthropic_blocks(content: Option<&Value>, _role: &str) -> Vec<Value> {
    match content {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(s)) => {
            if s.is_empty() {
                Vec::new()
            } else {
                vec![json!({"type": "text", "text": s})]
            }
        }
        Some(Value::Array(parts)) => {
            let mut blocks = Vec::new();
            for part in parts {
                if let Some(t) = part.as_str() {
                    if !t.is_empty() {
                        blocks.push(json!({"type": "text", "text": t}));
                    }
                    continue;
                }
                let ptype = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match ptype {
                    "input_text" | "output_text" | "text" => {
                        if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                            if !t.is_empty() {
                                blocks.push(json!({"type": "text", "text": t}));
                            }
                        }
                    }
                    "input_image" | "image_url" | "image" => {
                        // 尽量透传为 Anthropic image block
                        if let Some(image_url) = part
                            .get("image_url")
                            .or_else(|| part.get("url"))
                            .or_else(|| part.pointer("/source/url"))
                        {
                            let url = match image_url {
                                Value::String(s) => s.clone(),
                                Value::Object(o) => o
                                    .get("url")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                _ => String::new(),
                            };
                            if let Some((media_type, data)) = parse_data_url(&url) {
                                blocks.push(json!({
                                    "type": "image",
                                    "source": {
                                        "type": "base64",
                                        "media_type": media_type,
                                        "data": data
                                    }
                                }));
                            }
                        } else if let Some(data) = part.get("data").or_else(|| part.pointer("/source/data")) {
                            let media_type = part
                                .get("media_type")
                                .or_else(|| part.pointer("/source/media_type"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("image/png");
                            if let Some(d) = data.as_str() {
                                blocks.push(json!({
                                    "type": "image",
                                    "source": {
                                        "type": "base64",
                                        "media_type": media_type,
                                        "data": d
                                    }
                                }));
                            }
                        }
                    }
                    _ => {
                        // 未知块：若有 text 字段则保留
                        if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                            if !t.is_empty() {
                                blocks.push(json!({"type": "text", "text": t}));
                            }
                        }
                    }
                }
            }
            blocks
        }
        Some(other) => {
            let s = other.to_string();
            if s.is_empty() {
                Vec::new()
            } else {
                vec![json!({"type": "text", "text": s})]
            }
        }
    }
}

fn parse_data_url(url: &str) -> Option<(String, String)> {
    // data:image/png;base64,xxxx
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if !meta.contains("base64") {
        return None;
    }
    let media_type = meta
        .split(';')
        .next()
        .unwrap_or("image/png")
        .to_string();
    Some((media_type, data.to_string()))
}

fn convert_tools(tools: Option<&Vec<Value>>) -> Option<Vec<Tool>> {
    let tools = tools?;
    let mut out = Vec::new();

    for tool in tools {
        // 支持:
        // 1) { "type":"function", "name":..., "description":..., "parameters":{...} }
        // 2) { "type":"function", "function": { "name":..., "parameters":... } }  (chat 风格)
        // 3) namespace 包裹: { "type":"namespace", "tools":[ ... ] }
        let ttype = tool.get("type").and_then(|v| v.as_str()).unwrap_or("function");

        if ttype == "namespace" {
            if let Some(nested) = tool.get("tools").and_then(|v| v.as_array()) {
                if let Some(mut nested_tools) = convert_tools(Some(&nested.to_vec())) {
                    out.append(&mut nested_tools);
                }
            }
            continue;
        }

        // freeform / 其他特殊工具：尽量按 function 处理
        let (name, description, parameters) = if let Some(func) = tool.get("function") {
            (
                func.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                func.get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                func.get("parameters")
                    .or_else(|| func.get("input_schema"))
                    .cloned()
                    .unwrap_or_else(|| json!({"type":"object","properties":{}})),
            )
        } else {
            (
                tool.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                tool.get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                tool.get("parameters")
                    .or_else(|| tool.get("input_schema"))
                    .cloned()
                    .unwrap_or_else(|| json!({"type":"object","properties":{}})),
            )
        };

        if name.is_empty() {
            continue;
        }

        let input_schema: HashMap<String, Value> = match parameters {
            Value::Object(map) => map.into_iter().collect(),
            other => {
                let mut m = HashMap::new();
                m.insert("type".to_string(), json!("object"));
                m.insert("properties".to_string(), json!({}));
                m.insert("_raw".to_string(), other);
                m
            }
        };

        out.push(Tool {
            tool_type: None,
            name,
            description,
            input_schema,
            max_uses: None,
        });
    }

    if out.is_empty() { None } else { Some(out) }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Handler
// ═══════════════════════════════════════════════════════════════════════════════

/// POST /v1/responses
///
/// OpenAI Responses API 兼容入口，供 Codex / Codex CLI 使用。
pub async fn post_responses(
    State(state): State<AppState>,
    identity: Option<Extension<ApiKeyContext>>,
    JsonExtractor(payload): JsonExtractor<ResponsesRequest>,
) -> Response {
    tracing::info!(
        model = %payload.model,
        stream = %payload.stream,
        has_tools = payload.tools.as_ref().map(|t| !t.is_empty()).unwrap_or(false),
        input_is_array = payload.input.is_array(),
        "Received POST /v1/responses request"
    );

    if let Some(rpm_tracker) = &state.rpm_tracker {
        let api_key_id = identity.as_ref().map(|ext| ext.0.id);
        rpm_tracker.record_request(api_key_id);
    }

    let provider = match &state.kiro_provider {
        Some(p) => p.clone(),
        None => {
            return openai_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "server_error",
                "Kiro API provider not configured",
            );
        }
    };

    let mut messages_req = match convert_responses_to_messages(&payload) {
        Ok(req) => req,
        Err(e) => {
            tracing::warn!(error = %e, "Responses → Messages 转换失败");
            return openai_error(StatusCode::BAD_REQUEST, "invalid_request_error", e.to_string());
        }
    };

    // 与 /v1/messages 一致：thinking 模型名后缀
    override_thinking_from_model_name(&mut messages_req);

    let conversion_result = match convert_request(&messages_req) {
        Ok(r) => r,
        Err(e) => {
            let message = match &e {
                ConversionError::UnsupportedModel(model) => format!("Unsupported model: {model}"),
                ConversionError::EmptyMessages => "Empty messages".to_string(),
                ConversionError::InvalidImage(msg) => format!("Invalid image: {msg}"),
            };
            tracing::warn!(error = %e, "Kiro 请求转换失败");
            return openai_error(StatusCode::BAD_REQUEST, "invalid_request_error", message);
        }
    };

    let kiro_request = KiroRequest {
        conversation_state: conversion_result.conversation_state,
        profile_arn: None,
    };

    let request_body = match serde_json::to_string(&kiro_request) {
        Ok(body) => body.replace('\0', ""),
        Err(e) => {
            return openai_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                format!("serialize failed: {e}"),
            );
        }
    };

    let input_tokens = token::count_all_tokens(
        messages_req.model.clone(),
        messages_req.system.clone(),
        messages_req.messages.clone(),
        messages_req.tools.clone(),
    ) as i32;

    let api_key_id = identity.map(|ext| ext.0.id);
    let usage_tracker = state.usage_tracker.clone();
    let model = messages_req.model.clone();
    let stream = payload.stream;

    tracing::info!(
        model = %model,
        stream,
        estimated_input_tokens = input_tokens,
        request_bytes = request_body.len(),
        "Converted Responses request to Kiro request"
    );

    if stream {
        handle_responses_stream(
            provider,
            &request_body,
            &model,
            input_tokens,
            usage_tracker,
            api_key_id,
        )
        .await
    } else {
        handle_responses_non_stream(
            provider,
            &request_body,
            &model,
            input_tokens,
            usage_tracker,
            api_key_id,
        )
        .await
    }
}

/// 检测模型名是否包含 "thinking" 后缀，若包含则覆写 thinking 配置
///
/// 与 /v1/messages handlers 保持一致：
/// - Opus 4.6：覆写为 adaptive 类型，并设置 output_config.effort = high
/// - 其他模型：覆写为 enabled 类型
/// - budget_tokens 固定为 20000
fn override_thinking_from_model_name(payload: &mut MessagesRequest) {
    let model_lower = payload.model.to_lowercase();
    if !model_lower.contains("thinking") {
        return;
    }

    let is_opus_4_6 = model_lower.contains("opus")
        && (model_lower.contains("4-6") || model_lower.contains("4.6"));

    let thinking_type = if is_opus_4_6 { "adaptive" } else { "enabled" };

    tracing::info!(
        model = %payload.model,
        thinking_type = thinking_type,
        "模型名包含 thinking 后缀，覆写 thinking 配置"
    );

    payload.thinking = Some(Thinking {
        thinking_type: thinking_type.to_string(),
        budget_tokens: 20_000,
    });

    if is_opus_4_6 {
        payload.output_config = Some(OutputConfig {
            effort: "high".to_string(),
            format: None,
        });
    }
}

fn openai_error(
    status: StatusCode,
    error_type: impl Into<String>,
    message: impl Into<String>,
) -> Response {
    // OpenAI 风格错误 + 保留 anthropic ErrorResponse 结构的 message
    let body = json!({
        "error": {
            "message": message.into(),
            "type": error_type.into(),
            "param": null,
            "code": null
        }
    });
    (status, Json(body)).into_response()
}

fn openai_error_from_provider(err: anyhow::Error) -> Response {
    let err_str = err.to_string();
    let (status, code) = if err_str.contains("401") || err_str.to_lowercase().contains("unauthor") {
        (StatusCode::UNAUTHORIZED, "invalid_api_key")
    } else if err_str.contains("429") || err_str.to_lowercase().contains("rate") {
        (StatusCode::TOO_MANY_REQUESTS, "rate_limit_exceeded")
    } else if err_str.contains("400") || err_str.contains("Improperly formed") {
        (StatusCode::BAD_REQUEST, "invalid_request_error")
    } else if err_str.contains("CONTENT_LENGTH") || err_str.contains("too long") {
        (StatusCode::BAD_REQUEST, "context_length_exceeded")
    } else {
        (StatusCode::BAD_GATEWAY, "server_error")
    };
    openai_error(status, code, err_str)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Streaming
// ═══════════════════════════════════════════════════════════════════════════════

struct ResponsesStreamState {
    response_id: String,
    model: String,
    input_tokens: i32,
    output_tokens: i32,
    text_item_id: String,
    text_started: bool,
    text_buf: String,
    /// tool_use_id → (item_id, call_id, name, arguments_buf, started)
    tools: HashMap<String, ToolCallState>,
    sequence: i64,
    finished: bool,
    has_tool_use: bool,
    usage_tracker: Option<std::sync::Arc<crate::model::usage::UsageTracker>>,
    api_key_id: Option<u32>,
}

struct ToolCallState {
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
    added: bool,
}

impl ResponsesStreamState {
    fn new(
        model: impl Into<String>,
        input_tokens: i32,
        usage_tracker: Option<std::sync::Arc<crate::model::usage::UsageTracker>>,
        api_key_id: Option<u32>,
    ) -> Self {
        let response_id = format!("resp_{}", Uuid::new_v4().to_string().replace('-', ""));
        Self {
            response_id,
            model: model.into(),
            input_tokens,
            output_tokens: 0,
            text_item_id: format!("msg_{}", Uuid::new_v4().to_string().replace('-', "")),
            text_started: false,
            text_buf: String::new(),
            tools: HashMap::new(),
            sequence: 0,
            finished: false,
            has_tool_use: false,
            usage_tracker,
            api_key_id,
        }
    }

    fn next_seq(&mut self) -> i64 {
        let s = self.sequence;
        self.sequence += 1;
        s
    }

    fn sse(event_type: &str, data: Value) -> Bytes {
        // OpenAI Responses SSE：event 名与 data.type 一致
        Bytes::from(format!(
            "event: {event_type}\ndata: {}\n\n",
            data.to_string()
        ))
    }

    fn created_events(&mut self) -> Vec<Bytes> {
        let seq = self.next_seq();
        let created = json!({
            "type": "response.created",
            "sequence_number": seq,
            "response": {
                "id": self.response_id,
                "object": "response",
                "created_at": chrono_now(),
                "status": "in_progress",
                "model": self.model,
                "output": [],
                "usage": null
            }
        });
        let seq2 = self.next_seq();
        let in_progress = json!({
            "type": "response.in_progress",
            "sequence_number": seq2,
            "response": {
                "id": self.response_id,
                "object": "response",
                "created_at": chrono_now(),
                "status": "in_progress",
                "model": self.model,
                "output": [],
                "usage": null
            }
        });
        vec![
            Self::sse("response.created", created),
            Self::sse("response.in_progress", in_progress),
        ]
    }

    fn ensure_text_started(&mut self, out: &mut Vec<Bytes>) {
        if self.text_started {
            return;
        }
        self.text_started = true;
        let seq = self.next_seq();
        let added = json!({
            "type": "response.output_item.added",
            "sequence_number": seq,
            "output_index": 0,
            "item": {
                "type": "message",
                "id": self.text_item_id,
                "status": "in_progress",
                "role": "assistant",
                "content": []
            }
        });
        out.push(Self::sse("response.output_item.added", added));

        let seq = self.next_seq();
        let part_added = json!({
            "type": "response.content_part.added",
            "sequence_number": seq,
            "item_id": self.text_item_id,
            "output_index": 0,
            "content_index": 0,
            "part": {
                "type": "output_text",
                "text": ""
            }
        });
        out.push(Self::sse("response.content_part.added", part_added));
    }

    fn on_text_delta(&mut self, text: &str) -> Vec<Bytes> {
        if text.is_empty() {
            return Vec::new();
        }
        self.output_tokens += ((text.len() as i32) + 3) / 4;
        self.text_buf.push_str(text);
        let mut out = Vec::new();
        self.ensure_text_started(&mut out);
        let seq = self.next_seq();
        let delta = json!({
            "type": "response.output_text.delta",
            "sequence_number": seq,
            "item_id": self.text_item_id,
            "output_index": 0,
            "content_index": 0,
            "delta": text
        });
        out.push(Self::sse("response.output_text.delta", delta));
        out
    }

    fn finish_text_item(&mut self, out: &mut Vec<Bytes>) {
        if !self.text_started {
            return;
        }
        let seq = self.next_seq();
        out.push(Self::sse(
            "response.output_text.done",
            json!({
                "type": "response.output_text.done",
                "sequence_number": seq,
                "item_id": self.text_item_id,
                "output_index": 0,
                "content_index": 0,
                "text": self.text_buf
            }),
        ));
        let seq = self.next_seq();
        out.push(Self::sse(
            "response.content_part.done",
            json!({
                "type": "response.content_part.done",
                "sequence_number": seq,
                "item_id": self.text_item_id,
                "output_index": 0,
                "content_index": 0,
                "part": {
                    "type": "output_text",
                    "text": self.text_buf
                }
            }),
        ));
        let seq = self.next_seq();
        out.push(Self::sse(
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "sequence_number": seq,
                "output_index": 0,
                "item": {
                    "type": "message",
                    "id": self.text_item_id,
                    "status": "completed",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": self.text_buf
                    }]
                }
            }),
        ));
    }

    fn on_tool_use(
        &mut self,
        tool_use_id: &str,
        name: &str,
        input_chunk: &str,
        stop: bool,
    ) -> Vec<Bytes> {
        self.has_tool_use = true;
        let mut out = Vec::new();

        // 若文本还在进行，先不强制关闭；Codex 允许 message 与 function_call 并列
        // commercial converter 无 tool_name_map，工具名原样回传
        let resolved_name = name.to_string();

        let is_new = !self.tools.contains_key(tool_use_id);
        if is_new {
            let call_id = if tool_use_id.starts_with("call_") {
                tool_use_id.to_string()
            } else if tool_use_id.starts_with("tooluse_") {
                format!("call_{}", &tool_use_id[8..])
            } else {
                format!("call_{tool_use_id}")
            };
            self.tools.insert(
                tool_use_id.to_string(),
                ToolCallState {
                    item_id: format!("fc_{}", Uuid::new_v4().to_string().replace('-', "")),
                    call_id,
                    name: resolved_name.clone(),
                    arguments: String::new(),
                    added: false,
                },
            );
        }

        // 更新 name（首次可能为空）— 先取出需要的字段，避免跨可变借用
        {
            if let Some(entry) = self.tools.get_mut(tool_use_id) {
                if entry.name.is_empty() || entry.name == "unknown" {
                    entry.name = resolved_name;
                }
            }
        }

        let (item_id, call_id, tool_name, need_added, output_index) = {
            let entry = self.tools.get(tool_use_id).expect("tool just inserted");
            let output_index = 1 + self.tools.len() as i64 - 1;
            (
                entry.item_id.clone(),
                entry.call_id.clone(),
                entry.name.clone(),
                !entry.added,
                output_index,
            )
        };

        if need_added {
            if let Some(entry) = self.tools.get_mut(tool_use_id) {
                entry.added = true;
            }
            let seq = self.next_seq();
            out.push(Self::sse(
                "response.output_item.added",
                json!({
                    "type": "response.output_item.added",
                    "sequence_number": seq,
                    "output_index": output_index,
                    "item": {
                        "type": "function_call",
                        "id": item_id,
                        "call_id": call_id,
                        "name": tool_name,
                        "arguments": "",
                        "status": "in_progress"
                    }
                }),
            ));
        }

        if !input_chunk.is_empty() {
            if let Some(entry) = self.tools.get_mut(tool_use_id) {
                entry.arguments.push_str(input_chunk);
            }
            self.output_tokens += ((input_chunk.len() as i32) + 3) / 4;
            let seq = self.next_seq();
            out.push(Self::sse(
                "response.function_call_arguments.delta",
                json!({
                    "type": "response.function_call_arguments.delta",
                    "sequence_number": seq,
                    "item_id": item_id,
                    "output_index": 1,
                    "delta": input_chunk
                }),
            ));
        }

        if stop {
            let arguments = self
                .tools
                .get(tool_use_id)
                .map(|t| t.arguments.clone())
                .unwrap_or_default();
            let seq = self.next_seq();
            out.push(Self::sse(
                "response.function_call_arguments.done",
                json!({
                    "type": "response.function_call_arguments.done",
                    "sequence_number": seq,
                    "item_id": item_id,
                    "output_index": 1,
                    "arguments": arguments
                }),
            ));
            let seq = self.next_seq();
            out.push(Self::sse(
                "response.output_item.done",
                json!({
                    "type": "response.output_item.done",
                    "sequence_number": seq,
                    "output_index": 1,
                    "item": {
                        "type": "function_call",
                        "id": item_id,
                        "call_id": call_id,
                        "name": tool_name,
                        "arguments": arguments,
                        "status": "completed"
                    }
                }),
            ));
        }

        out
    }

    fn build_output_array(&self) -> Vec<Value> {
        let mut output = Vec::new();
        if self.text_started || !self.text_buf.is_empty() {
            output.push(json!({
                "type": "message",
                "id": self.text_item_id,
                "status": "completed",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": self.text_buf
                }]
            }));
        }
        for t in self.tools.values() {
            output.push(json!({
                "type": "function_call",
                "id": t.item_id,
                "call_id": t.call_id,
                "name": t.name,
                "arguments": t.arguments,
                "status": "completed"
            }));
        }
        output
    }

    fn completed_events(&mut self) -> Vec<Bytes> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;

        let mut out = Vec::new();
        self.finish_text_item(&mut out);

        // 确保未 stop 的 tool 也 done
        let pending: Vec<(String, String, String, String)> = self
            .tools
            .iter()
            .filter(|(_, t)| {
                // 若 arguments done 已发过不好判断；简化：始终在 completed 前保证 output 完整
                !t.arguments.is_empty() || t.added
            })
            .map(|(_, t)| {
                (
                    t.item_id.clone(),
                    t.call_id.clone(),
                    t.name.clone(),
                    t.arguments.clone(),
                )
            })
            .collect();
        let _ = pending; // output_array 已包含

        let status = if self.has_tool_use {
            "completed"
        } else {
            "completed"
        };

        let output = self.build_output_array();
        let usage = json!({
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens.max(1),
            "total_tokens": self.input_tokens + self.output_tokens.max(1),
            "input_tokens_details": { "cached_tokens": 0 },
            "output_tokens_details": { "reasoning_tokens": 0 }
        });

        if let (Some(tracker), Some(key_id)) = (&self.usage_tracker, self.api_key_id) {
            tracker.record(
                key_id,
                self.model.clone(),
                self.input_tokens,
                self.output_tokens.max(1),
            );
        }

        let seq = self.next_seq();
        out.push(Self::sse(
            "response.completed",
            json!({
                "type": "response.completed",
                "sequence_number": seq,
                "response": {
                    "id": self.response_id,
                    "object": "response",
                    "created_at": chrono_now(),
                    "status": status,
                    "model": self.model,
                    "output": output,
                    "usage": usage
                }
            }),
        ));
        out
    }

    fn process_event(&mut self, event: &Event) -> Vec<Bytes> {
        match event {
            Event::AssistantResponse(resp) => self.on_text_delta(&resp.content),
            Event::ToolUse(tool_use) => {
                self.on_tool_use(&tool_use.tool_use_id, &tool_use.name, &tool_use.input, tool_use.stop)
            }
            Event::Error {
                error_code,
                error_message,
            } => {
                tracing::error!(%error_code, %error_message, "Responses 流收到错误事件");
                let seq = self.next_seq();
                vec![Self::sse(
                    "response.failed",
                    json!({
                        "type": "response.failed",
                        "sequence_number": seq,
                        "response": {
                            "id": self.response_id,
                            "object": "response",
                            "created_at": chrono_now(),
                            "status": "failed",
                            "model": self.model,
                            "error": {
                                "code": error_code,
                                "message": error_message
                            }
                        }
                    }),
                )]
            }
            _ => Vec::new(),
        }
    }
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn handle_responses_stream(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    input_tokens: i32,
    usage_tracker: Option<std::sync::Arc<crate::model::usage::UsageTracker>>,
    api_key_id: Option<u32>,
) -> Response {
    let response = match provider.call_api_stream(request_body).await {
        Ok(resp) => resp,
        Err(e) => return openai_error_from_provider(e),
    };

    let mut state = ResponsesStreamState::new(model, input_tokens, usage_tracker, api_key_id);
    let initial = state.created_events();

    let stream = create_responses_sse_stream(response, state, initial);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(stream))
        .unwrap()
}

fn create_responses_sse_stream(
    response: reqwest::Response,
    state: ResponsesStreamState,
    initial_events: Vec<Bytes>,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    let initial_stream = stream::iter(initial_events.into_iter().map(Ok));
    let body_stream = response.bytes_stream();

    let processing_stream = stream::unfold(
        (body_stream, state, EventStreamDecoder::new(), false),
        |(mut body_stream, mut state, mut decoder, finished)| async move {
            if finished {
                return None;
            }

            match body_stream.next().await {
                Some(Ok(chunk)) => {
                    if let Err(e) = decoder.feed(&chunk) {
                        tracing::warn!("Responses 流缓冲区溢出: {}", e);
                    }
                    let mut events = Vec::new();
                    for result in decoder.decode_iter() {
                        match result {
                            Ok(frame) => {
                                if let Ok(event) = Event::from_frame(frame) {
                                    events.extend(state.process_event(&event));
                                }
                            }
                            Err(e) => tracing::warn!("Responses 解码失败: {}", e),
                        }
                    }
                    let bytes: Vec<Result<Bytes, Infallible>> =
                        events.into_iter().map(Ok).collect();
                    Some((stream::iter(bytes), (body_stream, state, decoder, false)))
                }
                Some(Err(e)) => {
                    tracing::error!("Responses 读取流失败: {}", e);
                    let final_events = state.completed_events();
                    let bytes: Vec<Result<Bytes, Infallible>> =
                        final_events.into_iter().map(Ok).collect();
                    Some((stream::iter(bytes), (body_stream, state, decoder, true)))
                }
                None => {
                    let final_events = state.completed_events();
                    let bytes: Vec<Result<Bytes, Infallible>> =
                        final_events.into_iter().map(Ok).collect();
                    Some((stream::iter(bytes), (body_stream, state, decoder, true)))
                }
            }
        },
    )
    .flatten();

    initial_stream.chain(processing_stream)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Non-stream
// ═══════════════════════════════════════════════════════════════════════════════

async fn handle_responses_non_stream(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    input_tokens: i32,
    usage_tracker: Option<std::sync::Arc<crate::model::usage::UsageTracker>>,
    api_key_id: Option<u32>,
) -> Response {
    let response = match provider.call_api(request_body).await {
        Ok(resp) => resp,
        Err(e) => return openai_error_from_provider(e),
    };

    let body_bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return openai_error(
                StatusCode::BAD_GATEWAY,
                "server_error",
                format!("read response failed: {e}"),
            );
        }
    };

    let mut decoder = EventStreamDecoder::new();
    if let Err(e) = decoder.feed(&body_bytes) {
        tracing::warn!("缓冲区溢出: {}", e);
    }

    let mut text_content = String::new();
    let mut tool_json_buffers: HashMap<String, String> = HashMap::new();
    let mut tool_meta: HashMap<String, String> = HashMap::new(); // id -> name
    let mut completed_tools: Vec<(String, String, String)> = Vec::new(); // id, name, args
    let mut output_tokens = 0i32;

    for result in decoder.decode_iter() {
        match result {
            Ok(frame) => {
                if let Ok(event) = Event::from_frame(frame) {
                    match event {
                        Event::AssistantResponse(resp) => {
                            output_tokens += ((resp.content.len() as i32) + 3) / 4;
                            text_content.push_str(&resp.content);
                        }
                        Event::ToolUse(tool_use) => {
                            // commercial 无 tool_name_map，原样使用上游名称
                            tool_meta.insert(tool_use.tool_use_id.clone(), tool_use.name);
                            let buf = tool_json_buffers
                                .entry(tool_use.tool_use_id.clone())
                                .or_default();
                            buf.push_str(&tool_use.input);
                            output_tokens += ((tool_use.input.len() as i32) + 3) / 4;
                            if tool_use.stop {
                                let args = tool_json_buffers
                                    .get(&tool_use.tool_use_id)
                                    .cloned()
                                    .unwrap_or_default();
                                let name = tool_meta
                                    .get(&tool_use.tool_use_id)
                                    .cloned()
                                    .unwrap_or_else(|| "unknown".to_string());
                                completed_tools.push((
                                    tool_use.tool_use_id,
                                    name,
                                    args,
                                ));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => tracing::warn!("解码失败: {}", e),
        }
    }

    if let (Some(tracker), Some(key_id)) = (&usage_tracker, api_key_id) {
        tracker.record(
            key_id,
            model.to_string(),
            input_tokens,
            output_tokens.max(1),
        );
    }

    let response_id = format!("resp_{}", Uuid::new_v4().to_string().replace('-', ""));
    let mut output = Vec::new();

    if !text_content.is_empty() {
        output.push(json!({
            "type": "message",
            "id": format!("msg_{}", Uuid::new_v4().to_string().replace('-', "")),
            "status": "completed",
            "role": "assistant",
            "content": [{
                "type": "output_text",
                "text": text_content
            }]
        }));
    }

    for (tool_use_id, name, args) in completed_tools {
        let call_id = if tool_use_id.starts_with("call_") {
            tool_use_id.clone()
        } else if tool_use_id.starts_with("tooluse_") {
            format!("call_{}", &tool_use_id[8..])
        } else {
            format!("call_{tool_use_id}")
        };
        output.push(json!({
            "type": "function_call",
            "id": format!("fc_{}", Uuid::new_v4().to_string().replace('-', "")),
            "call_id": call_id,
            "name": name,
            "arguments": args,
            "status": "completed"
        }));
    }

    // 若完全空输出，给一个空 message，避免客户端无 output
    if output.is_empty() {
        output.push(json!({
            "type": "message",
            "id": format!("msg_{}", Uuid::new_v4().to_string().replace('-', "")),
            "status": "completed",
            "role": "assistant",
            "content": [{
                "type": "output_text",
                "text": ""
            }]
        }));
    }

    let body = json!({
        "id": response_id,
        "object": "response",
        "created_at": chrono_now(),
        "status": "completed",
        "model": model,
        "output": output,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens.max(1),
            "total_tokens": input_tokens + output_tokens.max(1),
            "input_tokens_details": { "cached_tokens": 0 },
            "output_tokens_details": { "reasoning_tokens": 0 }
        }
    });

    (StatusCode::OK, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_simple_string_input() {
        let req = ResponsesRequest {
            model: "gpt-5.6-terra".into(),
            input: json!("Hello"),
            instructions: Some("You are helpful".into()),
            tools: None,
            tool_choice: None,
            stream: false,
            max_output_tokens: Some(128),
            max_tokens: None,
            reasoning: None,
            _extra: HashMap::new(),
        };
        let msg = convert_responses_to_messages(&req).unwrap();
        assert_eq!(msg.model, "gpt-5.6-terra");
        assert_eq!(msg.max_tokens, 128);
        assert_eq!(msg.messages.len(), 1);
        assert_eq!(msg.messages[0].role, "user");
        assert_eq!(
            msg.system.as_ref().unwrap()[0].text,
            "You are helpful"
        );
    }

    #[test]
    fn convert_message_array_input() {
        let req = ResponsesRequest {
            model: "gpt-5.6-sol".into(),
            input: json!([
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hi"}]},
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Hey"}]},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "How are you?"}]}
            ]),
            instructions: None,
            tools: None,
            tool_choice: None,
            stream: true,
            max_output_tokens: None,
            max_tokens: None,
            reasoning: None,
            _extra: HashMap::new(),
        };
        let msg = convert_responses_to_messages(&req).unwrap();
        assert_eq!(msg.messages.len(), 3);
        assert_eq!(msg.messages[0].role, "user");
        assert_eq!(msg.messages[1].role, "assistant");
        assert_eq!(msg.messages[2].role, "user");
        assert_eq!(msg.max_tokens, 32_000);
    }

    #[test]
    fn convert_function_call_roundtrip() {
        let req = ResponsesRequest {
            model: "gpt-5.6-luna".into(),
            input: json!([
                {"type": "message", "role": "user", "content": "list files"},
                {"type": "function_call", "call_id": "call_abc", "name": "shell", "arguments": "{\"cmd\":\"ls\"}"},
                {"type": "function_call_output", "call_id": "call_abc", "output": "a.txt\nb.txt"},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "thanks"}]}
            ]),
            instructions: None,
            tools: Some(vec![json!({
                "type": "function",
                "name": "shell",
                "description": "Run shell",
                "parameters": {"type": "object", "properties": {"cmd": {"type": "string"}}}
            })]),
            tool_choice: None,
            stream: true,
            max_output_tokens: Some(256),
            max_tokens: None,
            reasoning: None,
            _extra: HashMap::new(),
        };
        let msg = convert_responses_to_messages(&req).unwrap();
        // user, assistant(tool_use), user(tool_result), user
        assert!(msg.messages.len() >= 3);
        assert!(msg.tools.as_ref().unwrap().iter().any(|t| t.name == "shell"));

        // assistant 应含 tool_use
        let assistant = msg
            .messages
            .iter()
            .find(|m| m.role == "assistant")
            .expect("assistant message");
        let content = assistant.content.to_string();
        assert!(content.contains("tool_use"));
        assert!(content.contains("call_abc") || content.contains("shell"));

        // user 应含 tool_result
        let has_tool_result = msg.messages.iter().any(|m| {
            m.role == "user" && m.content.to_string().contains("tool_result")
        });
        assert!(has_tool_result);
    }

    #[test]
    fn convert_empty_input_errors() {
        let req = ResponsesRequest {
            model: "gpt-5.6-terra".into(),
            input: json!([]),
            instructions: None,
            tools: None,
            tool_choice: None,
            stream: false,
            max_output_tokens: None,
            max_tokens: None,
            reasoning: None,
            _extra: HashMap::new(),
        };
        assert!(convert_responses_to_messages(&req).is_err());
    }

    #[test]
    fn convert_namespace_tools() {
        let req = ResponsesRequest {
            model: "gpt-5.6-terra".into(),
            input: json!("hi"),
            instructions: None,
            tools: Some(vec![json!({
                "type": "namespace",
                "name": "default",
                "tools": [
                    {
                        "type": "function",
                        "name": "read_file",
                        "description": "Read a file",
                        "parameters": {"type": "object", "properties": {}}
                    }
                ]
            })]),
            tool_choice: None,
            stream: true,
            max_output_tokens: None,
            max_tokens: None,
            reasoning: None,
            _extra: HashMap::new(),
        };
        let msg = convert_responses_to_messages(&req).unwrap();
        assert_eq!(msg.tools.as_ref().unwrap().len(), 1);
        assert_eq!(msg.tools.as_ref().unwrap()[0].name, "read_file");
    }

    #[test]
    fn convert_system_role_to_instructions() {
        let req = ResponsesRequest {
            model: "gpt-5.6-terra".into(),
            input: json!([
                {"role": "system", "content": "Be brief"},
                {"role": "user", "content": "Hi"}
            ]),
            instructions: Some("Base".into()),
            tools: None,
            tool_choice: None,
            stream: false,
            max_output_tokens: None,
            max_tokens: None,
            reasoning: None,
            _extra: HashMap::new(),
        };
        let msg = convert_responses_to_messages(&req).unwrap();
        let system = msg.system.unwrap();
        assert_eq!(system.len(), 2);
        assert_eq!(system[0].text, "Base");
        assert_eq!(system[1].text, "Be brief");
    }
}
