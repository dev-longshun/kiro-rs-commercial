//! Admin API 业务逻辑服务

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as TokioMutex;

use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::token_manager::MultiTokenManager;
use crate::model::config::Config;

use super::error::AdminServiceError;
use super::types::{
    AddCredentialRequest, AddCredentialResponse, BalanceAutoRefreshSettingsResponse,
    BalanceResponse, BalanceSummaryResponse, CredentialStatusItem, CredentialsStatusResponse,
    EnableOverageAllResult, KamExportAccount, KamExportCredentials, KamExportResponse,
    LivenessCheckResponse, LoadBalancingModeResponse, SetBalanceAutoRefreshSettingsRequest,
    SetLoadBalancingModeRequest, UpdateCredentialRequest,
};

/// 余额缓存过期时间（秒），5 分钟
const BALANCE_CACHE_TTL_SECS: i64 = 300;
const MIN_BALANCE_AUTO_REFRESH_INTERVAL_SECS: u64 = 300;

/// 缓存的余额条目（含时间戳）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedBalance {
    /// 缓存时间（Unix 秒）
    cached_at: f64,
    /// 缓存的余额数据
    data: BalanceResponse,
}

/// Admin 服务
///
/// 封装所有 Admin API 的业务逻辑
pub struct AdminService {
    token_manager: Arc<MultiTokenManager>,
    balance_cache: Mutex<HashMap<u64, CachedBalance>>,
    cache_path: Option<PathBuf>,
    balance_refresh_lock: TokioMutex<()>,
    balance_auto_refresh_enabled: AtomicBool,
    balance_auto_refresh_interval_secs: AtomicU64,
    balance_auto_refresh_running: AtomicBool,
    balance_auto_refresh_started: AtomicBool,
    balance_auto_refresh_last_started_at: Mutex<Option<f64>>,
    balance_auto_refresh_last_finished_at: Mutex<Option<f64>>,
}

impl AdminService {
    pub fn new(token_manager: Arc<MultiTokenManager>) -> Self {
        let cache_path = token_manager
            .cache_dir()
            .map(|d| d.join("kiro_balance_cache.json"));

        let balance_cache = Self::load_balance_cache_from(&cache_path);

        let balance_auto_refresh_enabled = token_manager.config().balance_auto_refresh_enabled;
        let balance_auto_refresh_interval_secs =
            token_manager.config().balance_auto_refresh_interval_secs;

        Self {
            token_manager,
            balance_cache: Mutex::new(balance_cache),
            cache_path,
            balance_refresh_lock: TokioMutex::new(()),
            balance_auto_refresh_enabled: AtomicBool::new(balance_auto_refresh_enabled),
            balance_auto_refresh_interval_secs: AtomicU64::new(balance_auto_refresh_interval_secs),
            balance_auto_refresh_running: AtomicBool::new(false),
            balance_auto_refresh_started: AtomicBool::new(false),
            balance_auto_refresh_last_started_at: Mutex::new(None),
            balance_auto_refresh_last_finished_at: Mutex::new(None),
        }
    }

    pub fn token_manager(&self) -> &MultiTokenManager {
        &self.token_manager
    }

    pub fn config(&self) -> &Config {
        self.token_manager.config()
    }

    pub fn global_proxy(&self) -> Option<crate::http_client::ProxyConfig> {
        self.token_manager.global_proxy()
    }

    pub fn start_balance_auto_refresh(self: &Arc<Self>) {
        if self
            .balance_auto_refresh_started
            .swap(true, Ordering::Relaxed)
        {
            return;
        }

        let service = self.clone();
        tokio::spawn(async move {
            loop {
                let interval_secs = service
                    .balance_auto_refresh_interval_secs
                    .load(Ordering::Relaxed)
                    .max(MIN_BALANCE_AUTO_REFRESH_INTERVAL_SECS);
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
                if service.balance_auto_refresh_enabled.load(Ordering::Relaxed) {
                    if let Err(err) = service.refresh_all_balances().await {
                        tracing::warn!("余额自动刷新失败: {}", err);
                    }
                }
            }
        });
    }

    /// 获取所有凭据状态
    pub fn get_all_credentials(&self) -> CredentialsStatusResponse {
        let snapshot = self.token_manager.snapshot();

        let mut credentials: Vec<CredentialStatusItem> = snapshot
            .entries
            .into_iter()
            .map(|entry| CredentialStatusItem {
                id: entry.id,
                priority: entry.priority,
                disabled: entry.disabled,
                failure_count: entry.failure_count,
                is_current: entry.id == snapshot.current_id,
                expires_at: entry.expires_at,
                auth_method: entry.auth_method,
                has_profile_arn: entry.has_profile_arn,
                refresh_token_hash: entry.refresh_token_hash,
                email: entry.email,
                success_count: entry.success_count,
                last_used_at: entry.last_used_at.clone(),
                has_proxy: entry.has_proxy,
                proxy_url: entry.proxy_url,
                account_source: entry.account_source,
                account_source_label: entry.account_source_label,
                kam_idp: entry.kam_idp,
                kam_provider: entry.kam_provider,
                kam_group_id: entry.kam_group_id,
                kam_group_name: entry.kam_group_name,
                labels: entry.labels,
                last_token_refresh_at: entry.last_token_refresh_at,
                last_liveness_check_at: entry.last_liveness_check_at,
            })
            .collect();

        // 按优先级排序（数字越小优先级越高）
        credentials.sort_by_key(|c| c.priority);

        CredentialsStatusResponse {
            total: snapshot.total,
            available: snapshot.available,
            current_id: snapshot.current_id,
            credentials,
        }
    }

    /// 设置凭据禁用状态
    pub fn set_disabled(&self, id: u64, disabled: bool) -> Result<(), AdminServiceError> {
        // 先获取当前凭据 ID，用于判断是否需要切换
        let snapshot = self.token_manager.snapshot();
        let current_id = snapshot.current_id;

        self.token_manager
            .set_disabled(id, disabled)
            .map_err(|e| self.classify_error(e, id))?;

        // 只有禁用的是当前凭据时才尝试切换到下一个
        if disabled && id == current_id {
            let _ = self.token_manager.switch_to_next();
        }
        Ok(())
    }

    /// 设置凭据优先级
    pub fn set_priority(&self, id: u64, priority: u32) -> Result<(), AdminServiceError> {
        self.token_manager
            .set_priority(id, priority)
            .map_err(|e| self.classify_error(e, id))
    }

    /// 重置失败计数并重新启用
    pub fn reset_and_enable(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .reset_and_enable(id)
            .map_err(|e| self.classify_error(e, id))
    }

    /// 强制刷新指定凭据的 Token。
    pub async fn force_refresh_token(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .force_refresh_token(id)
            .await
            .map_err(|e| self.classify_error(e, id))
    }

    /// 设置凭据 Overage 开关。
    pub async fn set_overage(&self, id: u64, enabled: bool) -> Result<(), AdminServiceError> {
        let status = if enabled { "ENABLED" } else { "DISABLED" };
        self.token_manager
            .set_user_preference_for(id, status)
            .await
            .map_err(|e| self.classify_balance_error(e, id))?;

        {
            let mut cache = self.balance_cache.lock();
            cache.remove(&id);
        }
        self.save_balance_cache();
        Ok(())
    }

    /// 一键开启超额。
    pub async fn enable_overage_for_all_capable(
        &self,
        scope_ids: Option<Vec<u64>>,
    ) -> EnableOverageAllResult {
        let scope: Option<HashSet<u64>> = scope_ids.map(|ids| ids.into_iter().collect());
        let snapshot = self.token_manager.snapshot();
        let cache_snapshot = self.balance_cache.lock().clone();
        let now_ts = Utc::now().timestamp() as f64;

        let mut targets = Vec::new();
        let mut skipped = Vec::new();
        for entry in snapshot.entries.iter() {
            if let Some(scope) = &scope {
                if !scope.contains(&entry.id) {
                    continue;
                }
            }
            if entry.disabled {
                skipped.push(entry.id);
                continue;
            }

            let cached = cache_snapshot
                .get(&entry.id)
                .filter(|c| (now_ts - c.cached_at) < BALANCE_CACHE_TTL_SECS as f64);
            match cached {
                Some(c) if c.data.overage_capable == Some(false) => skipped.push(entry.id),
                Some(c) if c.data.overage_enabled == Some(true) => skipped.push(entry.id),
                _ => targets.push(entry.id),
            }
        }

        let mut enabled_ids = Vec::new();
        let mut failed_ids = Vec::new();
        let mut failure_messages = Vec::new();
        for id in targets {
            match self
                .token_manager
                .set_user_preference_for(id, "ENABLED")
                .await
            {
                Ok(()) => {
                    enabled_ids.push(id);
                    self.balance_cache.lock().remove(&id);
                }
                Err(e) => {
                    failed_ids.push(id);
                    failure_messages.push(e.to_string());
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }

        if !enabled_ids.is_empty() {
            self.save_balance_cache();
        }

        EnableOverageAllResult {
            enabled_ids,
            skipped_ids: skipped,
            failed_ids,
            failure_messages,
        }
    }

    /// 单凭据存活检测。当前使用轻量余额接口验证 token 和上游可达性。
    pub async fn liveness_check(
        &self,
        id: u64,
    ) -> Result<LivenessCheckResponse, AdminServiceError> {
        let started = std::time::Instant::now();
        let checked_at = Utc::now().to_rfc3339();
        match self.token_manager.get_usage_limits_for(id).await {
            Ok(_) => {
                let _ = self.token_manager.mark_liveness_checked(id);
                Ok(LivenessCheckResponse {
                    id,
                    status: "available".to_string(),
                    checked_at,
                    latency_ms: Some(started.elapsed().as_millis() as u64),
                    message: Some("上游余额接口可达".to_string()),
                })
            }
            Err(e) => {
                let msg = e.to_string();
                Err(self.classify_balance_error(anyhow::anyhow!(msg), id))
            }
        }
    }

    /// 获取凭据余额（带缓存）
    pub async fn get_balance(&self, id: u64) -> Result<BalanceResponse, AdminServiceError> {
        // 先查缓存
        {
            let cache = self.balance_cache.lock();
            if let Some(cached) = cache.get(&id) {
                let now = Utc::now().timestamp() as f64;
                if (now - cached.cached_at) < BALANCE_CACHE_TTL_SECS as f64 {
                    tracing::debug!("凭据 #{} 余额命中缓存", id);
                    return Ok(Self::balance_from_cached(cached));
                }
            }
        }

        // 缓存未命中或已过期，从上游获取
        let balance = self.fetch_balance(id).await?;
        let cached_balance = Self::cache_balance(balance);
        let response = Self::balance_from_cached(&cached_balance);

        // 更新缓存
        {
            let mut cache = self.balance_cache.lock();
            cache.insert(id, cached_balance);
        }
        self.save_balance_cache();

        Ok(response)
    }

    /// 从上游获取余额（无缓存）
    async fn fetch_balance(&self, id: u64) -> Result<BalanceResponse, AdminServiceError> {
        let usage = self
            .token_manager
            .get_usage_limits_for(id)
            .await
            .map_err(|e| self.classify_balance_error(e, id))?;

        let current_usage = usage.current_usage();
        let usage_limit = usage.usage_limit();
        let remaining = (usage_limit - current_usage).max(0.0);
        let usage_percentage = if usage_limit > 0.0 {
            (current_usage / usage_limit * 100.0).min(100.0)
        } else {
            0.0
        };

        Ok(BalanceResponse {
            id,
            subscription_title: usage.subscription_title().map(|s| s.to_string()),
            current_usage,
            usage_limit,
            remaining,
            usage_percentage,
            next_reset_at: usage.next_date_reset,
            queried_at: None,
            overage_enabled: usage.overage_enabled(),
            overage_capable: usage.overage_capable(),
            overage_capability_raw: usage
                .subscription_info
                .as_ref()
                .and_then(|s| s.overage_capability.clone()),
        })
    }

    fn cache_balance(mut balance: BalanceResponse) -> CachedBalance {
        let cached_at = Utc::now().timestamp() as f64;
        balance.queried_at = Some(cached_at);
        CachedBalance {
            cached_at,
            data: balance,
        }
    }

    fn balance_from_cached(cached: &CachedBalance) -> BalanceResponse {
        let mut balance = cached.data.clone();
        if balance.queried_at.is_none() {
            balance.queried_at = Some(cached.cached_at);
        }
        balance
    }

    /// 获取全局余额汇总（仅读缓存，只统计当前仍存在的凭据）。
    pub fn get_balance_summary(&self) -> BalanceSummaryResponse {
        let snapshot = self.token_manager.snapshot();
        let total_count = snapshot.entries.len();
        let cache = self.balance_cache.lock();

        let mut balances = Vec::new();
        let mut total_remaining = 0.0;
        let mut total_limit = 0.0;
        let mut last_updated_at: Option<f64> = None;

        for entry in &snapshot.entries {
            if let Some(cached) = cache.get(&entry.id) {
                let balance = Self::balance_from_cached(cached);
                total_remaining += balance.remaining;
                total_limit += balance.usage_limit;
                last_updated_at = Some(match last_updated_at {
                    Some(prev) => prev.max(cached.cached_at),
                    None => cached.cached_at,
                });
                balances.push(balance);
            }
        }

        BalanceSummaryResponse {
            total_remaining,
            total_limit,
            queried_count: balances.len(),
            total_count,
            balances,
            last_updated_at,
        }
    }

    /// 刷新所有凭据的余额（逐个查询，失败不影响整体）。
    pub async fn refresh_all_balances(&self) -> Result<BalanceSummaryResponse, AdminServiceError> {
        let _guard = self.balance_refresh_lock.lock().await;
        self.balance_auto_refresh_running
            .store(true, Ordering::Relaxed);
        *self.balance_auto_refresh_last_started_at.lock() = Some(Utc::now().timestamp() as f64);

        let snapshot = self.token_manager.snapshot();
        let target_ids: Vec<u64> = snapshot
            .entries
            .iter()
            .filter(|entry| !entry.disabled)
            .map(|entry| entry.id)
            .collect();

        for (index, id) in target_ids.iter().enumerate() {
            match self.fetch_balance(*id).await {
                Ok(balance) => {
                    let cached_balance = Self::cache_balance(balance);
                    let mut cache = self.balance_cache.lock();
                    cache.insert(*id, cached_balance);
                }
                Err(e) => tracing::warn!("凭据 #{} 余额刷新失败: {}", id, e),
            }
            if index + 1 < target_ids.len() {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }

        self.save_balance_cache();
        self.balance_auto_refresh_running
            .store(false, Ordering::Relaxed);
        *self.balance_auto_refresh_last_finished_at.lock() = Some(Utc::now().timestamp() as f64);
        Ok(self.get_balance_summary())
    }

    pub fn get_balance_auto_refresh_settings(&self) -> BalanceAutoRefreshSettingsResponse {
        BalanceAutoRefreshSettingsResponse {
            enabled: self.balance_auto_refresh_enabled.load(Ordering::Relaxed),
            interval_secs: self
                .balance_auto_refresh_interval_secs
                .load(Ordering::Relaxed),
            running: self.balance_auto_refresh_running.load(Ordering::Relaxed),
            last_started_at: *self.balance_auto_refresh_last_started_at.lock(),
            last_finished_at: *self.balance_auto_refresh_last_finished_at.lock(),
        }
    }

    pub fn set_balance_auto_refresh_settings(
        &self,
        req: SetBalanceAutoRefreshSettingsRequest,
    ) -> Result<BalanceAutoRefreshSettingsResponse, AdminServiceError> {
        let enabled = req
            .enabled
            .unwrap_or_else(|| self.balance_auto_refresh_enabled.load(Ordering::Relaxed));
        let interval_secs = req
            .interval_secs
            .unwrap_or_else(|| {
                self.balance_auto_refresh_interval_secs
                    .load(Ordering::Relaxed)
            })
            .max(MIN_BALANCE_AUTO_REFRESH_INTERVAL_SECS);

        self.balance_auto_refresh_enabled
            .store(enabled, Ordering::Relaxed);
        self.balance_auto_refresh_interval_secs
            .store(interval_secs, Ordering::Relaxed);

        if let Err(err) = self.persist_balance_auto_refresh_settings(enabled, interval_secs) {
            tracing::warn!("余额自动刷新配置持久化失败，仅当前进程生效: {}", err);
        }

        Ok(self.get_balance_auto_refresh_settings())
    }

    fn persist_balance_auto_refresh_settings(
        &self,
        enabled: bool,
        interval_secs: u64,
    ) -> anyhow::Result<()> {
        let config_path = match self.token_manager.config().config_path() {
            Some(path) => path.to_path_buf(),
            None => return Ok(()),
        };

        let mut config = Config::load(&config_path)?;
        config.balance_auto_refresh_enabled = enabled;
        config.balance_auto_refresh_interval_secs = interval_secs;
        config.save()?;
        Ok(())
    }

    /// 添加新凭据
    pub async fn add_credential(
        &self,
        req: AddCredentialRequest,
    ) -> Result<AddCredentialResponse, AdminServiceError> {
        // 构建凭据对象
        let email = req
            .email
            .clone()
            .or_else(|| Some(Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()));
        let new_cred = KiroCredentials {
            id: None,
            access_token: req.access_token,
            refresh_token: Some(req.refresh_token),
            profile_arn: req.profile_arn,
            expires_at: req.expires_at,
            auth_method: Some(req.auth_method),
            client_id: req.client_id,
            client_secret: req.client_secret,
            token_endpoint: req.token_endpoint,
            issuer_url: req.issuer_url,
            scopes: req.scopes,
            priority: req.priority,
            region: req.region,
            auth_region: req.auth_region,
            api_region: req.api_region,
            machine_id: req.machine_id,
            email: email.clone(),
            subscription_title: None, // 将在首次获取使用额度时自动更新
            account_source: req.account_source,
            account_source_label: req.account_source_label,
            kam_idp: req.kam_idp,
            kam_provider: req.kam_provider,
            kam_group_id: req.kam_group_id,
            kam_group_name: req.kam_group_name,
            labels: req.labels,
            last_token_refresh_at: None,
            last_liveness_check_at: None,
            proxy_url: req.proxy_url,
            proxy_username: req.proxy_username,
            proxy_password: req.proxy_password,
            disabled: false, // 新添加的凭据默认启用
        };

        // 调用 token_manager 添加凭据
        let credential_id = self
            .token_manager
            .add_credential(new_cred)
            .await
            .map_err(|e| self.classify_add_error(e))?;

        // 后台获取订阅等级，避免首次请求时 Free 账号绕过 Opus 模型过滤
        let tm = self.token_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = tm.get_usage_limits_for(credential_id).await {
                tracing::warn!("添加凭据后获取订阅等级失败（不影响凭据添加）: {}", e);
            }
        });

        Ok(AddCredentialResponse {
            success: true,
            message: format!("凭据添加成功，ID: {}", credential_id),
            credential_id,
            email,
        })
    }

    /// 删除凭据
    pub fn delete_credential(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .delete_credential(id)
            .map_err(|e| self.classify_delete_error(e, id))?;

        // 清理已删除凭据的余额缓存
        {
            let mut cache = self.balance_cache.lock();
            cache.remove(&id);
        }
        self.save_balance_cache();

        Ok(())
    }

    pub fn export_kam(&self, enabled_only: bool, ids: Option<&[u64]>) -> KamExportResponse {
        let credentials = self.token_manager.export_credentials();
        let snapshot = self.token_manager.snapshot();

        let accounts = credentials
            .iter()
            .filter_map(|cred| {
                let cred_id = cred.id.unwrap_or(0);
                if let Some(id_list) = ids {
                    if !id_list.contains(&cred_id) {
                        return None;
                    }
                }

                let disabled = snapshot
                    .entries
                    .iter()
                    .find(|entry| entry.id == cred_id)
                    .map(|entry| entry.disabled)
                    .unwrap_or(false);
                if enabled_only && disabled {
                    return None;
                }

                Some(KamExportAccount {
                    email: cred.email.clone(),
                    idp: cred.kam_idp.clone(),
                    credentials: KamExportCredentials {
                        refresh_token: cred.refresh_token.clone(),
                        client_id: cred.client_id.clone(),
                        client_secret: cred.client_secret.clone(),
                        token_endpoint: cred.token_endpoint.clone(),
                        issuer_url: cred.issuer_url.clone(),
                        scopes: cred.scopes.clone(),
                        region: cred.region.clone(),
                        auth_method: cred.auth_method.clone(),
                        provider: cred.kam_provider.clone(),
                    },
                    machine_id: cred.machine_id.clone(),
                    group_id: cred.kam_group_id.clone(),
                    group_name: cred.kam_group_name.clone(),
                    labels: cred.labels.clone(),
                    account_source: cred.account_source.clone(),
                    account_source_label: cred.account_source_label.clone(),
                    status: if disabled {
                        "disabled".to_string()
                    } else {
                        "active".to_string()
                    },
                })
            })
            .collect();

        KamExportResponse {
            schema_version: "1.0.0".to_string(),
            exported_at: Utc::now().timestamp_millis() as u64,
            accounts,
        }
    }

    pub fn bind_credential_proxy(
        &self,
        id: u64,
        proxy_url: Option<String>,
        username: Option<String>,
        password: Option<String>,
        direct: bool,
    ) -> Result<(), AdminServiceError> {
        self.token_manager
            .set_proxy_for_credential(id, proxy_url, username, password, direct)
            .map_err(|e| self.classify_error(e, id))
    }

    pub fn clear_proxy_url_bindings(&self, proxy_url: &str) -> Result<usize, AdminServiceError> {
        self.token_manager
            .clear_proxy_url_bindings(proxy_url)
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))
    }

    /// 更新凭据配置
    pub async fn update_credential(
        &self,
        id: u64,
        req: UpdateCredentialRequest,
    ) -> Result<(), AdminServiceError> {
        self.token_manager
            .update_credential(id, req)
            .await
            .map_err(|e| self.classify_update_error(e, id))?;

        // 清理该凭据的余额缓存（配置变更后需要重新获取）
        {
            let mut cache = self.balance_cache.lock();
            cache.remove(&id);
        }
        self.save_balance_cache();

        Ok(())
    }

    /// 获取负载均衡模式
    pub fn get_load_balancing_mode(&self) -> LoadBalancingModeResponse {
        LoadBalancingModeResponse {
            mode: self.token_manager.get_load_balancing_mode(),
        }
    }

    /// 设置负载均衡模式
    pub fn set_load_balancing_mode(
        &self,
        req: SetLoadBalancingModeRequest,
    ) -> Result<LoadBalancingModeResponse, AdminServiceError> {
        // 验证模式值
        if req.mode != "priority" && req.mode != "balanced" {
            return Err(AdminServiceError::InvalidCredential(
                "mode 必须是 'priority' 或 'balanced'".to_string(),
            ));
        }

        self.token_manager
            .set_load_balancing_mode(req.mode.clone())
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        Ok(LoadBalancingModeResponse { mode: req.mode })
    }

    // ============ 余额缓存持久化 ============

    fn load_balance_cache_from(cache_path: &Option<PathBuf>) -> HashMap<u64, CachedBalance> {
        let path = match cache_path {
            Some(p) => p,
            None => return HashMap::new(),
        };

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        // 文件中使用字符串 key 以兼容 JSON 格式
        let map: HashMap<String, CachedBalance> = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("解析余额缓存失败，将忽略: {}", e);
                return HashMap::new();
            }
        };

        let now = Utc::now().timestamp() as f64;
        map.into_iter()
            .filter_map(|(k, v)| {
                let id = k.parse::<u64>().ok()?;
                // 丢弃超过 TTL 的条目
                if (now - v.cached_at) < BALANCE_CACHE_TTL_SECS as f64 {
                    Some((id, v))
                } else {
                    None
                }
            })
            .collect()
    }

    fn save_balance_cache(&self) {
        let path = match &self.cache_path {
            Some(p) => p,
            None => return,
        };

        // 持有锁期间完成序列化和写入，防止并发损坏
        let cache = self.balance_cache.lock();
        let map: HashMap<String, &CachedBalance> =
            cache.iter().map(|(k, v)| (k.to_string(), v)).collect();

        match serde_json::to_string_pretty(&map) {
            Ok(json) => {
                if let Err(e) = std::fs::write(path, json) {
                    tracing::warn!("保存余额缓存失败: {}", e);
                }
            }
            Err(e) => tracing::warn!("序列化余额缓存失败: {}", e),
        }
    }

    // ============ 错误分类 ============

    /// 分类简单操作错误（set_disabled, set_priority, reset_and_enable）
    fn classify_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();
        if msg.contains("不存在") {
            AdminServiceError::NotFound { id }
        } else {
            AdminServiceError::InternalError(msg)
        }
    }

    /// 分类余额查询错误（可能涉及上游 API 调用）
    fn classify_balance_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();

        // 1. 凭据不存在
        if msg.contains("不存在") {
            return AdminServiceError::NotFound { id };
        }

        // 2. 上游服务错误特征：HTTP 响应错误或网络错误
        let is_upstream_error =
            // HTTP 响应错误（来自 refresh_*_token 的错误消息）
            msg.contains("凭证已过期或无效") ||
            msg.contains("权限不足") ||
            msg.contains("已被限流") ||
            msg.contains("服务器错误") ||
            msg.contains("Token 刷新失败") ||
            msg.contains("暂时不可用") ||
            // 网络错误（reqwest 错误）
            msg.contains("error trying to connect") ||
            msg.contains("connection") ||
            msg.contains("timeout") ||
            msg.contains("timed out");

        if is_upstream_error {
            AdminServiceError::UpstreamError(msg)
        } else {
            // 3. 默认归类为内部错误（本地验证失败、配置错误等）
            // 包括：缺少 refreshToken、refreshToken 已被截断、无法生成 machineId 等
            AdminServiceError::InternalError(msg)
        }
    }

    /// 分类添加凭据错误
    fn classify_add_error(&self, e: anyhow::Error) -> AdminServiceError {
        let msg = e.to_string();

        // 凭据验证失败（refreshToken 无效、格式错误等）
        let is_invalid_credential = msg.contains("缺少 refreshToken")
            || msg.contains("refreshToken 为空")
            || msg.contains("refreshToken 已被截断")
            || msg.contains("凭据已存在")
            || msg.contains("refreshToken 重复")
            || msg.contains("凭证已过期或无效")
            || msg.contains("权限不足")
            || msg.contains("已被限流");

        if is_invalid_credential {
            AdminServiceError::InvalidCredential(msg)
        } else if msg.contains("error trying to connect")
            || msg.contains("connection")
            || msg.contains("timeout")
        {
            AdminServiceError::UpstreamError(msg)
        } else {
            AdminServiceError::InternalError(msg)
        }
    }

    /// 分类删除凭据错误
    fn classify_delete_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();
        if msg.contains("不存在") {
            AdminServiceError::NotFound { id }
        } else if msg.contains("只能删除已禁用的凭据") || msg.contains("请先禁用凭据")
        {
            AdminServiceError::InvalidCredential(msg)
        } else {
            AdminServiceError::InternalError(msg)
        }
    }

    /// 分类更新凭据错误
    fn classify_update_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();
        if msg.contains("不存在") {
            AdminServiceError::NotFound { id }
        } else if msg.contains("凭证已过期或无效")
            || msg.contains("权限不足")
            || msg.contains("已被限流")
            || msg.contains("error trying to connect")
            || msg.contains("timeout")
        {
            AdminServiceError::UpstreamError(msg)
        } else {
            AdminServiceError::InvalidCredential(msg)
        }
    }
}
