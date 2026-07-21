//! Admin API HTTP 处理器

use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};
use serde::Deserialize;

use super::{
    middleware::AdminState,
    types::{
        AddCredentialRequest, AddCredentialResponse, AuthFlowPollResponse, AuthSessionPollRequest,
        BuilderIdStartRequest, BuilderIdStartResponse, CacheSimulationConfigResponse,
        CompactionConfigResponse, EnableOverageAllRequest, IamSsoCompleteRequest,
        IamSsoStartRequest, IamSsoStartResponse, KiroSsoCancelRequest, KiroSsoStartRequest,
        KiroSsoStartResponse, SetBalanceAutoRefreshSettingsRequest,
        SetCacheSimulationConfigRequest, SetCompactionConfigRequest, SetDisabledRequest,
        SetLoadBalancingModeRequest, SetOverageRequest, SetPriorityRequest, SsoTokenImportRequest,
        SsoTokenImportResponse, SuccessResponse, UpdateCredentialRequest,
    },
};
use crate::anthropic::compaction::CompactionConfig;
use crate::kiro::auth_flows::{self, BuilderIdPoll, FlowCredential, KiroSsoPoll};

#[derive(Deserialize)]
pub struct ErrorEventsQuery {
    pub limit: Option<usize>,
}

fn flow_error_response(error: impl std::fmt::Display) -> axum::response::Response {
    (
        axum::http::StatusCode::BAD_REQUEST,
        Json(super::types::AdminErrorResponse::api_error(
            error.to_string(),
        )),
    )
        .into_response()
}

fn flow_source(credential: &FlowCredential) -> (Option<String>, Option<String>) {
    match credential.provider.as_deref().unwrap_or_default() {
        "BuilderId" => (
            Some("builder_id".to_string()),
            Some("AWS Builder ID".to_string()),
        ),
        "Enterprise" => (Some("iam_sso".to_string()), Some("IAM SSO".to_string())),
        "AzureAD" => (
            Some("kiro_sso".to_string()),
            Some("Microsoft 365".to_string()),
        ),
        "Kiro SSO" => (Some("kiro_sso".to_string()), Some("Kiro SSO".to_string())),
        _ if credential.auth_method == "external_idp" => (
            Some("sso_token".to_string()),
            Some("Enterprise SSO".to_string()),
        ),
        _ => (Some("manual".to_string()), Some("Manual".to_string())),
    }
}

fn flow_credential_to_add_request(credential: FlowCredential) -> AddCredentialRequest {
    let (account_source, account_source_label) = flow_source(&credential);
    AddCredentialRequest {
        access_token: None,
        refresh_token: Some(credential.refresh_token),
        auth_method: credential.auth_method,
        client_id: credential.client_id,
        client_secret: credential.client_secret,
        token_endpoint: credential.token_endpoint,
        issuer_url: credential.issuer_url,
        scopes: credential.scopes,
        profile_arn: None,
        expires_at: None,
        priority: 0,
        region: credential.region.clone(),
        auth_region: credential.region,
        api_region: None,
        machine_id: None,
        email: credential.email,
        subscription_title: None,
        current_usage: None,
        usage_limit: None,
        next_reset_at: None,
        overage_enabled: None,
        overage_capable: None,
        overage_capability_raw: None,
        account_source,
        account_source_label,
        kam_idp: None,
        kam_provider: credential.provider,
        kam_group_id: None,
        kam_group_name: None,
        labels: Vec::new(),
        proxy_url: None,
        proxy_username: None,
        proxy_password: None,
    }
}

async fn persist_flow_credential(
    state: &AdminState,
    credential: FlowCredential,
) -> Result<AddCredentialResponse, super::error::AdminServiceError> {
    state
        .service
        .add_credential(flow_credential_to_add_request(credential))
        .await
}

fn pending_auth_response(status: impl Into<String>, interval: Option<u64>) -> AuthFlowPollResponse {
    AuthFlowPollResponse {
        success: true,
        completed: false,
        status: Some(status.into()),
        interval,
        account: None,
    }
}

fn completed_auth_response(account: AddCredentialResponse) -> AuthFlowPollResponse {
    AuthFlowPollResponse {
        success: true,
        completed: true,
        status: Some("completed".to_string()),
        interval: None,
        account: Some(account),
    }
}

/// GET /api/admin/credentials
/// 获取所有凭据状态
pub async fn get_all_credentials(State(state): State<AdminState>) -> impl IntoResponse {
    let response = state.service.get_all_credentials();
    Json(response)
}

/// POST /api/admin/credentials/:id/disabled
/// 设置凭据禁用状态
pub async fn set_credential_disabled(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetDisabledRequest>,
) -> impl IntoResponse {
    match state.service.set_disabled(id, payload.disabled) {
        Ok(_) => {
            let action = if payload.disabled { "禁用" } else { "启用" };
            Json(SuccessResponse::new(format!("凭据 #{} 已{}", id, action))).into_response()
        }
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/priority
/// 设置凭据优先级
pub async fn set_credential_priority(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetPriorityRequest>,
) -> impl IntoResponse {
    match state.service.set_priority(id, payload.priority) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "凭据 #{} 优先级已设置为 {}",
            id, payload.priority
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/reset
/// 重置失败计数并重新启用
pub async fn reset_failure_count(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.reset_and_enable(id) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "凭据 #{} 失败计数已重置并重新启用",
            id
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/{id}/refresh-token
pub async fn force_refresh_token(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.force_refresh_token(id).await {
        Ok(_) => Json(SuccessResponse::new(format!(
            "凭据 #{} Token 已强制刷新",
            id
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/{id}/liveness-check
pub async fn liveness_check(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.liveness_check(id).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/credentials/:id/balance
/// 获取指定凭据的余额
pub async fn get_credential_balance(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.get_balance(id).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/{id}/overage
pub async fn set_credential_overage(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetOverageRequest>,
) -> impl IntoResponse {
    match state.service.set_overage(id, payload.enabled).await {
        Ok(_) => Json(SuccessResponse::new(format!(
            "凭据 #{} 已{}超额",
            id,
            if payload.enabled { "开启" } else { "关闭" }
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/overage/enable-all
pub async fn enable_overage_all(
    State(state): State<AdminState>,
    body: Option<Json<EnableOverageAllRequest>>,
) -> impl IntoResponse {
    let req = body.map(|Json(body)| body).unwrap_or_default();
    let scope = match (req.ids, req.all) {
        (Some(ids), _) => Some(ids),
        (None, true) => None,
        (None, false) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(super::types::AdminErrorResponse::invalid_request(
                    "必须提供 ids 限定范围，或显式 all=true 才能全局开启超额",
                )),
            )
                .into_response();
        }
    };
    Json(state.service.enable_overage_for_all_capable(scope).await).into_response()
}

/// GET /api/admin/balance/summary
pub async fn get_balance_summary(State(state): State<AdminState>) -> impl IntoResponse {
    Json(state.service.get_balance_summary()).into_response()
}

/// POST /api/admin/balance/refresh-all
pub async fn refresh_all_balances(State(state): State<AdminState>) -> impl IntoResponse {
    match state.service.refresh_all_balances().await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/balance/auto-refresh
pub async fn get_balance_auto_refresh_settings(
    State(state): State<AdminState>,
) -> impl IntoResponse {
    Json(state.service.get_balance_auto_refresh_settings()).into_response()
}

/// PUT /api/admin/balance/auto-refresh
pub async fn set_balance_auto_refresh_settings(
    State(state): State<AdminState>,
    Json(payload): Json<SetBalanceAutoRefreshSettingsRequest>,
) -> impl IntoResponse {
    match state.service.set_balance_auto_refresh_settings(payload) {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/credentials/{id}/events
pub async fn get_credential_events(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match &state.event_store {
        Some(store) => Json(serde_json::json!({
            "credentialId": id,
            "events": store.get_events(id),
        }))
        .into_response(),
        None => {
            let error = super::types::AdminErrorResponse::not_found("事件日志未启用");
            (axum::http::StatusCode::NOT_FOUND, Json(error)).into_response()
        }
    }
}

/// GET /api/admin/credentials/error-events
/// 获取所有凭据的错误类型事件（按时间倒序）
pub async fn get_error_events(
    State(state): State<AdminState>,
    Query(params): Query<ErrorEventsQuery>,
) -> impl IntoResponse {
    match &state.event_store {
        Some(store) => {
            let limit = params.limit.unwrap_or(200);
            let all = store.get_all_recent(limit);
            // 只返回错误类型的事件
            let errors: Vec<_> = all
                .into_iter()
                .filter(|e| {
                    matches!(
                        e.event_type,
                        crate::model::credential_event::CredentialEventType::ApiFailure
                            | crate::model::credential_event::CredentialEventType::NetworkError
                            | crate::model::credential_event::CredentialEventType::TokenRefreshFailure
                            | crate::model::credential_event::CredentialEventType::RateLimited
                            | crate::model::credential_event::CredentialEventType::QuotaExhausted
                    )
                })
                .collect();
            Json(serde_json::json!({
                "events": errors,
                "total": errors.len(),
            }))
            .into_response()
        }
        None => {
            let error = super::types::AdminErrorResponse::not_found("事件日志未启用");
            (axum::http::StatusCode::NOT_FOUND, Json(error)).into_response()
        }
    }
}

/// DELETE /api/admin/credentials/error-events
/// 清理所有凭据的错误类型事件
pub async fn clear_error_events(State(state): State<AdminState>) -> impl IntoResponse {
    match &state.event_store {
        Some(store) => Json(serde_json::json!({
            "success": true,
            "removed": store.clear_error_events(),
        }))
        .into_response(),
        None => {
            let error = super::types::AdminErrorResponse::not_found("事件日志未启用");
            (axum::http::StatusCode::NOT_FOUND, Json(error)).into_response()
        }
    }
}

/// POST /api/admin/credentials
/// 添加新凭据
pub async fn add_credential(
    State(state): State<AdminState>,
    Json(payload): Json<AddCredentialRequest>,
) -> impl IntoResponse {
    match state.service.add_credential(payload).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/auth/builderid/start
pub async fn start_builder_id_login(
    State(state): State<AdminState>,
    Json(payload): Json<BuilderIdStartRequest>,
) -> impl IntoResponse {
    let proxy = state.service.global_proxy();
    match auth_flows::start_builder_id_login(payload.region, state.service.config(), proxy.as_ref())
        .await
    {
        Ok(start) => Json(BuilderIdStartResponse {
            session_id: start.session_id,
            user_code: start.user_code,
            verification_uri: start.verification_uri,
            interval: start.interval,
        })
        .into_response(),
        Err(e) => flow_error_response(e),
    }
}

/// POST /api/admin/auth/builderid/poll
pub async fn poll_builder_id_login(
    State(state): State<AdminState>,
    Json(payload): Json<AuthSessionPollRequest>,
) -> impl IntoResponse {
    let proxy = state.service.global_proxy();
    match auth_flows::poll_builder_id_login(
        &payload.session_id,
        state.service.config(),
        proxy.as_ref(),
    )
    .await
    {
        Ok(BuilderIdPoll::Pending { status, interval }) => {
            Json(pending_auth_response(status, Some(interval))).into_response()
        }
        Ok(BuilderIdPoll::Completed(credential)) => {
            match persist_flow_credential(&state, credential).await {
                Ok(account) => Json(completed_auth_response(account)).into_response(),
                Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
            }
        }
        Err(e) => flow_error_response(e),
    }
}

/// POST /api/admin/auth/iam-sso/start
pub async fn start_iam_sso_login(
    State(state): State<AdminState>,
    Json(payload): Json<IamSsoStartRequest>,
) -> impl IntoResponse {
    let proxy = state.service.global_proxy();
    match auth_flows::start_iam_sso_login(
        payload.start_url,
        payload.region,
        state.service.config(),
        proxy.as_ref(),
    )
    .await
    {
        Ok(start) => Json(IamSsoStartResponse {
            session_id: start.session_id,
            authorize_url: start.authorize_url,
            expires_in: start.expires_in,
        })
        .into_response(),
        Err(e) => flow_error_response(e),
    }
}

/// POST /api/admin/auth/iam-sso/complete
pub async fn complete_iam_sso_login(
    State(state): State<AdminState>,
    Json(payload): Json<IamSsoCompleteRequest>,
) -> impl IntoResponse {
    let proxy = state.service.global_proxy();
    match auth_flows::complete_iam_sso_login(
        &payload.session_id,
        &payload.callback_url,
        state.service.config(),
        proxy.as_ref(),
    )
    .await
    {
        Ok(credential) => match persist_flow_credential(&state, credential).await {
            Ok(account) => Json(account).into_response(),
            Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
        },
        Err(e) => flow_error_response(e),
    }
}

/// POST /api/admin/auth/kiro-sso/start
pub async fn start_kiro_sso_login(
    State(state): State<AdminState>,
    Json(payload): Json<KiroSsoStartRequest>,
) -> impl IntoResponse {
    let proxy = state.service.global_proxy();
    match auth_flows::start_kiro_sso_login(payload.region, state.service.config(), proxy.as_ref())
        .await
    {
        Ok(start) => Json(KiroSsoStartResponse {
            session_id: start.session_id,
            sign_in_url: start.sign_in_url,
            interval: start.interval,
        })
        .into_response(),
        Err(e) => flow_error_response(e),
    }
}

/// POST /api/admin/auth/kiro-sso/poll
pub async fn poll_kiro_sso_login(
    State(state): State<AdminState>,
    Json(payload): Json<AuthSessionPollRequest>,
) -> impl IntoResponse {
    let proxy = state.service.global_proxy();
    match auth_flows::poll_kiro_sso_login(
        &payload.session_id,
        state.service.config(),
        proxy.as_ref(),
    )
    .await
    {
        Ok(KiroSsoPoll::Pending) => Json(pending_auth_response("pending", Some(2))).into_response(),
        Ok(KiroSsoPoll::Completed(credential)) => {
            match persist_flow_credential(&state, credential).await {
                Ok(account) => Json(completed_auth_response(account)).into_response(),
                Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
            }
        }
        Err(e) => flow_error_response(e),
    }
}

/// POST /api/admin/auth/kiro-sso/cancel
pub async fn cancel_kiro_sso_login(Json(payload): Json<KiroSsoCancelRequest>) -> impl IntoResponse {
    auth_flows::cancel_kiro_sso_login(&payload.session_id);
    Json(SuccessResponse::new("Kiro SSO 登录已取消")).into_response()
}

/// POST /api/admin/auth/sso-token
pub async fn import_sso_token(
    State(state): State<AdminState>,
    Json(payload): Json<SsoTokenImportRequest>,
) -> impl IntoResponse {
    if payload.bearer_token.trim().is_empty() {
        return flow_error_response("bearerToken is required");
    }

    let proxy = state.service.global_proxy();
    let result = auth_flows::import_sso_tokens(
        payload.bearer_token,
        payload.region,
        state.service.config(),
        proxy.as_ref(),
    )
    .await;

    let mut accounts = Vec::new();
    let mut errors = result.errors;
    for credential in result.imported {
        match persist_flow_credential(&state, credential).await {
            Ok(account) => accounts.push(account),
            Err(e) => errors.push(e.to_string()),
        }
    }

    Json(SsoTokenImportResponse {
        success: !accounts.is_empty(),
        accounts,
        errors,
    })
    .into_response()
}

/// DELETE /api/admin/credentials/:id
/// 删除凭据
pub async fn delete_credential(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.delete_credential(id) {
        Ok(_) => Json(SuccessResponse::new(format!("凭据 #{} 已删除", id))).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ExportKamQuery {
    pub enabled_only: Option<bool>,
    #[serde(rename = "enabledOnly")]
    pub enabled_only_camel: Option<bool>,
    pub ids: Option<String>,
}

/// GET /api/admin/credentials/export-kam
pub async fn export_kam(
    State(state): State<AdminState>,
    Query(query): Query<ExportKamQuery>,
) -> impl IntoResponse {
    let ids: Option<Vec<u64>> = query.ids.as_ref().map(|raw| {
        raw.split(',')
            .filter_map(|id| id.trim().parse::<u64>().ok())
            .collect()
    });
    let enabled_only = query
        .enabled_only
        .or(query.enabled_only_camel)
        .unwrap_or(false);
    Json(state.service.export_kam(enabled_only, ids.as_deref())).into_response()
}

/// PUT /api/admin/credentials/:id
/// 更新凭据配置
pub async fn update_credential(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdateCredentialRequest>,
) -> impl IntoResponse {
    match state.service.update_credential(id, payload).await {
        Ok(_) => Json(SuccessResponse::new(format!("凭据 #{} 已更新", id))).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/config/load-balancing
/// 获取负载均衡模式
pub async fn get_load_balancing_mode(State(state): State<AdminState>) -> impl IntoResponse {
    let response = state.service.get_load_balancing_mode();
    Json(response)
}

/// PUT /api/admin/config/load-balancing
/// 设置负载均衡模式
pub async fn set_load_balancing_mode(
    State(state): State<AdminState>,
    Json(payload): Json<SetLoadBalancingModeRequest>,
) -> impl IntoResponse {
    match state.service.set_load_balancing_mode(payload) {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// 将 API Key 脱敏显示（保留前半部分 + ***）
fn mask_key(key: &str) -> String {
    let visible = key.len() / 2;
    format!("{}***", &key[..visible])
}

/// GET /api/admin/config/auth-keys
/// 获取当前认证密钥（脱敏显示）
pub async fn get_auth_keys(State(state): State<AdminState>) -> impl IntoResponse {
    let api_key = state
        .master_api_key
        .as_ref()
        .map(|k| mask_key(&k.read()))
        .unwrap_or_default();
    let admin_api_key = mask_key(&state.admin_api_key.read());

    Json(super::types::AuthKeysResponse {
        api_key,
        admin_api_key,
    })
}

/// PUT /api/admin/config/auth-keys
/// 修改认证密钥（运行时生效并持久化到 config.json）
pub async fn set_auth_keys(
    State(state): State<AdminState>,
    Json(payload): Json<super::types::SetAuthKeysRequest>,
) -> impl IntoResponse {
    let new_api_key = payload.api_key.as_ref().map(|key| key.trim().to_string());
    let new_admin_api_key = payload
        .admin_api_key
        .as_ref()
        .map(|key| key.trim().to_string());

    // 验证输入
    if let Some(ref key) = new_api_key {
        if key.is_empty() {
            let error = super::types::AdminErrorResponse::invalid_request("apiKey 不能为空");
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!(error)),
            )
                .into_response();
        }
    }
    if let Some(ref key) = new_admin_api_key {
        if key.is_empty() {
            let error = super::types::AdminErrorResponse::invalid_request("adminApiKey 不能为空");
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!(error)),
            )
                .into_response();
        }
    }

    // 持久化到 config.json
    if let Some(ref config_path) = state.config_path {
        if let Err(e) = persist_auth_keys(config_path, &new_api_key, &new_admin_api_key) {
            tracing::error!("持久化认证密钥失败: {}", e);
            let error =
                super::types::AdminErrorResponse::internal_error("持久化失败，认证密钥未更新");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!(error)),
            )
                .into_response();
        }
    }

    // 更新运行时值
    if let Some(ref new_api_key) = new_api_key {
        if let Some(ref master_key) = state.master_api_key {
            *master_key.write() = new_api_key.clone();
        }
    }
    if let Some(ref new_admin_key) = new_admin_api_key {
        *state.admin_api_key.write() = new_admin_key.clone();
    }

    Json(SuccessResponse::new("认证密钥已更新")).into_response()
}

/// GET /api/admin/config/cache-simulation
pub async fn get_cache_simulation_config(State(state): State<AdminState>) -> impl IntoResponse {
    let tm = state.service.token_manager();
    Json(CacheSimulationConfigResponse {
        enabled: tm.get_cache_simulation_enabled(),
        read_ratio: tm.get_cache_read_ratio(),
        creation_ratio: tm.get_cache_creation_ratio(),
    })
}

/// PUT /api/admin/config/cache-simulation
pub async fn set_cache_simulation_config(
    State(state): State<AdminState>,
    Json(payload): Json<SetCacheSimulationConfigRequest>,
) -> impl IntoResponse {
    let tm = state.service.token_manager();
    match tm.set_cache_simulation_config(
        payload.enabled,
        payload.read_ratio,
        payload.creation_ratio,
    ) {
        Ok(()) => Json(CacheSimulationConfigResponse {
            enabled: payload.enabled,
            read_ratio: payload.read_ratio,
            creation_ratio: payload.creation_ratio,
        })
        .into_response(),
        Err(e) => {
            let error = super::types::AdminErrorResponse::internal_error(&e.to_string());
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!(error)),
            )
                .into_response()
        }
    }
}

/// GET /api/admin/config/compaction
pub async fn get_compaction_config(State(state): State<AdminState>) -> impl IntoResponse {
    let config = state.compaction_config.read().clone();
    Json(CompactionConfigResponse {
        enabled: config.enabled,
        threshold_percent: config.threshold_percent,
        preserve_recent_pairs: config.preserve_recent_pairs,
        tool_result_max_chars: config.tool_result_max_chars,
    })
}

/// PUT /api/admin/config/compaction
pub async fn set_compaction_config(
    State(state): State<AdminState>,
    Json(payload): Json<SetCompactionConfigRequest>,
) -> impl IntoResponse {
    if !(1.0..=100.0).contains(&payload.threshold_percent) {
        let error = super::types::AdminErrorResponse::invalid_request(
            "thresholdPercent 必须在 1 到 100 之间",
        );
        return (axum::http::StatusCode::BAD_REQUEST, Json(error)).into_response();
    }
    if payload.preserve_recent_pairs > 100 {
        let error =
            super::types::AdminErrorResponse::invalid_request("preserveRecentPairs 不能超过 100");
        return (axum::http::StatusCode::BAD_REQUEST, Json(error)).into_response();
    }
    if payload.tool_result_max_chars < 32 {
        let error =
            super::types::AdminErrorResponse::invalid_request("toolResultMaxChars 至少需要 32");
        return (axum::http::StatusCode::BAD_REQUEST, Json(error)).into_response();
    }

    let next = CompactionConfig {
        enabled: payload.enabled,
        threshold_percent: payload.threshold_percent,
        preserve_recent_pairs: payload.preserve_recent_pairs,
        tool_result_max_chars: payload.tool_result_max_chars,
    };

    if let Some(config_path) = &state.config_path {
        match crate::model::config::Config::load(config_path) {
            Ok(mut config) => {
                config.compaction_enabled = next.enabled;
                config.compaction_threshold_percent = next.threshold_percent;
                config.compaction_preserve_recent_pairs = next.preserve_recent_pairs;
                config.compaction_tool_result_max_chars = next.tool_result_max_chars;
                if let Err(err) = config.save() {
                    let error = super::types::AdminErrorResponse::internal_error(format!(
                        "持久化 Compaction 配置失败: {}",
                        err
                    ));
                    return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(error))
                        .into_response();
                }
            }
            Err(err) => {
                let error = super::types::AdminErrorResponse::internal_error(format!(
                    "读取配置失败: {}",
                    err
                ));
                return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(error))
                    .into_response();
            }
        }
    }

    *state.compaction_config.write() = next.clone();
    Json(CompactionConfigResponse {
        enabled: next.enabled,
        threshold_percent: next.threshold_percent,
        preserve_recent_pairs: next.preserve_recent_pairs,
        tool_result_max_chars: next.tool_result_max_chars,
    })
    .into_response()
}

/// 将修改后的密钥写回 config.json
fn persist_auth_keys(
    config_path: &std::path::Path,
    new_api_key: &Option<String>,
    new_admin_api_key: &Option<String>,
) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(config_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)?;

    if let Some(key) = new_api_key {
        json["apiKey"] = serde_json::Value::String(key.clone());
    }
    if let Some(key) = new_admin_api_key {
        json["adminApiKey"] = serde_json::Value::String(key.clone());
    }

    let output = serde_json::to_string_pretty(&json)?;
    std::fs::write(config_path, output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::persist_auth_keys;
    use serde_json::Value;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_config_path(test_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kiro-rs-{test_name}-{}-{nanos}.json",
            std::process::id()
        ))
    }

    #[test]
    fn persist_auth_keys_updates_only_provided_keys() {
        let path = temp_config_path("auth-keys");
        fs::write(
            &path,
            r#"{"host":"127.0.0.1","apiKey":"old-api","adminApiKey":"old-admin"}"#,
        )
        .unwrap();

        persist_auth_keys(&path, &Some("new-api".to_string()), &None).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["apiKey"], "new-api");
        assert_eq!(json["adminApiKey"], "old-admin");
        assert_eq!(json["host"], "127.0.0.1");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn persist_auth_keys_can_update_both_auth_keys() {
        let path = temp_config_path("auth-keys-both");
        fs::write(&path, r#"{"apiKey":"old-api","adminApiKey":"old-admin"}"#).unwrap();

        persist_auth_keys(
            &path,
            &Some("new-api".to_string()),
            &Some("new-admin".to_string()),
        )
        .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["apiKey"], "new-api");
        assert_eq!(json["adminApiKey"], "new-admin");

        let _ = fs::remove_file(path);
    }
}
