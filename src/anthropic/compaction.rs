//! 对话历史压缩模块
//!
//! 当请求的 token 数超过阈值时，压缩旧消息中的 tool_result、tool_use.input、
//! thinking 块和图片，降低 token 消耗并防止上下文窗口溢出。

use crate::anthropic::types::Message;
use crate::token;
use serde_json::Value;

pub const CONTEXT_WINDOW_SIZE: u64 = 1_000_000;

#[derive(Debug, Clone)]
pub struct CompactionConfig {
    pub enabled: bool,
    pub threshold_percent: f64,
    pub preserve_recent_pairs: usize,
    pub tool_result_max_chars: usize,
}

impl CompactionConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_percent: 80.0,
            preserve_recent_pairs: 10,
            tool_result_max_chars: 200,
        }
    }
}

pub struct CompactionResult {
    pub messages: Vec<Message>,
    pub compacted: bool,
    pub original_estimate: u64,
    pub compacted_estimate: u64,
    pub tool_results_compressed: usize,
    pub thinking_blocks_removed: usize,
    pub images_removed: usize,
    pub pairs_dropped: usize,
}

/// 如果估算 token 超过阈值，压缩对话历史中的旧消息。
///
/// 压缩策略（按顺序）：
/// 1. 保留最近 N 个消息对不动
/// 2. 旧消息中：截断 tool_result 内容、tool_use input、剥离 thinking、移除图片
/// 3. 如果仍然超阈值，从最旧的消息对开始丢弃
pub fn compact_if_needed(
    messages: &[Message],
    system: &Option<Vec<crate::anthropic::types::SystemMessage>>,
    tools: &Option<Vec<crate::anthropic::types::Tool>>,
    config: &CompactionConfig,
) -> CompactionResult {
    let threshold_tokens = (CONTEXT_WINDOW_SIZE as f64 * config.threshold_percent / 100.0) as u64;

    let original_estimate = estimate_total_tokens(messages, system, tools);

    if !config.enabled || original_estimate < threshold_tokens {
        return CompactionResult {
            messages: messages.to_vec(),
            compacted: false,
            original_estimate,
            compacted_estimate: original_estimate,
            tool_results_compressed: 0,
            thinking_blocks_removed: 0,
            images_removed: 0,
            pairs_dropped: 0,
        };
    }

    let mut msgs = messages.to_vec();

    // 识别消息对边界：排除最后一条（currentMessage）
    // 保留最近 preserve_recent_pairs 对不动
    let pairs = identify_pairs(&msgs);
    let total_pairs = pairs.len();
    let compactable_end = total_pairs.saturating_sub(config.preserve_recent_pairs);

    let mut tool_results_compressed = 0usize;
    let mut thinking_blocks_removed = 0usize;
    let mut images_removed = 0usize;

    // 压缩旧消息对
    for &(user_idx, assistant_idx) in &pairs[..compactable_end] {
        let (tr, th, img) = compact_message_pair(
            &mut msgs,
            user_idx,
            assistant_idx,
            config.tool_result_max_chars,
        );
        tool_results_compressed += tr;
        thinking_blocks_removed += th;
        images_removed += img;
    }

    let compacted_estimate = estimate_total_tokens(&msgs, system, tools);

    // 紧急截断：如果压缩后仍超阈值
    let mut pairs_dropped = 0usize;
    if compacted_estimate >= threshold_tokens && compactable_end > 0 {
        let mut current_estimate = compacted_estimate;
        // 从最旧的对开始逐对丢弃
        let mut indices_to_remove: Vec<usize> = Vec::new();
        for &(user_idx, assistant_idx) in &pairs[..compactable_end] {
            if current_estimate < threshold_tokens {
                break;
            }
            let user_tokens = estimate_message_tokens(&msgs[user_idx]);
            let assistant_tokens = if let Some(ai) = assistant_idx {
                estimate_message_tokens(&msgs[ai])
            } else {
                0
            };
            current_estimate = current_estimate.saturating_sub(user_tokens + assistant_tokens);
            indices_to_remove.push(user_idx);
            if let Some(ai) = assistant_idx {
                indices_to_remove.push(ai);
            }
            pairs_dropped += 1;
        }

        // 从后向前删除，避免索引偏移
        indices_to_remove.sort_unstable();
        indices_to_remove.dedup();
        for &idx in indices_to_remove.iter().rev() {
            if idx < msgs.len() {
                msgs.remove(idx);
            }
        }
    }

    let final_estimate = if pairs_dropped > 0 {
        estimate_total_tokens(&msgs, system, tools)
    } else {
        compacted_estimate
    };

    CompactionResult {
        messages: msgs,
        compacted: true,
        original_estimate,
        compacted_estimate: final_estimate,
        tool_results_compressed,
        thinking_blocks_removed,
        images_removed,
        pairs_dropped,
    }
}

/// 识别消息对：(user_idx, Option<assistant_idx>)
/// 最后一条消息（currentMessage）不参与配对
fn identify_pairs(messages: &[Message]) -> Vec<(usize, Option<usize>)> {
    if messages.len() <= 1 {
        return vec![];
    }

    let mut pairs = Vec::new();
    let end = messages.len() - 1; // 排除最后一条
    let mut i = 0;

    while i < end {
        if messages[i].role == "user" {
            let assistant_idx = if i + 1 < end && messages[i + 1].role == "assistant" {
                Some(i + 1)
            } else {
                None
            };
            pairs.push((i, assistant_idx));
            i += if assistant_idx.is_some() { 2 } else { 1 };
        } else if messages[i].role == "assistant" {
            // 孤立的 assistant 消息，跳过
            i += 1;
        } else {
            i += 1;
        }
    }

    pairs
}

/// 压缩一个消息对，返回 (tool_results_compressed, thinking_removed, images_removed)
fn compact_message_pair(
    messages: &mut [Message],
    user_idx: usize,
    assistant_idx: Option<usize>,
    tool_result_max_chars: usize,
) -> (usize, usize, usize) {
    let mut tr_count = 0;
    let mut th_count = 0;
    let mut img_count = 0;

    // 压缩 user 消息
    let (tr, img) = compact_user_content(&mut messages[user_idx].content, tool_result_max_chars);
    tr_count += tr;
    img_count += img;

    // 压缩 assistant 消息
    if let Some(ai) = assistant_idx {
        let (tr, th, img) =
            compact_assistant_content(&mut messages[ai].content, tool_result_max_chars);
        tr_count += tr;
        th_count += th;
        img_count += img;
    }

    (tr_count, th_count, img_count)
}

/// 压缩 user 消息内容：截断 tool_result、移除图片
fn compact_user_content(content: &mut Value, max_chars: usize) -> (usize, usize) {
    let mut tr_count = 0;
    let mut img_count = 0;

    if let Value::Array(blocks) = content {
        let mut to_remove = Vec::new();

        for (idx, block) in blocks.iter_mut().enumerate() {
            if let Value::Object(map) = block {
                let block_type = map
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                match block_type.as_str() {
                    "tool_result" => {
                        if truncate_tool_result_content(map, max_chars) {
                            tr_count += 1;
                        }
                    }
                    "image" => {
                        to_remove.push(idx);
                        img_count += 1;
                    }
                    _ => {}
                }
            }
        }

        // 从后向前删除图片块
        for idx in to_remove.into_iter().rev() {
            blocks.remove(idx);
        }
    }

    (tr_count, img_count)
}

/// 压缩 assistant 消息内容：截断 tool_use input、剥离 thinking、移除图片
fn compact_assistant_content(content: &mut Value, max_chars: usize) -> (usize, usize, usize) {
    let mut tr_count = 0;
    let mut th_count = 0;
    let mut img_count = 0;

    if let Value::Array(blocks) = content {
        let mut to_remove = Vec::new();

        for (idx, block) in blocks.iter_mut().enumerate() {
            if let Value::Object(map) = block {
                let block_type = map
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                match block_type.as_str() {
                    "tool_use" => {
                        if truncate_tool_use_input(map, max_chars) {
                            tr_count += 1;
                        }
                    }
                    "thinking" => {
                        to_remove.push(idx);
                        th_count += 1;
                    }
                    "image" => {
                        to_remove.push(idx);
                        img_count += 1;
                    }
                    _ => {}
                }
            }
        }

        for idx in to_remove.into_iter().rev() {
            blocks.remove(idx);
        }
    }

    (tr_count, th_count, img_count)
}

/// 截断 tool_result 的 content 字段
fn truncate_tool_result_content(
    map: &mut serde_json::Map<String, Value>,
    max_chars: usize,
) -> bool {
    if let Some(content) = map.get("content") {
        let original_len = match content {
            Value::String(s) => s.len(),
            Value::Array(items) => items
                .iter()
                .map(|item| {
                    item.as_object()
                        .and_then(|o| o.get("text"))
                        .and_then(|t| t.as_str())
                        .map(|s| s.len())
                        .unwrap_or(0)
                })
                .sum(),
            _ => return false,
        };

        if original_len <= max_chars {
            return false;
        }

        let approx_tokens = token::count_tokens(
            &content
                .as_str()
                .unwrap_or("x")
                .repeat(original_len.min(100)),
        );
        let truncated = format!(
            "[Truncated: ~{} tokens]",
            approx_tokens.max(original_len as u64 / 4)
        );

        map.insert("content".to_string(), Value::String(truncated));
        return true;
    }
    false
}

/// 截断 tool_use 的 input 字段
fn truncate_tool_use_input(map: &mut serde_json::Map<String, Value>, max_chars: usize) -> bool {
    if let Some(input) = map.get("input") {
        let input_str = serde_json::to_string(input).unwrap_or_default();
        if input_str.len() <= max_chars {
            return false;
        }

        let tool_name = map
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let approx_tokens = input_str.len() as u64 / 4;
        let truncated_input = serde_json::json!({
            "_truncated": format!("~{} tokens from {}", approx_tokens, tool_name)
        });

        map.insert("input".to_string(), truncated_input);
        return true;
    }
    false
}

fn estimate_message_tokens(msg: &Message) -> u64 {
    estimate_content_tokens(&msg.content)
}

fn estimate_total_tokens(
    messages: &[Message],
    system: &Option<Vec<crate::anthropic::types::SystemMessage>>,
    tools: &Option<Vec<crate::anthropic::types::Tool>>,
) -> u64 {
    token::count_all_tokens(
        "compaction-estimate".to_string(),
        system.clone(),
        messages.to_vec(),
        tools.clone(),
    )
}

fn estimate_content_tokens(content: &Value) -> u64 {
    match content {
        Value::String(s) => token::count_tokens(s),
        Value::Array(blocks) => blocks
            .iter()
            .map(|block| {
                block
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(token::count_tokens)
                    .unwrap_or_else(|| {
                        serde_json::to_string(block)
                            .map(|s| token::count_tokens(&s))
                            .unwrap_or(0)
                    })
            })
            .sum(),
        other => serde_json::to_string(other)
            .map(|s| token::count_tokens(&s))
            .unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_user_msg(content: Value) -> Message {
        Message {
            role: "user".to_string(),
            content,
        }
    }

    fn make_assistant_msg(content: Value) -> Message {
        Message {
            role: "assistant".to_string(),
            content,
        }
    }

    fn make_large_tool_result(id: &str, size: usize) -> Value {
        serde_json::json!([{
            "type": "tool_result",
            "tool_use_id": id,
            "content": "x".repeat(size)
        }])
    }

    fn make_tool_use(id: &str, name: &str, input_size: usize) -> Value {
        serde_json::json!([{
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": { "content": "y".repeat(input_size) }
        }])
    }

    #[test]
    fn below_threshold_no_compaction() {
        let messages = vec![
            make_user_msg(Value::String("hello".into())),
            make_assistant_msg(Value::String("hi".into())),
            make_user_msg(Value::String("bye".into())),
        ];

        let config = CompactionConfig::default();
        let result = compact_if_needed(&messages, &None, &None, &config);

        assert!(!result.compacted);
        assert_eq!(result.messages.len(), 3);
    }

    #[test]
    fn tool_result_truncation() {
        let content = make_large_tool_result("t1", 5000);
        let mut msg = make_user_msg(content);

        let (tr, img) = compact_user_content(&mut msg.content, 200);
        assert_eq!(tr, 1);
        assert_eq!(img, 0);

        // Verify content was truncated
        if let Value::Array(blocks) = &msg.content {
            let block = blocks[0].as_object().unwrap();
            let content = block.get("content").unwrap().as_str().unwrap();
            assert!(content.starts_with("[Truncated:"));
            assert!(content.len() < 200);
        }
    }

    #[test]
    fn tool_use_input_truncation() {
        let content = make_tool_use("t1", "write_file", 5000);
        let mut msg = make_assistant_msg(content);

        let (tr, th, img) = compact_assistant_content(&mut msg.content, 200);
        assert_eq!(tr, 1);
        assert_eq!(th, 0);
        assert_eq!(img, 0);

        if let Value::Array(blocks) = &msg.content {
            let block = blocks[0].as_object().unwrap();
            let input = block.get("input").unwrap();
            assert!(input.get("_truncated").is_some());
        }
    }

    #[test]
    fn thinking_blocks_removed() {
        let content = serde_json::json!([
            { "type": "thinking", "thinking": "long internal reasoning ".repeat(100) },
            { "type": "text", "text": "Hello!" }
        ]);
        let mut msg = make_assistant_msg(content);

        let (_, th, _) = compact_assistant_content(&mut msg.content, 200);
        assert_eq!(th, 1);

        if let Value::Array(blocks) = &msg.content {
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0]["type"], "text");
        }
    }

    #[test]
    fn images_removed_from_user() {
        let content = serde_json::json!([
            { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "x".repeat(10000) }},
            { "type": "text", "text": "What's in this image?" }
        ]);
        let mut msg = make_user_msg(content);

        let (_, img) = compact_user_content(&mut msg.content, 200);
        assert_eq!(img, 1);

        if let Value::Array(blocks) = &msg.content {
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0]["type"], "text");
        }
    }

    #[test]
    fn preserves_recent_pairs() {
        let mut messages = Vec::new();
        // 制造 15 对 + 1 条当前消息
        for i in 0..15 {
            messages.push(make_user_msg(make_large_tool_result(
                &format!("t{i}"),
                5000,
            )));
            messages.push(make_assistant_msg(make_tool_use(
                &format!("t{i}"),
                "read",
                5000,
            )));
        }
        messages.push(make_user_msg(Value::String("current".into())));

        let config = CompactionConfig {
            enabled: true,
            threshold_percent: 0.0, // 强制触发
            preserve_recent_pairs: 5,
            tool_result_max_chars: 200,
        };

        let result = compact_if_needed(&messages, &None, &None, &config);
        assert!(result.compacted);

        // 最近 5 对（10 条消息）+ 最后 1 条 = 至少有这些
        // 旧的 10 对被压缩或丢弃
        assert!(result.tool_results_compressed > 0 || result.pairs_dropped > 0);
    }

    #[test]
    fn identify_pairs_basic() {
        let messages = vec![
            make_user_msg(Value::String("a".into())),
            make_assistant_msg(Value::String("b".into())),
            make_user_msg(Value::String("c".into())),
            make_assistant_msg(Value::String("d".into())),
            make_user_msg(Value::String("current".into())),
        ];

        let pairs = identify_pairs(&messages);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], (0, Some(1)));
        assert_eq!(pairs[1], (2, Some(3)));
    }

    #[test]
    fn small_tool_result_not_truncated() {
        let content = make_large_tool_result("t1", 100);
        let mut msg = make_user_msg(content);

        let (tr, _) = compact_user_content(&mut msg.content, 200);
        assert_eq!(tr, 0);
    }
}
