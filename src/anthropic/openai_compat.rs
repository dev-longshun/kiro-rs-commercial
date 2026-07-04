//! OpenAI Chat Completions API 兼容层
//!
//! 实现 POST /v1/chat/completions 端点，将 OpenAI 格式请求转换为
//! Anthropic 格式，复用现有的 Kiro 调用链，再将响应转回 OpenAI 格式。
//!
//! 主要用于支持 SillyTavern 等酒馆客户端的"自定义 API"模式。

use std::convert::Infallible;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Extension, Json as JsonExtractor,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Json, Response},
};
use bytes::Bytes;
use futures::{Stream, StreamExt, stream};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::kiro::model::events::Event;
use crate::kiro::model::requests::kiro::KiroRequest;
use crate::kiro::parser::decoder::EventStreamDecoder;
use crate::token;

use super::converter::{ConversionError, convert_request};
use super::middleware::{ApiKeyContext, AppState};
use super::types::{Message, MessagesRequest, SystemMessage};

// ==================== OpenAI 请求类型 ====================

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub max_tokens: Option<i32>,
    #[serde(default)]
    pub max_completion_tokens: Option<i32>,
    #[serde(default)]
    #[allow(dead_code)]
    pub temperature: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub top_p: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub stop: Option<serde_json::Value>,
    #[serde(default)]
    #[allow(dead_code)]
    pub presence_penalty: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub frequency_penalty: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub user: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: ChatMessageContent,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ChatMessageContent {
    Text(String),
    Parts(Vec<ChatContentPart>),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ChatContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    #[allow(dead_code)]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Deserialize)]
pub struct ImageUrl {
    #[allow(dead_code)]
    pub url: String,
}

// ==================== OpenAI 响应类型 ====================

#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: ChatUsage,
}

#[derive(Debug, Serialize)]
pub struct ChatChoice {
    pub index: i32,
    pub message: ChatChoiceMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChatChoiceMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatUsage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
}

#[derive(Debug, Serialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

#[derive(Debug, Serialize)]
pub struct ChunkChoice {
    pub index: i32,
    pub delta: ChunkDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

// ==================== 格式转换 ====================

fn convert_to_anthropic_request(req: ChatCompletionRequest) -> MessagesRequest {
    let mut system: Option<Vec<SystemMessage>> = None;
    let mut messages: Vec<Message> = Vec::new();

    for msg in req.messages {
        let content_text = match msg.content {
            ChatMessageContent::Text(text) => text,
            ChatMessageContent::Parts(parts) => parts
                .into_iter()
                .filter_map(|p| match p {
                    ChatContentPart::Text { text } => Some(text),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        };

        match msg.role.as_str() {
            "system" => {
                let sys_msgs = system.get_or_insert_with(Vec::new);
                sys_msgs.push(SystemMessage { text: content_text });
            }
            "user" => {
                messages.push(Message {
                    role: "user".to_string(),
                    content: serde_json::Value::String(content_text),
                });
            }
            "assistant" => {
                messages.push(Message {
                    role: "assistant".to_string(),
                    content: serde_json::Value::String(content_text),
                });
            }
            _ => {
                // tool / function 等角色当作 user 处理
                messages.push(Message {
                    role: "user".to_string(),
                    content: serde_json::Value::String(content_text),
                });
            }
        }
    }

    // 确保消息列表不为空，且最后一条是 user
    if messages.is_empty() {
        messages.push(Message {
            role: "user".to_string(),
            content: serde_json::Value::String("Hello".to_string()),
        });
    }

    let max_tokens = req.max_completion_tokens.or(req.max_tokens).unwrap_or(4096);

    MessagesRequest {
        model: req.model,
        max_tokens,
        messages,
        stream: req.stream,
        system,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    }
}

fn map_stop_reason(stop_reason: &str) -> &str {
    match stop_reason {
        "end_turn" => "stop",
        "max_tokens" => "length",
        "tool_use" => "tool_calls",
        "model_context_window_exceeded" => "length",
        _ => "stop",
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn openai_error_response(status: StatusCode, message: impl Into<String>) -> Response {
    let msg = message.into();
    (
        status,
        Json(json!({
            "error": {
                "message": msg,
                "type": "invalid_request_error",
                "code": null
            }
        })),
    )
        .into_response()
}

// ==================== Handler ====================

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(state): State<AppState>,
    identity: Option<Extension<ApiKeyContext>>,
    JsonExtractor(payload): JsonExtractor<ChatCompletionRequest>,
) -> Response {
    tracing::info!(
        model = %payload.model,
        stream = %payload.stream,
        message_count = %payload.messages.len(),
        "Received POST /v1/chat/completions request"
    );

    // 记录 RPM
    if let Some(rpm_tracker) = &state.rpm_tracker {
        let api_key_id = identity.as_ref().map(|ext| ext.0.id);
        rpm_tracker.record_request(api_key_id);
    }

    let provider = match &state.kiro_provider {
        Some(p) => p.clone(),
        None => {
            return openai_error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "API provider not configured",
            );
        }
    };

    let is_stream = payload.stream;
    let model_name = payload.model.clone();

    // 转换为 Anthropic 格式
    let anthropic_request = convert_to_anthropic_request(payload);

    // 转换为 Kiro 请求
    let conversion_result = match convert_request(&anthropic_request) {
        Ok(result) => result,
        Err(e) => {
            let message = match &e {
                ConversionError::UnsupportedModel(model) => {
                    format!("Model '{}' is not supported", model)
                }
                ConversionError::EmptyMessages => "Messages cannot be empty".to_string(),
            };
            return openai_error_response(StatusCode::BAD_REQUEST, message);
        }
    };

    let kiro_request = KiroRequest {
        conversation_state: conversion_result.conversation_state,
        profile_arn: None,
    };

    let request_body = match serde_json::to_string(&kiro_request) {
        Ok(body) => body,
        Err(e) => {
            return openai_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to serialize request: {}", e),
            );
        }
    };

    // 估算输入 tokens
    let input_tokens = token::count_all_tokens(
        anthropic_request.model.clone(),
        anthropic_request.system,
        anthropic_request.messages,
        anthropic_request.tools,
    ) as i32;

    let api_key_id = identity.map(|ext| ext.0.id);
    let usage_tracker = state.usage_tracker.clone();

    if is_stream {
        handle_stream(
            provider,
            &request_body,
            &model_name,
            input_tokens,
            usage_tracker,
            api_key_id,
        )
        .await
    } else {
        handle_non_stream(
            provider,
            &request_body,
            &model_name,
            input_tokens,
            usage_tracker,
            api_key_id,
        )
        .await
    }
}

/// 非流式响应
async fn handle_non_stream(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    input_tokens: i32,
    usage_tracker: Option<std::sync::Arc<crate::model::usage::UsageTracker>>,
    api_key_id: Option<u32>,
) -> Response {
    let response = match provider.call_api(request_body).await {
        Ok(resp) => resp,
        Err(e) => {
            return openai_error_response(
                StatusCode::BAD_GATEWAY,
                format!("Upstream API error: {}", e),
            );
        }
    };

    let body_bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            return openai_error_response(
                StatusCode::BAD_GATEWAY,
                format!("Failed to read response: {}", e),
            );
        }
    };

    // 解析事件流，收集文本
    let mut decoder = EventStreamDecoder::new();
    if let Err(e) = decoder.feed(&body_bytes) {
        tracing::warn!("缓冲区溢出: {}", e);
    }

    let mut text_content = String::new();
    let mut stop_reason = "end_turn".to_string();
    let mut output_tokens = 0i32;

    for result in decoder.decode_iter() {
        if let Ok(frame) = result {
            if let Ok(event) = Event::from_frame(frame) {
                match event {
                    Event::AssistantResponse(resp) => {
                        text_content.push_str(&resp.content);
                        output_tokens += (resp.content.len() as i32 + 3) / 4;
                    }
                    Event::ContextUsage(ctx) => {
                        if ctx.context_usage_percentage >= 100.0 {
                            stop_reason = "model_context_window_exceeded".to_string();
                        }
                    }
                    Event::Exception { exception_type, .. } => {
                        if exception_type == "ContentLengthExceededException" {
                            stop_reason = "max_tokens".to_string();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let reported_output_tokens = output_tokens.min(380);

    if let (Some(tracker), Some(key_id)) = (&usage_tracker, api_key_id) {
        tracker.record(key_id, model.to_string(), input_tokens, output_tokens);
    }

    let response_body = ChatCompletionResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4().to_string().replace('-', "")),
        object: "chat.completion".to_string(),
        created: now_unix(),
        model: model.to_string(),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatChoiceMessage {
                role: "assistant".to_string(),
                content: text_content,
            },
            finish_reason: Some(map_stop_reason(&stop_reason).to_string()),
        }],
        usage: ChatUsage {
            prompt_tokens: input_tokens,
            completion_tokens: reported_output_tokens,
            total_tokens: input_tokens + reported_output_tokens,
        },
    };

    (StatusCode::OK, Json(response_body)).into_response()
}

/// 流式响应
async fn handle_stream(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    input_tokens: i32,
    usage_tracker: Option<std::sync::Arc<crate::model::usage::UsageTracker>>,
    api_key_id: Option<u32>,
) -> Response {
    let response = match provider.call_api_stream(request_body).await {
        Ok(resp) => resp,
        Err(e) => {
            return openai_error_response(
                StatusCode::BAD_GATEWAY,
                format!("Upstream API error: {}", e),
            );
        }
    };

    let completion_id = format!("chatcmpl-{}", Uuid::new_v4().to_string().replace('-', ""));
    let created = now_unix();
    let model_owned = model.to_string();

    let stream = create_openai_stream(
        response,
        completion_id,
        created,
        model_owned,
        input_tokens,
        usage_tracker,
        api_key_id,
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// 创建 OpenAI 格式的 SSE 事件流
fn create_openai_stream(
    response: reqwest::Response,
    completion_id: String,
    created: u64,
    model: String,
    input_tokens: i32,
    usage_tracker: Option<std::sync::Arc<crate::model::usage::UsageTracker>>,
    api_key_id: Option<u32>,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    let body_stream = response.bytes_stream();

    // 先发送 role delta
    let role_chunk = ChatCompletionChunk {
        id: completion_id.clone(),
        object: "chat.completion.chunk".to_string(),
        created,
        model: model.clone(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                role: Some("assistant".to_string()),
                content: None,
            },
            finish_reason: None,
        }],
    };
    let role_bytes = format!(
        "data: {}\n\n",
        serde_json::to_string(&role_chunk).unwrap_or_default()
    );
    let initial_stream = stream::iter(vec![Ok(Bytes::from(role_bytes))]);

    let processing_stream = stream::unfold(
        (
            body_stream,
            EventStreamDecoder::new(),
            false,
            completion_id,
            created,
            model,
            0i32, // output_tokens
            "end_turn".to_string(),
            input_tokens,
            usage_tracker,
            api_key_id,
        ),
        |(
            mut body_stream,
            mut decoder,
            finished,
            completion_id,
            created,
            model,
            mut output_tokens,
            mut stop_reason,
            input_tokens,
            usage_tracker,
            api_key_id,
        )| async move {
            if finished {
                return None;
            }

            match body_stream.next().await {
                Some(Ok(chunk)) => {
                    if let Err(e) = decoder.feed(&chunk) {
                        tracing::warn!("缓冲区溢出: {}", e);
                    }

                    let mut bytes_out: Vec<Result<Bytes, Infallible>> = Vec::new();

                    for result in decoder.decode_iter() {
                        if let Ok(frame) = result {
                            if let Ok(event) = Event::from_frame(frame) {
                                match event {
                                    Event::AssistantResponse(resp) => {
                                        if !resp.content.is_empty() {
                                            // 过滤 thinking 标签
                                            let content = strip_thinking_tags(&resp.content);
                                            if !content.is_empty() {
                                                output_tokens += (content.len() as i32 + 3) / 4;
                                                let chunk = ChatCompletionChunk {
                                                    id: completion_id.clone(),
                                                    object: "chat.completion.chunk".to_string(),
                                                    created,
                                                    model: model.clone(),
                                                    choices: vec![ChunkChoice {
                                                        index: 0,
                                                        delta: ChunkDelta {
                                                            role: None,
                                                            content: Some(content),
                                                        },
                                                        finish_reason: None,
                                                    }],
                                                };
                                                let data = format!(
                                                    "data: {}\n\n",
                                                    serde_json::to_string(&chunk)
                                                        .unwrap_or_default()
                                                );
                                                bytes_out.push(Ok(Bytes::from(data)));
                                            }
                                        }
                                    }
                                    Event::ContextUsage(ctx) => {
                                        if ctx.context_usage_percentage >= 100.0 {
                                            stop_reason =
                                                "model_context_window_exceeded".to_string();
                                        }
                                    }
                                    Event::Exception { exception_type, .. } => {
                                        if exception_type == "ContentLengthExceededException" {
                                            stop_reason = "max_tokens".to_string();
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }

                    Some((
                        stream::iter(bytes_out),
                        (
                            body_stream,
                            decoder,
                            false,
                            completion_id,
                            created,
                            model,
                            output_tokens,
                            stop_reason,
                            input_tokens,
                            usage_tracker,
                            api_key_id,
                        ),
                    ))
                }
                Some(Err(e)) => {
                    tracing::error!("读取响应流失败: {}", e);
                    // 发送结束事件
                    let final_bytes =
                        generate_final_chunks(&completion_id, created, &model, &stop_reason);

                    if let (Some(tracker), Some(key_id)) = (&usage_tracker, api_key_id) {
                        tracker.record(key_id, model.clone(), input_tokens, output_tokens);
                    }

                    Some((
                        stream::iter(final_bytes),
                        (
                            body_stream,
                            decoder,
                            true,
                            completion_id,
                            created,
                            model,
                            output_tokens,
                            stop_reason,
                            input_tokens,
                            usage_tracker,
                            api_key_id,
                        ),
                    ))
                }
                None => {
                    // 流结束
                    let final_bytes =
                        generate_final_chunks(&completion_id, created, &model, &stop_reason);

                    if let (Some(tracker), Some(key_id)) = (&usage_tracker, api_key_id) {
                        tracker.record(key_id, model.clone(), input_tokens, output_tokens);
                    }

                    Some((
                        stream::iter(final_bytes),
                        (
                            body_stream,
                            decoder,
                            true,
                            completion_id,
                            created,
                            model,
                            output_tokens,
                            stop_reason,
                            input_tokens,
                            usage_tracker,
                            api_key_id,
                        ),
                    ))
                }
            }
        },
    )
    .flatten();

    initial_stream.chain(processing_stream)
}

fn generate_final_chunks(
    completion_id: &str,
    created: u64,
    model: &str,
    stop_reason: &str,
) -> Vec<Result<Bytes, Infallible>> {
    let mut bytes = Vec::new();

    // finish_reason chunk
    let chunk = ChatCompletionChunk {
        id: completion_id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created,
        model: model.to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta {
                role: None,
                content: None,
            },
            finish_reason: Some(map_stop_reason(stop_reason).to_string()),
        }],
    };
    let data = format!(
        "data: {}\n\n",
        serde_json::to_string(&chunk).unwrap_or_default()
    );
    bytes.push(Ok(Bytes::from(data)));

    // [DONE]
    bytes.push(Ok(Bytes::from("data: [DONE]\n\n")));

    bytes
}

/// 过滤 thinking 标签及其内容
///
/// 模型输出可能包含 `<thinking>...</thinking>` 内容，
/// 酒馆客户端不需要看到这些，直接过滤掉。
fn strip_thinking_tags(content: &str) -> String {
    let mut result = String::new();
    let mut remaining = content;

    loop {
        if let Some(start) = remaining.find("<thinking>") {
            result.push_str(&remaining[..start]);
            if let Some(end) = remaining[start..].find("</thinking>") {
                let after = start + end + "</thinking>".len();
                remaining = &remaining[after..];
                // 跳过紧随其后的换行
                remaining = remaining.trim_start_matches('\n');
            } else {
                // 开始标签后没有结束标签，说明 thinking 内容跨 chunk 了
                // 丢弃从 <thinking> 开始的所有内容
                break;
            }
        } else {
            // 检查是否有不完整的 <thinking 标签
            if let Some(partial) = remaining.find("<thinking") {
                // 可能是不完整标签，保留前面的内容
                result.push_str(&remaining[..partial]);
            } else {
                result.push_str(remaining);
            }
            break;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_basic_request() {
        let req = ChatCompletionRequest {
            model: "claude-sonnet-4-6".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: ChatMessageContent::Text("You are helpful.".to_string()),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: ChatMessageContent::Text("Hello!".to_string()),
                },
            ],
            stream: false,
            max_tokens: Some(1024),
            max_completion_tokens: None,
            temperature: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        };

        let result = convert_to_anthropic_request(req);
        assert_eq!(result.model, "claude-sonnet-4-6");
        assert_eq!(result.max_tokens, 1024);
        assert!(result.system.is_some());
        assert_eq!(result.system.unwrap()[0].text, "You are helpful.");
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].role, "user");
    }

    #[test]
    fn test_convert_max_completion_tokens_priority() {
        let req = ChatCompletionRequest {
            model: "claude-sonnet-4-6".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: ChatMessageContent::Text("Hi".to_string()),
            }],
            stream: false,
            max_tokens: Some(1024),
            max_completion_tokens: Some(2048),
            temperature: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        };

        let result = convert_to_anthropic_request(req);
        assert_eq!(result.max_tokens, 2048);
    }

    #[test]
    fn test_convert_default_max_tokens() {
        let req = ChatCompletionRequest {
            model: "claude-sonnet-4-6".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: ChatMessageContent::Text("Hi".to_string()),
            }],
            stream: false,
            max_tokens: None,
            max_completion_tokens: None,
            temperature: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        };

        let result = convert_to_anthropic_request(req);
        assert_eq!(result.max_tokens, 4096);
    }

    #[test]
    fn test_convert_multi_turn() {
        let req = ChatCompletionRequest {
            model: "claude-sonnet-4-6".to_string(),
            messages: vec![
                ChatMessage {
                    role: "user".to_string(),
                    content: ChatMessageContent::Text("What is 2+2?".to_string()),
                },
                ChatMessage {
                    role: "assistant".to_string(),
                    content: ChatMessageContent::Text("4".to_string()),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: ChatMessageContent::Text("And 3+3?".to_string()),
                },
            ],
            stream: false,
            max_tokens: None,
            max_completion_tokens: None,
            temperature: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        };

        let result = convert_to_anthropic_request(req);
        assert_eq!(result.messages.len(), 3);
        assert_eq!(result.messages[0].role, "user");
        assert_eq!(result.messages[1].role, "assistant");
        assert_eq!(result.messages[2].role, "user");
    }

    #[test]
    fn test_strip_thinking_tags_basic() {
        assert_eq!(strip_thinking_tags("Hello world"), "Hello world");
        assert_eq!(
            strip_thinking_tags("<thinking>some thought</thinking>\nHello"),
            "Hello"
        );
        assert_eq!(
            strip_thinking_tags("<thinking>deep thought</thinking>\n\nHello world"),
            "Hello world"
        );
    }

    #[test]
    fn test_strip_thinking_tags_partial() {
        // 不完整的 thinking 标签（跨 chunk）
        assert_eq!(strip_thinking_tags("Hello <thinking>unfinished"), "Hello ");
    }

    #[test]
    fn test_strip_thinking_tags_no_tags() {
        assert_eq!(strip_thinking_tags("Just normal text"), "Just normal text");
    }

    #[test]
    fn test_map_stop_reason() {
        assert_eq!(map_stop_reason("end_turn"), "stop");
        assert_eq!(map_stop_reason("max_tokens"), "length");
        assert_eq!(map_stop_reason("tool_use"), "tool_calls");
        assert_eq!(map_stop_reason("unknown"), "stop");
    }
}
