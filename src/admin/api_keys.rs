//! Admin API Key 管理处理器

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Serialize;

use super::{
    middleware::AdminState,
    types::{AdminErrorResponse, CreateApiKeyRequest, SuccessResponse, UpdateApiKeyRequest},
};

/// GET /api/admin/server-info
/// 获取服务器连接信息（主 API Key）
pub async fn get_server_info(State(state): State<AdminState>) -> impl IntoResponse {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ServerInfo {
        master_api_key: Option<String>,
    }
    Json(ServerInfo {
        master_api_key: state.master_api_key.clone(),
    })
}

/// GET /api/admin/api-keys
/// 列出所有 API Key
pub async fn list_api_keys(State(state): State<AdminState>) -> impl IntoResponse {
    match &state.api_key_manager {
        Some(manager) => Json(manager.list()).into_response(),
        None => {
            let error = AdminErrorResponse::internal_error("API Key 管理未启用");
            (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response()
        }
    }
}

/// POST /api/admin/api-keys
/// 创建新 API Key
pub async fn create_api_key(
    State(state): State<AdminState>,
    Json(payload): Json<CreateApiKeyRequest>,
) -> impl IntoResponse {
    let Some(manager) = &state.api_key_manager else {
        let error = AdminErrorResponse::internal_error("API Key 管理未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };

    match manager.create(payload.name, payload.expires_at) {
        Ok(api_key) => (StatusCode::CREATED, Json(api_key)).into_response(),
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// PUT /api/admin/api-keys/:id
/// 更新 API Key
pub async fn update_api_key(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
    Json(payload): Json<UpdateApiKeyRequest>,
) -> impl IntoResponse {
    let Some(manager) = &state.api_key_manager else {
        let error = AdminErrorResponse::internal_error("API Key 管理未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };

    match manager.update(id, payload.name, payload.enabled, payload.expires_at) {
        Ok(Some(api_key)) => Json(api_key).into_response(),
        Ok(None) => {
            let error = AdminErrorResponse::not_found(format!("API Key #{} 不存在", id));
            (StatusCode::NOT_FOUND, Json(error)).into_response()
        }
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// DELETE /api/admin/api-keys/:id
/// 删除 API Key
pub async fn delete_api_key(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let Some(manager) = &state.api_key_manager else {
        let error = AdminErrorResponse::internal_error("API Key 管理未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };

    match manager.delete(id) {
        Ok(true) => Json(SuccessResponse::new(format!("API Key #{} 已删除", id))).into_response(),
        Ok(false) => {
            let error = AdminErrorResponse::not_found(format!("API Key #{} 不存在", id));
            (StatusCode::NOT_FOUND, Json(error)).into_response()
        }
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}
