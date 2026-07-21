//! Admin API 类型定义

use serde::{Deserialize, Serialize};

// ============ 凭据状态 ============

/// 所有凭据状态响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialsStatusResponse {
    /// 凭据总数
    pub total: usize,
    /// 可用凭据数量（未禁用）
    pub available: usize,
    /// 当前活跃凭据 ID
    pub current_id: u64,
    /// 各凭据状态列表
    pub credentials: Vec<CredentialStatusItem>,
}

/// 单个凭据的状态信息
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatusItem {
    /// 凭据唯一 ID
    pub id: u64,
    /// 优先级（数字越小优先级越高）
    pub priority: u32,
    /// 是否被禁用
    pub disabled: bool,
    /// 连续失败次数
    pub failure_count: u32,
    /// 是否为当前活跃凭据
    pub is_current: bool,
    /// Token 过期时间（RFC3339 格式）
    pub expires_at: Option<String>,
    /// 认证方式
    pub auth_method: Option<String>,
    /// 是否有 Profile ARN
    pub has_profile_arn: bool,
    /// refreshToken 的 SHA-256 哈希（用于前端重复检测）
    pub refresh_token_hash: Option<String>,
    /// 用户邮箱（用于前端显示）
    pub email: Option<String>,
    /// API 调用成功次数
    pub success_count: u64,
    /// 最后一次 API 调用时间（RFC3339 格式）
    pub last_used_at: Option<String>,
    /// 是否配置了凭据级代理
    pub has_proxy: bool,
    /// 代理 URL（用于前端展示）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    /// 账号来源分类
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_source: Option<String>,
    /// 账号来源展示标签
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_source_label: Option<String>,
    /// KAM 顶层 idp 原始值
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kam_idp: Option<String>,
    /// KAM credentials.provider 原始值
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kam_provider: Option<String>,
    /// KAM 分组 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kam_group_id: Option<String>,
    /// KAM 分组名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kam_group_name: Option<String>,
    /// KAM / 手动标签
    pub labels: Vec<String>,
    /// 最近一次显式 Token 刷新时间（RFC3339）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_token_refresh_at: Option<String>,
    /// 最近一次存活检测时间（RFC3339）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_liveness_check_at: Option<String>,
}

// ============ 操作请求 ============

/// 启用/禁用凭据请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDisabledRequest {
    /// 是否禁用
    pub disabled: bool,
}

/// 修改优先级请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPriorityRequest {
    /// 新优先级值
    pub priority: u32,
}

/// 添加凭据请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCredentialRequest {
    /// 访问令牌（api_key 模式必填；external_idp KAM 导入时可直接信任未过期 accessToken）
    pub access_token: Option<String>,

    /// 刷新令牌（OAuth 模式必填，API Key 模式不需要）
    pub refresh_token: Option<String>,

    /// 认证方式（可选，默认 social；支持 social / idc / external_idp / api_key）
    #[serde(default = "default_auth_method")]
    pub auth_method: String,

    /// OIDC Client ID（IdC 认证需要）
    pub client_id: Option<String>,

    /// OIDC Client Secret（IdC 认证需要）
    pub client_secret: Option<String>,

    /// External IdP OAuth2 token endpoint（external_idp 认证需要）
    pub token_endpoint: Option<String>,

    /// External IdP OIDC issuer URL（可选，仅记录）
    pub issuer_url: Option<String>,

    /// External IdP OAuth2 scopes（可选）
    pub scopes: Option<String>,

    /// Profile ARN（可选，KAM/备份导入时保留）
    pub profile_arn: Option<String>,

    /// Token 过期时间（RFC3339 字符串；前端会把 KAM 毫秒时间戳转换为该格式）
    pub expires_at: Option<String>,

    /// 优先级（可选，默认 0）
    #[serde(default)]
    pub priority: u32,

    /// 凭据级 Region 配置（用于 OIDC token 刷新）
    /// 未配置时回退到 config.json 的全局 region
    pub region: Option<String>,

    /// 凭据级 Auth Region（用于 Token 刷新）
    pub auth_region: Option<String>,

    /// 凭据级 API Region（用于 API 请求）
    pub api_region: Option<String>,

    /// 凭据级 Machine ID（可选，64 位字符串）
    /// 未配置时回退到 config.json 的 machineId
    pub machine_id: Option<String>,

    /// 用户邮箱（可选，用于前端显示）
    pub email: Option<String>,

    /// 导入文件中已有的订阅标题（可选，用于余额接口不可用时的初始展示）
    pub subscription_title: Option<String>,

    /// 导入文件中已有的当前用量（可选）
    pub current_usage: Option<f64>,

    /// 导入文件中已有的使用限额（可选）
    pub usage_limit: Option<f64>,

    /// 导入文件中已有的下次重置时间（Unix 秒，可选）
    pub next_reset_at: Option<f64>,

    /// 导入文件中已有的超额开关状态（可选）
    pub overage_enabled: Option<bool>,

    /// 导入文件中已有的超额能力判定（可选）
    pub overage_capable: Option<bool>,

    /// 导入文件中已有的超额能力原始字符串（可选）
    pub overage_capability_raw: Option<String>,

    /// 账号来源分类
    pub account_source: Option<String>,

    /// 账号来源展示标签
    pub account_source_label: Option<String>,

    /// KAM 顶层 idp 原始值
    pub kam_idp: Option<String>,

    /// KAM credentials.provider 原始值
    pub kam_provider: Option<String>,

    /// KAM 分组 ID
    pub kam_group_id: Option<String>,

    /// KAM 分组名称
    pub kam_group_name: Option<String>,

    /// KAM / 手动标签
    #[serde(default)]
    pub labels: Vec<String>,

    /// 凭据级代理 URL（可选，特殊值 "direct" 表示不使用代理）
    pub proxy_url: Option<String>,

    /// 凭据级代理认证用户名（可选）
    pub proxy_username: Option<String>,

    /// 凭据级代理认证密码（可选）
    pub proxy_password: Option<String>,
}

fn default_auth_method() -> String {
    "social".to_string()
}

/// 添加凭据成功响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCredentialResponse {
    pub success: bool,
    pub message: String,
    /// 新添加的凭据 ID
    pub credential_id: u64,
    /// 用户邮箱（如果获取成功）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

// ============ 登录/导入流程 ============

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderIdStartRequest {
    pub region: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderIdStartResponse {
    pub session_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSessionPollRequest {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthFlowPollResponse {
    pub success: bool,
    pub completed: bool,
    pub status: Option<String>,
    pub interval: Option<u64>,
    pub account: Option<AddCredentialResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IamSsoStartRequest {
    pub start_url: String,
    pub region: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IamSsoStartResponse {
    pub session_id: String,
    pub authorize_url: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IamSsoCompleteRequest {
    pub session_id: String,
    pub callback_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroSsoStartRequest {
    pub region: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroSsoStartResponse {
    pub session_id: String,
    pub sign_in_url: String,
    pub interval: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroSsoCancelRequest {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsoTokenImportRequest {
    pub bearer_token: String,
    pub region: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SsoTokenImportResponse {
    pub success: bool,
    pub accounts: Vec<AddCredentialResponse>,
    pub errors: Vec<String>,
}

/// 更新凭据请求（所有字段可选，只更新提供的字段）
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCredentialRequest {
    /// 刷新令牌（可选，更新后会重新验证）
    pub refresh_token: Option<String>,

    /// API Key 值（可选，api_key 类型凭据更新时使用）
    pub access_token: Option<String>,

    /// 认证方式（可选）
    pub auth_method: Option<String>,

    /// OIDC Client ID（可选）
    pub client_id: Option<String>,

    /// OIDC Client Secret（可选）
    pub client_secret: Option<String>,

    /// External IdP OAuth2 token endpoint（可选）
    pub token_endpoint: Option<String>,

    /// External IdP OIDC issuer URL（可选）
    pub issuer_url: Option<String>,

    /// External IdP OAuth2 scopes（可选）
    pub scopes: Option<String>,

    /// 凭据级 Auth Region（用于 Token 刷新）
    pub auth_region: Option<String>,

    /// 凭据级 API Region（用于 API 请求）
    pub api_region: Option<String>,

    /// 凭据级 Machine ID（可选）
    pub machine_id: Option<String>,

    /// 用户邮箱（可选）
    pub email: Option<String>,

    /// 账号来源分类
    pub account_source: Option<String>,

    /// 账号来源展示标签
    pub account_source_label: Option<String>,

    /// KAM 顶层 idp 原始值
    pub kam_idp: Option<String>,

    /// KAM credentials.provider 原始值
    pub kam_provider: Option<String>,

    /// KAM 分组 ID
    pub kam_group_id: Option<String>,

    /// KAM 分组名称
    pub kam_group_name: Option<String>,

    /// KAM / 手动标签
    pub labels: Option<Vec<String>>,

    /// 凭据级代理 URL（可选）
    pub proxy_url: Option<String>,

    /// 凭据级代理认证用户名（可选）
    pub proxy_username: Option<String>,

    /// 凭据级代理认证密码（可选）
    pub proxy_password: Option<String>,
}

// ============ 余额查询 ============

/// 余额查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceResponse {
    /// 凭据 ID
    pub id: u64,
    /// 订阅类型
    pub subscription_title: Option<String>,
    /// 当前使用量
    pub current_usage: f64,
    /// 使用限额
    pub usage_limit: f64,
    /// 剩余额度
    pub remaining: f64,
    /// 使用百分比
    pub usage_percentage: f64,
    /// 下次重置时间（Unix 时间戳）
    pub next_reset_at: Option<f64>,
    /// 本条余额数据的查询时间（Unix 秒）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queried_at: Option<f64>,
    /// 用户当前是否开启了超额
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overage_enabled: Option<bool>,
    /// 账号是否能开启超额
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overage_capable: Option<bool>,
    /// 上游 overageCapability 原始字符串
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overage_capability_raw: Option<String>,
}

/// 单账号超额开关请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetOverageRequest {
    pub enabled: bool,
}

/// 批量开启超额请求
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableOverageAllRequest {
    #[serde(default)]
    pub ids: Option<Vec<u64>>,
    #[serde(default)]
    pub all: bool,
}

/// 批量开启超额结果
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableOverageAllResult {
    pub enabled_ids: Vec<u64>,
    pub skipped_ids: Vec<u64>,
    pub failed_ids: Vec<u64>,
    pub failure_messages: Vec<String>,
}

/// 全局余额汇总响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSummaryResponse {
    pub total_remaining: f64,
    pub total_limit: f64,
    pub queried_count: usize,
    pub total_count: usize,
    pub balances: Vec<BalanceResponse>,
    pub last_updated_at: Option<f64>,
}

/// 余额自动刷新设置
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceAutoRefreshSettingsResponse {
    pub enabled: bool,
    pub interval_secs: u64,
    pub running: bool,
    pub last_started_at: Option<f64>,
    pub last_finished_at: Option<f64>,
}

/// 更新余额自动刷新设置
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetBalanceAutoRefreshSettingsRequest {
    pub enabled: Option<bool>,
    pub interval_secs: Option<u64>,
}

/// Compaction 配置响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionConfigResponse {
    pub enabled: bool,
    pub threshold_percent: f64,
    pub preserve_recent_pairs: usize,
    pub tool_result_max_chars: usize,
}

/// 更新 Compaction 配置请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCompactionConfigRequest {
    pub enabled: bool,
    pub threshold_percent: f64,
    pub preserve_recent_pairs: usize,
    pub tool_result_max_chars: usize,
}

/// 存活检测响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivenessCheckResponse {
    pub id: u64,
    pub status: String,
    pub checked_at: String,
    pub latency_ms: Option<u64>,
    pub message: Option<String>,
}

// ============ 负载均衡配置 ============

/// 负载均衡模式响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadBalancingModeResponse {
    /// 当前模式（"priority" 或 "balanced"）
    pub mode: String,
}

/// 设置负载均衡模式请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetLoadBalancingModeRequest {
    /// 模式（"priority" 或 "balanced"）
    pub mode: String,
}

// ============ 通用响应 ============

/// 操作成功响应
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

impl SuccessResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
        }
    }
}

// ============ API Key 管理 ============

/// 创建 API Key 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateApiKeyRequest {
    /// 备注名称（如 "张三-月付"）
    pub name: String,
    /// 过期时间（可选，ISO 8601 格式）— 按日期模式
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 额度限制（美元）— 按额度模式
    #[serde(default)]
    pub spending_limit: Option<f64>,
    /// 有效期天数（懒激活模式）
    #[serde(default)]
    pub duration_days: Option<f64>,
}

/// 更新 API Key 请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateApiKeyRequest {
    /// 备注名称
    #[serde(default)]
    pub name: Option<String>,
    /// 启用状态
    #[serde(default)]
    pub enabled: Option<bool>,
    /// 过期时间（null 表示永不过期）
    #[serde(default, deserialize_with = "deserialize_optional_datetime")]
    pub expires_at: Option<Option<chrono::DateTime<chrono::Utc>>>,
    /// 额度限制（null 表示不限额）
    #[serde(default, deserialize_with = "deserialize_optional_f64")]
    pub spending_limit: Option<Option<f64>>,
    /// 有效期天数（懒激活模式）
    #[serde(default, deserialize_with = "deserialize_optional_f64")]
    pub duration_days: Option<Option<f64>>,
}

/// 区分 JSON 中"字段缺失"与"字段为 null"
/// 缺失 → None（不更新），null → Some(None)（永不过期），有值 → Some(Some(dt))
fn deserialize_optional_datetime<'de, D>(
    deserializer: D,
) -> Result<Option<Option<chrono::DateTime<chrono::Utc>>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::deserialize(deserializer).map(Some)
}

/// 区分 JSON 中"字段缺失"与"字段为 null"（f64 版本）
/// 缺失 → None（不更新），null → Some(None)（不限额），有值 → Some(Some(limit))
fn deserialize_optional_f64<'de, D>(deserializer: D) -> Result<Option<Option<f64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::deserialize(deserializer).map(Some)
}

/// 错误响应
#[derive(Debug, Serialize)]
pub struct AdminErrorResponse {
    pub error: AdminError,
}

#[derive(Debug, Serialize)]
pub struct AdminError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

impl AdminErrorResponse {
    pub fn new(error_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: AdminError {
                error_type: error_type.into(),
                message: message.into(),
            },
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new("invalid_request", message)
    }

    pub fn authentication_error() -> Self {
        Self::new("authentication_error", "Invalid or missing admin API key")
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("not_found", message)
    }

    pub fn api_error(message: impl Into<String>) -> Self {
        Self::new("api_error", message)
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new("internal_error", message)
    }
}

// ============ 代理池管理 ============

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddProxyRequest {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProxyRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub username: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub password: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetProxyEnabledRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetProxyBindingRequest {
    pub proxy_id: Option<u32>,
    #[serde(default)]
    pub direct: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyBindingEntry {
    pub proxy_id: u32,
    pub proxy_name: String,
    pub credentials: Vec<BoundCredentialInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundCredentialInfo {
    pub id: u64,
    pub email: Option<String>,
    pub disabled: bool,
}

fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::deserialize(deserializer).map(Some)
}

// ============ KAM 导出 ============

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KamExportResponse {
    pub schema_version: String,
    pub exported_at: u64,
    pub accounts: Vec<KamExportAccount>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KamExportAccount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idp: Option<String>,
    pub credentials: KamExportCredentials,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_source_label: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KamExportCredentials {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

// ============ 认证密钥管理 ============

/// 认证密钥查询响应（脱敏）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthKeysResponse {
    /// 主 API Key（脱敏显示）
    pub api_key: String,
    /// Admin API Key（脱敏显示）
    pub admin_api_key: String,
}

/// 修改认证密钥请求
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAuthKeysRequest {
    /// 新的主 API Key（可选，不传则不修改）
    #[serde(default)]
    pub api_key: Option<String>,
    /// 新的 Admin API Key（可选，不传则不修改）
    #[serde(default)]
    pub admin_api_key: Option<String>,
}

// ============ 缓存模拟配置 ============

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheSimulationConfigResponse {
    pub enabled: bool,
    pub read_ratio: f64,
    pub creation_ratio: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCacheSimulationConfigRequest {
    pub enabled: bool,
    #[serde(default = "default_read_ratio")]
    pub read_ratio: f64,
    #[serde(default = "default_creation_ratio")]
    pub creation_ratio: f64,
}

fn default_read_ratio() -> f64 {
    0.20
}

fn default_creation_ratio() -> f64 {
    0.10
}
