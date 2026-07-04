//! Admin proxy pool handlers.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Serialize;

use super::{
    middleware::AdminState,
    types::{
        AddProxyRequest, AdminErrorResponse, BoundCredentialInfo, ProxyBindingEntry,
        SetProxyBindingRequest, SetProxyEnabledRequest, SuccessResponse, UpdateProxyRequest,
    },
};

fn pool_unavailable() -> axum::response::Response {
    let error = AdminErrorResponse::internal_error("代理池未启用");
    (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response()
}

pub async fn list_proxies(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    Json(pool.list()).into_response()
}

pub async fn add_proxy(
    State(state): State<AdminState>,
    Json(payload): Json<AddProxyRequest>,
) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    match pool.add(
        payload.name,
        payload.url,
        payload.username,
        payload.password,
        payload.tags,
    ) {
        Ok(entry) => (StatusCode::CREATED, Json(entry)).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(AdminErrorResponse::invalid_request(err.to_string())),
        )
            .into_response(),
    }
}

pub async fn update_proxy(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
    Json(payload): Json<UpdateProxyRequest>,
) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    match pool.update(
        id,
        payload.name,
        payload.url,
        payload.username,
        payload.password,
        payload.tags,
    ) {
        Ok(Some(entry)) => Json(entry).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(AdminErrorResponse::not_found(format!(
                "代理 #{} 不存在",
                id
            ))),
        )
            .into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(AdminErrorResponse::invalid_request(err.to_string())),
        )
            .into_response(),
    }
}

pub async fn delete_proxy(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    match pool.delete(id) {
        Ok(Some(entry)) => {
            let cleared = state
                .service
                .clear_proxy_url_bindings(&entry.url)
                .unwrap_or_default();
            Json(SuccessResponse::new(format!(
                "代理 #{} 已删除，已清理 {} 个账号绑定",
                id, cleared
            )))
            .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(AdminErrorResponse::not_found(format!(
                "代理 #{} 不存在",
                id
            ))),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AdminErrorResponse::internal_error(err.to_string())),
        )
            .into_response(),
    }
}

pub async fn set_proxy_enabled(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
    Json(payload): Json<SetProxyEnabledRequest>,
) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    match pool.set_enabled(id, payload.enabled) {
        Ok(true) => Json(SuccessResponse::new(format!(
            "代理 #{} 已{}",
            id,
            if payload.enabled { "启用" } else { "禁用" }
        )))
        .into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(AdminErrorResponse::not_found(format!(
                "代理 #{} 不存在",
                id
            ))),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AdminErrorResponse::internal_error(err.to_string())),
        )
            .into_response(),
    }
}

pub async fn check_proxy(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    match pool.check_single(id).await {
        Ok(Some(entry)) => Json(entry).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(AdminErrorResponse::not_found(format!(
                "代理 #{} 不存在",
                id
            ))),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AdminErrorResponse::internal_error(err.to_string())),
        )
            .into_response(),
    }
}

pub async fn delete_unhealthy_proxies(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    match pool.delete_unhealthy() {
        Ok(removed) => {
            let mut cleared = 0usize;
            for entry in &removed {
                cleared += state
                    .service
                    .clear_proxy_url_bindings(&entry.url)
                    .unwrap_or_default();
            }
            Json(serde_json::json!({ "deleted": removed.len(), "clearedBindings": cleared }))
                .into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AdminErrorResponse::internal_error(err.to_string())),
        )
            .into_response(),
    }
}

pub async fn delete_all_proxies(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    match pool.delete_all() {
        Ok(removed) => {
            let mut cleared = 0usize;
            for entry in &removed {
                cleared += state
                    .service
                    .clear_proxy_url_bindings(&entry.url)
                    .unwrap_or_default();
            }
            Json(serde_json::json!({ "deleted": removed.len(), "clearedBindings": cleared }))
                .into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AdminErrorResponse::internal_error(err.to_string())),
        )
            .into_response(),
    }
}

pub async fn disable_high_latency_proxies(
    State(state): State<AdminState>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    let threshold_ms = payload
        .get("thresholdMs")
        .or_else(|| payload.get("threshold_ms"))
        .and_then(|value| value.as_u64())
        .unwrap_or(800);
    match pool.disable_high_latency(threshold_ms) {
        Ok(disabled) => {
            Json(serde_json::json!({ "disabled": disabled, "thresholdMs": threshold_ms }))
                .into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AdminErrorResponse::internal_error(err.to_string())),
        )
            .into_response(),
    }
}

pub async fn rebalance_proxies(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    let proxies = pool.list();
    let available: Vec<_> = proxies
        .into_iter()
        .filter(|proxy| proxy.enabled && proxy.healthy)
        .collect();
    if available.is_empty() {
        return Json(SuccessResponse::new("没有可用代理，未改动账号绑定")).into_response();
    }

    let credentials = state.service.get_all_credentials();
    let mut bound = 0usize;
    for (index, credential) in credentials
        .credentials
        .iter()
        .filter(|credential| !credential.disabled)
        .enumerate()
    {
        let proxy = &available[index % available.len()];
        if let Some(entry) = pool.get(proxy.id) {
            if state
                .service
                .bind_credential_proxy(
                    credential.id,
                    Some(entry.url),
                    entry.username,
                    entry.password,
                    false,
                )
                .is_ok()
            {
                bound += 1;
            }
        }
    }
    Json(SuccessResponse::new(format!(
        "自动分配完成：{} 个账号已绑定代理",
        bound
    )))
    .into_response()
}

pub async fn get_proxy_bindings(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    let proxies = pool.list();
    let credentials = state.service.get_all_credentials();
    let mut by_url: HashMap<String, Vec<BoundCredentialInfo>> = HashMap::new();
    for credential in credentials.credentials {
        if let Some(url) = credential.proxy_url.clone() {
            by_url.entry(url).or_default().push(BoundCredentialInfo {
                id: credential.id,
                email: credential.email,
                disabled: credential.disabled,
            });
        }
    }

    let result: Vec<ProxyBindingEntry> = proxies
        .into_iter()
        .map(|proxy| ProxyBindingEntry {
            proxy_id: proxy.id,
            proxy_name: proxy.name,
            credentials: by_url.remove(&proxy.url).unwrap_or_default(),
        })
        .collect();
    Json(result).into_response()
}

pub async fn set_credential_proxy_binding(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetProxyBindingRequest>,
) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };
    if payload.direct {
        return match state
            .service
            .bind_credential_proxy(id, None, None, None, true)
        {
            Ok(()) => {
                Json(SuccessResponse::new(format!("账号 #{} 已设置为直连", id))).into_response()
            }
            Err(err) => (err.status_code(), Json(err.into_response())).into_response(),
        };
    }

    match payload.proxy_id {
        Some(proxy_id) => match pool.get(proxy_id) {
            Some(entry) => match state.service.bind_credential_proxy(
                id,
                Some(entry.url),
                entry.username,
                entry.password,
                false,
            ) {
                Ok(()) => Json(SuccessResponse::new(format!(
                    "账号 #{} 已绑定到代理 #{}",
                    id, proxy_id
                )))
                .into_response(),
                Err(err) => (err.status_code(), Json(err.into_response())).into_response(),
            },
            None => (
                StatusCode::NOT_FOUND,
                Json(AdminErrorResponse::not_found("指定代理不存在")),
            )
                .into_response(),
        },
        None => match state
            .service
            .bind_credential_proxy(id, None, None, None, false)
        {
            Ok(()) => {
                Json(SuccessResponse::new(format!("账号 #{} 已解绑代理", id))).into_response()
            }
            Err(err) => (err.status_code(), Json(err.into_response())).into_response(),
        },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExitIpResult {
    name: String,
    proxy_id: Option<u32>,
    exit_ip: Option<String>,
    error: Option<String>,
    latency_ms: u64,
}

pub async fn check_exit_ips(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(pool) = &state.proxy_pool else {
        return pool_unavailable();
    };

    let mut results = Vec::new();
    let (exit_ip, error, latency_ms) = pool.check_exit_ip_for(None).await;
    results.push(ExitIpResult {
        name: "服务器直连".to_string(),
        proxy_id: None,
        exit_ip,
        error,
        latency_ms,
    });

    for proxy in pool.list() {
        let entry = pool.get(proxy.id);
        let (exit_ip, error, latency_ms) = pool.check_exit_ip_for(entry.as_ref()).await;
        results.push(ExitIpResult {
            name: proxy.name,
            proxy_id: Some(proxy.id),
            exit_ip,
            error,
            latency_ms,
        });
    }

    Json(results).into_response()
}
