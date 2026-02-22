//! Anthropic API 中间件

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};

use crate::common::auth;
use crate::kiro::provider::KiroProvider;
use crate::model::api_key::{ApiKeyAuthResult, ApiKeyManager};

use super::types::ErrorResponse;

/// 应用共享状态
#[derive(Clone)]
pub struct AppState {
    /// 主 API 密钥（始终有效，不可禁用）
    pub api_key: String,
    /// API Key 管理器（多用户 key）
    pub api_key_manager: Option<Arc<ApiKeyManager>>,
    /// Kiro Provider（可选，用于实际 API 调用）
    pub kiro_provider: Option<Arc<KiroProvider>>,
    /// Profile ARN（可选，用于请求）
    pub profile_arn: Option<String>,
}

impl AppState {
    /// 创建新的应用状态
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            api_key_manager: None,
            kiro_provider: None,
            profile_arn: None,
        }
    }

    /// 设置 API Key 管理器
    pub fn with_api_key_manager(mut self, manager: Arc<ApiKeyManager>) -> Self {
        self.api_key_manager = Some(manager);
        self
    }

    /// 设置 KiroProvider
    pub fn with_kiro_provider(mut self, provider: KiroProvider) -> Self {
        self.kiro_provider = Some(Arc::new(provider));
        self
    }

    /// 设置 Profile ARN
    pub fn with_profile_arn(mut self, arn: impl Into<String>) -> Self {
        self.profile_arn = Some(arn.into());
        self
    }
}

/// API Key 认证中间件
///
/// 认证优先级：
/// 1. 主密钥（config.apiKey）→ 始终有效
/// 2. 用户 key（api_keys.json）→ 检查 enabled + 过期时间
pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let Some(key) = auth::extract_api_key(&request) else {
        let error = ErrorResponse::authentication_error();
        return (StatusCode::UNAUTHORIZED, Json(error)).into_response();
    };

    // 先检查主密钥
    if auth::constant_time_eq(&key, &state.api_key) {
        return next.run(request).await;
    }

    // 再检查用户 key
    if let Some(manager) = &state.api_key_manager {
        match manager.authenticate(&key) {
            ApiKeyAuthResult::Valid => return next.run(request).await,
            ApiKeyAuthResult::Disabled => {
                let error = ErrorResponse::new("permission_error", "API key has been disabled");
                return (StatusCode::FORBIDDEN, Json(error)).into_response();
            }
            ApiKeyAuthResult::Expired => {
                let error = ErrorResponse::new("permission_error", "API key has expired");
                return (StatusCode::FORBIDDEN, Json(error)).into_response();
            }
            ApiKeyAuthResult::NotFound => {}
        }
    }

    let error = ErrorResponse::authentication_error();
    (StatusCode::UNAUTHORIZED, Json(error)).into_response()
}

/// CORS 中间件层
///
/// **安全说明**：当前配置允许所有来源（Any），这是为了支持公开 API 服务。
/// 如果需要更严格的安全控制，请根据实际需求配置具体的允许来源、方法和头信息。
///
/// # 配置说明
/// - `allow_origin(Any)`: 允许任何来源的请求
/// - `allow_methods(Any)`: 允许任何 HTTP 方法
/// - `allow_headers(Any)`: 允许任何请求头
pub fn cors_layer() -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};

    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}
