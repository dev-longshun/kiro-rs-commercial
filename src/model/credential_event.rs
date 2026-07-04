use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use parking_lot::Mutex;
use serde::Serialize;

const MAX_ERROR_EVENTS: usize = 100;
const SUCCESS_TTL_SECS: i64 = 300;

/// 凭据事件类型
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialEventType {
    ApiSuccess,
    ApiFailure,
    RateLimited,
    QuotaExhausted,
    TokenRefreshSuccess,
    TokenRefreshFailure,
    AutoDisabled,
    SelfHealingRecovery,
    ManualEnabled,
    ManualDisabled,
    NetworkError,
    ModelFallback,
    PoolFailover,
}

impl CredentialEventType {
    fn is_error(&self) -> bool {
        matches!(
            self,
            CredentialEventType::ApiFailure
                | CredentialEventType::NetworkError
                | CredentialEventType::TokenRefreshFailure
                | CredentialEventType::RateLimited
                | CredentialEventType::QuotaExhausted
        )
    }
}

/// 单条凭据事件
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: CredentialEventType,
    pub credential_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpm: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 请求头摘要（用于调试 403 等问题）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_headers: Option<std::collections::HashMap<String, String>>,
    /// 代理名称（来自代理池或凭据/全局代理）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_name: Option<String>,
    /// 实际使用的代理 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    /// 模型降级：原始请求模型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_model: Option<String>,
    /// 模型降级：最终成功的回退模型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_model: Option<String>,
    /// 模型降级：完整回退链
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_chain: Option<Vec<String>>,
}

impl CredentialEvent {
    pub fn new(event_type: CredentialEventType, credential_id: u64) -> Self {
        Self {
            timestamp: Utc::now(),
            event_type,
            credential_id,
            status_code: None,
            body_snippet: None,
            url: None,
            proxy_id: None,
            attempt: None,
            max_retries: None,
            rpm: None,
            reason: None,
            request_headers: None,
            proxy_name: None,
            proxy_url: None,
            original_model: None,
            fallback_model: None,
            fallback_chain: None,
        }
    }

    pub fn with_status(mut self, code: u16) -> Self {
        self.status_code = Some(code);
        self
    }

    pub fn with_body(mut self, body: &str) -> Self {
        self.body_snippet = Some(body.chars().take(200).collect());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_proxy(mut self, proxy_id: Option<u32>) -> Self {
        self.proxy_id = proxy_id;
        self
    }

    pub fn with_attempt(mut self, attempt: usize, max_retries: usize) -> Self {
        self.attempt = Some(attempt);
        self.max_retries = Some(max_retries);
        self
    }

    pub fn with_rpm(mut self, rpm: Option<u64>) -> Self {
        self.rpm = rpm;
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn with_request_headers(
        mut self,
        headers: std::collections::HashMap<String, String>,
    ) -> Self {
        self.request_headers = Some(headers);
        self
    }

    pub fn with_proxy_detail(mut self, name: Option<String>, url: Option<String>) -> Self {
        self.proxy_name = name;
        self.proxy_url = url;
        self
    }

    #[allow(dead_code)]
    pub fn with_fallback_info(
        mut self,
        original_model: impl Into<String>,
        fallback_model: impl Into<String>,
        fallback_chain: Vec<String>,
    ) -> Self {
        self.original_model = Some(original_model.into());
        self.fallback_model = Some(fallback_model.into());
        self.fallback_chain = Some(fallback_chain);
        self
    }
}

struct PerCredentialEvents {
    errors: VecDeque<CredentialEvent>,
    successes: VecDeque<CredentialEvent>,
}

/// 凭据事件存储（错误队列 + 成功队列分离）
pub struct CredentialEventStore {
    events: DashMap<u64, Mutex<PerCredentialEvents>>,
}

impl CredentialEventStore {
    pub fn new() -> Self {
        Self {
            events: DashMap::new(),
        }
    }

    /// 推入一条事件
    pub fn push(&self, event: CredentialEvent) {
        let id = event.credential_id;
        let is_error = event.event_type.is_error();
        let entry = self.events.entry(id).or_insert_with(|| {
            Mutex::new(PerCredentialEvents {
                errors: VecDeque::new(),
                successes: VecDeque::new(),
            })
        });
        let mut per = entry.lock();
        if is_error {
            if per.errors.len() >= MAX_ERROR_EVENTS {
                per.errors.pop_front();
            }
            per.errors.push_back(event);
        } else {
            per.successes.push_back(event);
        }
    }

    /// 获取某个凭据的所有事件（按时间正序）
    pub fn get_events(&self, credential_id: u64) -> Vec<CredentialEvent> {
        self.events
            .get(&credential_id)
            .map(|entry| {
                let per = entry.lock();
                let mut all: Vec<_> = per
                    .errors
                    .iter()
                    .chain(per.successes.iter())
                    .cloned()
                    .collect();
                all.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                all
            })
            .unwrap_or_default()
    }

    /// 获取所有凭据的最近事件（按时间倒序，限制条数）
    pub fn get_all_recent(&self, limit: usize) -> Vec<CredentialEvent> {
        let mut all: Vec<CredentialEvent> = self
            .events
            .iter()
            .flat_map(|entry| {
                let per = entry.value().lock();
                per.errors
                    .iter()
                    .chain(per.successes.iter())
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all.truncate(limit);
        all
    }

    /// 清除所有错误类型的事件
    pub fn clear_error_events(&self) -> usize {
        let mut total_removed = 0;
        for entry in self.events.iter() {
            let mut per = entry.value().lock();
            total_removed += per.errors.len();
            per.errors.clear();
        }
        total_removed
    }

    /// 清理过期的成功事件（超过 TTL 的自动删除）
    pub fn cleanup_expired_successes(&self) {
        let cutoff = Utc::now() - chrono::Duration::seconds(SUCCESS_TTL_SECS);
        for entry in self.events.iter() {
            let mut per = entry.value().lock();
            per.successes.retain(|e| e.timestamp > cutoff);
        }
    }
}
