//! Admin API 中间件

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use parking_lot::RwLock;

use super::service::AdminService;
use super::types::AdminErrorResponse;
use crate::anthropic::compaction::CompactionConfig;
use crate::common::auth;
use crate::model::api_key::ApiKeyManager;
use crate::model::config::TlsBackend;
use crate::model::credential_event::CredentialEventStore;
use crate::model::proxy_pool::ProxyPoolManager;
use crate::model::rpm::RpmTracker;
use crate::model::usage::UsageTracker;

/// Admin API 共享状态
#[derive(Clone)]
pub struct AdminState {
    /// Admin API 密钥（运行时可修改）
    pub admin_api_key: Arc<RwLock<String>>,
    /// 主 API 密钥（用于前端展示，运行时可修改）
    pub master_api_key: Option<Arc<RwLock<String>>>,
    /// Admin 服务
    pub service: Arc<AdminService>,
    /// API Key 管理器（可选）
    pub api_key_manager: Option<Arc<ApiKeyManager>>,
    /// 用量追踪器（可选）
    pub usage_tracker: Option<Arc<UsageTracker>>,
    /// RPM 追踪器（可选）
    pub rpm_tracker: Option<Arc<RpmTracker>>,
    /// 凭据事件日志存储
    pub event_store: Option<Arc<CredentialEventStore>>,
    /// 代理池管理器
    pub proxy_pool: Option<Arc<ProxyPoolManager>>,
    /// TLS 后端配置（用于代理检查）
    pub tls_backend: TlsBackend,
    /// 运行时 Compaction 配置
    pub compaction_config: Arc<RwLock<CompactionConfig>>,
    /// 配置文件路径（用于持久化修改）
    pub config_path: Option<PathBuf>,
}

impl AdminState {
    pub fn new(admin_api_key: Arc<RwLock<String>>, service: AdminService) -> Self {
        let service = Arc::new(service);
        service.start_balance_auto_refresh();
        Self {
            admin_api_key,
            master_api_key: None,
            service,
            api_key_manager: None,
            usage_tracker: None,
            rpm_tracker: None,
            event_store: None,
            proxy_pool: None,
            tls_backend: TlsBackend::Rustls,
            compaction_config: Arc::new(RwLock::new(CompactionConfig::disabled())),
            config_path: None,
        }
    }

    pub fn with_master_api_key(mut self, key: Arc<RwLock<String>>) -> Self {
        self.master_api_key = Some(key);
        self
    }

    pub fn with_api_key_manager(mut self, manager: Arc<ApiKeyManager>) -> Self {
        self.api_key_manager = Some(manager);
        self
    }

    pub fn with_usage_tracker(mut self, tracker: Arc<UsageTracker>) -> Self {
        self.usage_tracker = Some(tracker);
        self
    }

    pub fn with_rpm_tracker(mut self, tracker: Arc<RpmTracker>) -> Self {
        self.rpm_tracker = Some(tracker);
        self
    }

    pub fn with_event_store(mut self, store: Arc<CredentialEventStore>) -> Self {
        self.event_store = Some(store);
        self
    }

    pub fn with_proxy_pool(mut self, pool: Arc<ProxyPoolManager>) -> Self {
        self.proxy_pool = Some(pool);
        self
    }

    pub fn with_tls_backend(mut self, tls_backend: TlsBackend) -> Self {
        self.tls_backend = tls_backend;
        self
    }

    pub fn with_compaction_config(mut self, config: Arc<RwLock<CompactionConfig>>) -> Self {
        self.compaction_config = config;
        self
    }

    pub fn with_config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }
}

/// Admin API 认证中间件
pub async fn admin_auth_middleware(
    State(state): State<AdminState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let api_key = auth::extract_api_key(&request);

    match api_key {
        Some(key) if auth::constant_time_eq(&key, &state.admin_api_key.read()) => {
            next.run(request).await
        }
        _ => {
            let error = AdminErrorResponse::authentication_error();
            (StatusCode::UNAUTHORIZED, Json(error)).into_response()
        }
    }
}
