//! Admin API 路由配置

use axum::{
    Router, middleware,
    routing::{delete, get, post, put},
};

use super::{
    api_keys::{
        create_api_key, delete_api_key, get_all_usage, get_key_usage, get_rpm, get_server_info,
        list_api_keys, reset_key_usage, update_api_key,
    },
    handlers::{
        add_credential, cancel_kiro_sso_login, clear_error_events, complete_iam_sso_login,
        delete_credential, enable_overage_all, export_kam, force_refresh_token,
        get_all_credentials, get_auth_keys, get_balance_auto_refresh_settings, get_balance_summary,
        get_cache_simulation_config, get_compaction_config, get_credential_balance,
        get_credential_events, get_error_events, get_load_balancing_mode, import_sso_token,
        liveness_check, poll_builder_id_login, poll_kiro_sso_login, refresh_all_balances,
        reset_failure_count, set_auth_keys, set_balance_auto_refresh_settings,
        set_cache_simulation_config, set_compaction_config, set_credential_disabled,
        set_credential_overage, set_credential_priority, set_load_balancing_mode,
        start_builder_id_login, start_iam_sso_login, start_kiro_sso_login, update_credential,
    },
    middleware::{AdminState, admin_auth_middleware},
    proxy_pool::{
        add_proxy, check_exit_ips, check_proxy, delete_all_proxies, delete_proxy,
        delete_unhealthy_proxies, disable_high_latency_proxies, get_proxy_bindings, list_proxies,
        rebalance_proxies, set_credential_proxy_binding, set_proxy_enabled, update_proxy,
    },
};

/// 创建 Admin API 路由
pub fn create_admin_router(state: AdminState) -> Router {
    Router::new()
        // 凭据管理
        .route(
            "/credentials",
            get(get_all_credentials).post(add_credential),
        )
        .route(
            "/credentials/{id}",
            delete(delete_credential).put(update_credential),
        )
        .route("/credentials/{id}/disabled", post(set_credential_disabled))
        .route("/credentials/{id}/priority", post(set_credential_priority))
        .route("/credentials/{id}/reset", post(reset_failure_count))
        .route("/credentials/{id}/refresh-token", post(force_refresh_token))
        .route("/credentials/{id}/liveness-check", post(liveness_check))
        .route("/credentials/{id}/overage", post(set_credential_overage))
        .route(
            "/credentials/{id}/proxy-binding",
            put(set_credential_proxy_binding),
        )
        .route("/credentials/overage/enable-all", post(enable_overage_all))
        .route("/credentials/export-kam", get(export_kam))
        .route("/credentials/{id}/balance", get(get_credential_balance))
        .route("/credentials/{id}/events", get(get_credential_events))
        .route(
            "/credentials/error-events",
            get(get_error_events).delete(clear_error_events),
        )
        .route("/balance/summary", get(get_balance_summary))
        .route("/balance/refresh-all", post(refresh_all_balances))
        .route(
            "/balance/auto-refresh",
            get(get_balance_auto_refresh_settings).put(set_balance_auto_refresh_settings),
        )
        .route("/auth/builderid/start", post(start_builder_id_login))
        .route("/auth/builderid/poll", post(poll_builder_id_login))
        .route("/auth/iam-sso/start", post(start_iam_sso_login))
        .route("/auth/iam-sso/complete", post(complete_iam_sso_login))
        .route("/auth/kiro-sso/start", post(start_kiro_sso_login))
        .route("/auth/kiro-sso/poll", post(poll_kiro_sso_login))
        .route("/auth/kiro-sso/cancel", post(cancel_kiro_sso_login))
        .route("/auth/sso-token", post(import_sso_token))
        .route("/proxy-pool", get(list_proxies).post(add_proxy))
        .route("/proxy-pool/{id}", put(update_proxy).delete(delete_proxy))
        .route("/proxy-pool/{id}/enabled", put(set_proxy_enabled))
        .route("/proxy-pool/{id}/check", post(check_proxy))
        .route("/proxy-pool/rebalance", post(rebalance_proxies))
        .route(
            "/proxy-pool/delete-unhealthy",
            post(delete_unhealthy_proxies),
        )
        .route("/proxy-pool/delete-all", post(delete_all_proxies))
        .route(
            "/proxy-pool/disable-high-latency",
            post(disable_high_latency_proxies),
        )
        .route("/proxy-pool/check-ip", post(check_exit_ips))
        .route("/proxy-pool/bindings", get(get_proxy_bindings))
        .route(
            "/config/load-balancing",
            get(get_load_balancing_mode).put(set_load_balancing_mode),
        )
        .route("/config/auth-keys", get(get_auth_keys).put(set_auth_keys))
        .route(
            "/config/cache-simulation",
            get(get_cache_simulation_config).put(set_cache_simulation_config),
        )
        .route(
            "/config/compaction",
            get(get_compaction_config).put(set_compaction_config),
        )
        // API Key 管理
        .route("/server-info", get(get_server_info))
        .route("/api-keys", get(list_api_keys).post(create_api_key))
        .route("/api-keys/usage", get(get_all_usage))
        .route("/api-keys/{id}", put(update_api_key).delete(delete_api_key))
        .route(
            "/api-keys/{id}/usage",
            get(get_key_usage).delete(reset_key_usage),
        )
        // RPM 监控
        .route("/rpm", get(get_rpm))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ))
        .with_state(state)
}
