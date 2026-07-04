use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration as StdDuration, Instant};

use anyhow::{Context, bail};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD},
};
use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

use crate::http_client::{ProxyConfig, build_client};
use crate::model::config::{Config, TlsBackend};

const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const AWS_START_URL: &str = "https://view.awsapps.com/start";
const AWS_PORTAL_BASE: &str = "https://portal.sso.us-east-1.amazonaws.com";
const KIRO_SIGN_IN_BASE_URL: &str = "https://app.kiro.dev/signin";
const KIRO_REDIRECT_URI: &str = "http://localhost:3128";
const KIRO_REDIRECT_PORT: u16 = 3128;
const KIRO_REDIRECT_FROM: &str = "KiroIDE";
const KIRO_OAUTH_CALLBACK_PATH: &str = "/oauth/callback";
const KIRO_SOCIAL_TOKEN_URL: &str = "https://prod.us-east-1.auth.desktop.kiro.dev/oauth/token";
const KIRO_SSO_LOGIN_TIMEOUT: StdDuration = StdDuration::from_secs(10 * 60);

const CODEWHISPERER_SCOPES: &[&str] = &[
    "codewhisperer:completions",
    "codewhisperer:analysis",
    "codewhisperer:conversations",
    "codewhisperer:transformations",
    "codewhisperer:taskassist",
];

const ALLOWED_EXTERNAL_IDP_SUFFIXES: &[&str] = &[
    ".microsoftonline.com",
    ".microsoftonline.us",
    ".microsoftonline.cn",
];

#[derive(Debug, Clone)]
pub struct FlowCredential {
    pub refresh_token: String,
    pub auth_method: String,
    pub provider: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub token_endpoint: Option<String>,
    pub issuer_url: Option<String>,
    pub scopes: Option<String>,
    pub region: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderIdStart {
    pub session_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
}

#[derive(Debug, Clone)]
pub enum BuilderIdPoll {
    Pending { status: String, interval: u64 },
    Completed(FlowCredential),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IamSsoStart {
    pub session_id: String,
    pub authorize_url: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroSsoStart {
    pub session_id: String,
    pub sign_in_url: String,
    pub interval: u64,
}

#[derive(Debug, Clone)]
pub enum KiroSsoPoll {
    Pending,
    Completed(FlowCredential),
}

#[derive(Debug, Clone)]
pub struct SsoTokenImportResult {
    pub imported: Vec<FlowCredential>,
    pub errors: Vec<String>,
}

#[derive(Clone)]
struct BuilderIdSession {
    client_id: String,
    client_secret: String,
    device_code: String,
    interval: u64,
    expires_at: DateTime<Utc>,
    region: String,
}

#[derive(Clone)]
struct IamSsoSession {
    client_id: String,
    client_secret: String,
    code_verifier: String,
    state: String,
    region: String,
    redirect_uri: String,
    expires_at: DateTime<Utc>,
}

#[derive(Clone)]
struct KiroSsoSession {
    verifier: String,
    state: String,
    region: String,
    expires_at: DateTime<Utc>,
    result: Arc<Mutex<Option<KiroSsoCapture>>>,
    leg2: Arc<Mutex<Option<KiroLeg2>>>,
    shutdown: Arc<Mutex<Vec<oneshot::Sender<()>>>>,
}

#[derive(Debug, Clone)]
struct KiroLeg2 {
    state: String,
    verifier: String,
    token_endpoint: String,
    issuer_url: String,
    client_id: String,
    scopes: String,
    redirect_uri: String,
}

#[derive(Debug, Clone)]
enum KiroSsoCapture {
    Social {
        code: String,
    },
    ExternalIdp {
        code: String,
        token_endpoint: String,
        issuer_url: String,
        client_id: String,
        scopes: String,
        redirect_uri: String,
        code_verifier: String,
    },
    Error(String),
}

static BUILDER_ID_SESSIONS: OnceLock<Mutex<HashMap<String, BuilderIdSession>>> = OnceLock::new();
static IAM_SSO_SESSIONS: OnceLock<Mutex<HashMap<String, IamSsoSession>>> = OnceLock::new();
static KIRO_SSO_SESSIONS: OnceLock<Mutex<HashMap<String, KiroSsoSession>>> = OnceLock::new();

fn builder_id_sessions() -> &'static Mutex<HashMap<String, BuilderIdSession>> {
    BUILDER_ID_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn iam_sso_sessions() -> &'static Mutex<HashMap<String, IamSsoSession>> {
    IAM_SSO_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn kiro_sso_sessions() -> &'static Mutex<HashMap<String, KiroSsoSession>> {
    KIRO_SSO_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn auth_client(
    config: &Config,
    proxy: Option<&ProxyConfig>,
    timeout_secs: u64,
) -> anyhow::Result<Client> {
    build_client(proxy, timeout_secs, config.tls_backend)
}

fn oidc_base(region: &str) -> String {
    let region = if region.trim().is_empty() {
        "us-east-1"
    } else {
        region.trim()
    };
    format!("https://oidc.{}.amazonaws.com", region)
}

fn clean_region(region: Option<String>) -> String {
    region
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .unwrap_or("us-east-1")
        .to_string()
}

fn generate_code_verifier() -> String {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn join_scopes_for_authorize() -> String {
    CODEWHISPERER_SCOPES.join(",")
}

fn extract_email_from_jwt(access_token: &str) -> Option<String> {
    let payload = access_token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| URL_SAFE.decode(payload))
        .or_else(|_| STANDARD.decode(payload))
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    for key in ["email", "preferred_username", "upn", "unique_name"] {
        if let Some(email) = value.get(key).and_then(|v| v.as_str()).map(str::trim) {
            if !email.is_empty() {
                return Some(email.to_string());
            }
        }
    }
    None
}

async fn post_json<T, B>(client: &Client, url: impl AsRef<str>, body: &B) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
    B: Serialize + ?Sized,
{
    let response = client
        .post(url.as_ref())
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(body)
        .send()
        .await?;
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("HTTP {}: {}", status.as_u16(), body_text);
    }
    Ok(serde_json::from_str(&body_text)?)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterClientResponse {
    client_id: String,
    client_secret: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceAuthorizationResponse {
    device_code: String,
    user_code: String,
    verification_uri: Option<String>,
    verification_uri_complete: Option<String>,
    interval: Option<u64>,
    expires_in: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AwsTokenResponse {
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct AwsTokenError {
    error: Option<String>,
}

async fn register_oidc_client(
    client: &Client,
    oidc_base: &str,
    start_url: &str,
    redirect_uri: Option<&str>,
    grant_types: &[&str],
    client_name: &str,
) -> anyhow::Result<RegisterClientResponse> {
    let mut payload = serde_json::json!({
        "clientName": client_name,
        "clientType": "public",
        "scopes": CODEWHISPERER_SCOPES,
        "grantTypes": grant_types,
        "issuerUrl": start_url,
    });
    if let Some(redirect_uri) = redirect_uri {
        payload["redirectUris"] = serde_json::json!([redirect_uri]);
    }
    post_json(client, format!("{}/client/register", oidc_base), &payload).await
}

async fn start_device_authorization(
    client: &Client,
    oidc_base: &str,
    client_id: &str,
    client_secret: &str,
    start_url: &str,
) -> anyhow::Result<DeviceAuthorizationResponse> {
    let payload = serde_json::json!({
        "clientId": client_id,
        "clientSecret": client_secret,
        "startUrl": start_url,
    });
    post_json(
        client,
        format!("{}/device_authorization", oidc_base),
        &payload,
    )
    .await
}

pub async fn start_builder_id_login(
    region: Option<String>,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<BuilderIdStart> {
    cleanup_builder_id_sessions();
    let region = clean_region(region);
    let oidc_base = oidc_base(&region);
    let client = auth_client(config, proxy, 60)?;

    let reg = register_oidc_client(
        &client,
        &oidc_base,
        AWS_START_URL,
        None,
        &[DEVICE_CODE_GRANT, "refresh_token"],
        "Kiro",
    )
    .await?;
    let auth = start_device_authorization(
        &client,
        &oidc_base,
        &reg.client_id,
        &reg.client_secret,
        AWS_START_URL,
    )
    .await?;

    let interval = auth.interval.unwrap_or(5).max(1);
    let expires_at = Utc::now() + Duration::seconds(auth.expires_in.unwrap_or(600));
    let verification_uri = auth
        .verification_uri_complete
        .or(auth.verification_uri)
        .unwrap_or_else(|| "https://device.sso.us-east-1.amazonaws.com/".to_string());
    let session_id = uuid::Uuid::new_v4().to_string();
    builder_id_sessions().lock().insert(
        session_id.clone(),
        BuilderIdSession {
            client_id: reg.client_id,
            client_secret: reg.client_secret,
            device_code: auth.device_code,
            interval,
            expires_at,
            region: region.clone(),
        },
    );

    Ok(BuilderIdStart {
        session_id,
        user_code: auth.user_code,
        verification_uri,
        interval,
    })
}

pub async fn poll_builder_id_login(
    session_id: &str,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<BuilderIdPoll> {
    let session = {
        let sessions = builder_id_sessions().lock();
        sessions
            .get(session_id)
            .cloned()
            .context("session not found or expired")?
    };

    if Utc::now() > session.expires_at {
        builder_id_sessions().lock().remove(session_id);
        bail!("authorization expired");
    }

    let client = auth_client(config, proxy, 60)?;
    let payload = serde_json::json!({
        "clientId": session.client_id,
        "clientSecret": session.client_secret,
        "grantType": DEVICE_CODE_GRANT,
        "deviceCode": session.device_code,
    });
    let response = client
        .post(format!("{}/token", oidc_base(&session.region)))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&payload)
        .send()
        .await?;
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();

    if status.is_success() {
        let token: AwsTokenResponse = serde_json::from_str(&body_text)?;
        builder_id_sessions().lock().remove(session_id);
        return Ok(BuilderIdPoll::Completed(FlowCredential {
            refresh_token: token.refresh_token,
            auth_method: "idc".to_string(),
            provider: Some("BuilderId".to_string()),
            client_id: Some(session.client_id),
            client_secret: Some(session.client_secret),
            token_endpoint: None,
            issuer_url: None,
            scopes: None,
            region: Some(session.region),
            email: extract_email_from_jwt(&token.access_token),
        }));
    }

    if status.as_u16() == 400 {
        let err: AwsTokenError =
            serde_json::from_str(&body_text).unwrap_or(AwsTokenError { error: None });
        match err.error.as_deref().unwrap_or_default() {
            "authorization_pending" => {
                return Ok(BuilderIdPoll::Pending {
                    status: "pending".to_string(),
                    interval: session.interval,
                });
            }
            "slow_down" => {
                let mut sessions = builder_id_sessions().lock();
                if let Some(stored) = sessions.get_mut(session_id) {
                    stored.interval += 5;
                    return Ok(BuilderIdPoll::Pending {
                        status: "slow_down".to_string(),
                        interval: stored.interval,
                    });
                }
            }
            "expired_token" => {
                builder_id_sessions().lock().remove(session_id);
                bail!("device code expired");
            }
            "access_denied" => {
                builder_id_sessions().lock().remove(session_id);
                bail!("user denied authorization");
            }
            other => bail!("authorization error: {}", other),
        }
    }

    bail!("unexpected response: {} {}", status.as_u16(), body_text);
}

pub async fn start_iam_sso_login(
    start_url: String,
    region: Option<String>,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<IamSsoStart> {
    cleanup_iam_sso_sessions();
    let start_url = start_url.trim().to_string();
    if start_url.is_empty() {
        bail!("startUrl is required");
    }

    let region = clean_region(region);
    let oidc_base = oidc_base(&region);
    let redirect_uri = "http://127.0.0.1/oauth/callback";
    let client = auth_client(config, proxy, 60)?;
    let reg = register_oidc_client(
        &client,
        &oidc_base,
        &start_url,
        Some(redirect_uri),
        &["authorization_code", "refresh_token"],
        "Kiro",
    )
    .await?;

    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = uuid::Uuid::new_v4().to_string();
    let authorize_url = Url::parse_with_params(
        &format!("{}/authorize", oidc_base),
        &[
            ("response_type", "code"),
            ("client_id", reg.client_id.as_str()),
            ("redirect_uri", redirect_uri),
            ("scopes", join_scopes_for_authorize().as_str()),
            ("state", state.as_str()),
            ("code_challenge", code_challenge.as_str()),
            ("code_challenge_method", "S256"),
        ],
    )?
    .to_string();

    let session_id = uuid::Uuid::new_v4().to_string();
    iam_sso_sessions().lock().insert(
        session_id.clone(),
        IamSsoSession {
            client_id: reg.client_id,
            client_secret: reg.client_secret,
            code_verifier,
            state,
            region,
            redirect_uri: redirect_uri.to_string(),
            expires_at: Utc::now() + Duration::minutes(10),
        },
    );

    Ok(IamSsoStart {
        session_id,
        authorize_url,
        expires_in: 600,
    })
}

pub async fn complete_iam_sso_login(
    session_id: &str,
    callback_url: &str,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<FlowCredential> {
    let session = {
        let sessions = iam_sso_sessions().lock();
        sessions
            .get(session_id)
            .cloned()
            .context("session not found or expired")?
    };
    if Utc::now() > session.expires_at {
        iam_sso_sessions().lock().remove(session_id);
        bail!("session expired");
    }

    let url = Url::parse(callback_url).context("invalid callback URL")?;
    let code = query_param(&url, "code").unwrap_or_default();
    let state = query_param(&url, "state").unwrap_or_default();
    let error = query_param(&url, "error").unwrap_or_default();
    if !error.is_empty() {
        bail!("authorization failed: {}", error);
    }
    if state != session.state {
        bail!("state mismatch");
    }
    if code.is_empty() {
        bail!("missing authorization code");
    }

    let client = auth_client(config, proxy, 60)?;
    let payload = serde_json::json!({
        "clientId": session.client_id,
        "clientSecret": session.client_secret,
        "grantType": "authorization_code",
        "redirectUri": session.redirect_uri,
        "code": code,
        "codeVerifier": session.code_verifier,
    });
    let token: AwsTokenResponse = post_json(
        &client,
        format!("{}/token", oidc_base(&session.region)),
        &payload,
    )
    .await?;

    iam_sso_sessions().lock().remove(session_id);
    Ok(FlowCredential {
        refresh_token: token.refresh_token,
        auth_method: "idc".to_string(),
        provider: Some("Enterprise".to_string()),
        client_id: Some(session.client_id),
        client_secret: Some(session.client_secret),
        token_endpoint: None,
        issuer_url: None,
        scopes: None,
        region: Some(session.region),
        email: extract_email_from_jwt(&token.access_token),
    })
}

pub async fn import_sso_tokens(
    bearer_tokens: String,
    region: Option<String>,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> SsoTokenImportResult {
    let region = clean_region(region);
    let mut imported = Vec::new();
    let mut errors = Vec::new();

    for token in bearer_tokens
        .lines()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        match import_single_sso_token(token, &region, config, proxy).await {
            Ok(credential) => imported.push(credential),
            Err(err) => errors.push(err.to_string()),
        }
    }

    SsoTokenImportResult { imported, errors }
}

async fn import_single_sso_token(
    bearer_token: &str,
    region: &str,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<FlowCredential> {
    let client = auth_client(config, proxy, 60)?;
    let oidc_base = oidc_base(region);
    let reg = register_oidc_client(
        &client,
        &oidc_base,
        AWS_START_URL,
        None,
        &[DEVICE_CODE_GRANT, "refresh_token"],
        "Kiro API Proxy",
    )
    .await?;
    let auth = start_device_authorization(
        &client,
        &oidc_base,
        &reg.client_id,
        &reg.client_secret,
        AWS_START_URL,
    )
    .await?;

    verify_bearer_token(&client, bearer_token).await?;
    let device_session_token = get_device_session_token(&client, bearer_token).await?;
    if let Some(device_context) =
        accept_user_code(&client, &oidc_base, &auth.user_code, &device_session_token).await?
    {
        approve_auth(&client, &oidc_base, &device_context, &device_session_token).await?;
    }

    let token = poll_for_device_token(
        &client,
        &oidc_base,
        &reg.client_id,
        &reg.client_secret,
        &auth.device_code,
        auth.interval.unwrap_or(1).max(1),
    )
    .await?;

    Ok(FlowCredential {
        refresh_token: token.refresh_token,
        auth_method: "idc".to_string(),
        provider: Some("Enterprise".to_string()),
        client_id: Some(reg.client_id),
        client_secret: Some(reg.client_secret),
        token_endpoint: None,
        issuer_url: None,
        scopes: None,
        region: Some(region.to_string()),
        email: extract_email_from_jwt(&token.access_token),
    })
}

async fn verify_bearer_token(client: &Client, bearer_token: &str) -> anyhow::Result<()> {
    let response = client
        .get(format!("{}/token/whoAmI", AWS_PORTAL_BASE))
        .header("Authorization", format!("Bearer {}", bearer_token))
        .header("Accept", "application/json")
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("Token 验证失败: HTTP {}", response.status().as_u16());
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct DeviceSessionResponse {
    token: String,
}

async fn get_device_session_token(client: &Client, bearer_token: &str) -> anyhow::Result<String> {
    let response: DeviceSessionResponse = post_json_with_auth(
        client,
        format!("{}/session/device", AWS_PORTAL_BASE),
        bearer_token,
        &serde_json::json!({}),
    )
    .await?;
    Ok(response.token)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceContext {
    device_context_id: String,
    client_id: String,
    client_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptUserCodeResponse {
    device_context: Option<DeviceContext>,
}

async fn accept_user_code(
    client: &Client,
    oidc_base: &str,
    user_code: &str,
    device_session_token: &str,
) -> anyhow::Result<Option<DeviceContext>> {
    let payload = serde_json::json!({
        "userCode": user_code,
        "userSessionId": device_session_token,
    });
    let response = client
        .post(format!(
            "{}/device_authorization/accept_user_code",
            oidc_base
        ))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Referer", "https://view.awsapps.com/")
        .json(&payload)
        .send()
        .await?;
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("接受用户代码失败: HTTP {}: {}", status.as_u16(), body_text);
    }
    let data: AcceptUserCodeResponse = serde_json::from_str(&body_text)?;
    Ok(data.device_context)
}

async fn approve_auth(
    client: &Client,
    oidc_base: &str,
    device_context: &DeviceContext,
    device_session_token: &str,
) -> anyhow::Result<()> {
    let payload = serde_json::json!({
        "deviceContext": {
            "deviceContextId": device_context.device_context_id,
            "clientId": device_context.client_id,
            "clientType": device_context.client_type,
        },
        "userSessionId": device_session_token,
    });
    let response = client
        .post(format!(
            "{}/device_authorization/associate_token",
            oidc_base
        ))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("Referer", "https://view.awsapps.com/")
        .json(&payload)
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        bail!("批准授权失败: HTTP {}: {}", status.as_u16(), body_text);
    }
    Ok(())
}

async fn post_json_with_auth<T, B>(
    client: &Client,
    url: impl AsRef<str>,
    bearer_token: &str,
    body: &B,
) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de>,
    B: Serialize + ?Sized,
{
    let response = client
        .post(url.as_ref())
        .header("Authorization", format!("Bearer {}", bearer_token))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(body)
        .send()
        .await?;
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("HTTP {}: {}", status.as_u16(), body_text);
    }
    Ok(serde_json::from_str(&body_text)?)
}

async fn poll_for_device_token(
    client: &Client,
    oidc_base: &str,
    client_id: &str,
    client_secret: &str,
    device_code: &str,
    mut interval: u64,
) -> anyhow::Result<AwsTokenResponse> {
    let deadline = Instant::now() + StdDuration::from_secs(120);
    let payload = serde_json::json!({
        "clientId": client_id,
        "clientSecret": client_secret,
        "grantType": DEVICE_CODE_GRANT,
        "deviceCode": device_code,
    });

    loop {
        if Instant::now() > deadline {
            bail!("授权超时");
        }
        tokio::time::sleep(StdDuration::from_secs(interval.max(1))).await;
        let response = client
            .post(format!("{}/token", oidc_base))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&payload)
            .send()
            .await?;
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        if status.is_success() {
            return Ok(serde_json::from_str(&body_text)?);
        }
        if status.as_u16() == 400 {
            let err: AwsTokenError =
                serde_json::from_str(&body_text).unwrap_or(AwsTokenError { error: None });
            match err.error.as_deref().unwrap_or_default() {
                "authorization_pending" => continue,
                "slow_down" => {
                    interval += 5;
                    continue;
                }
                other => bail!("授权错误: {}", other),
            }
        }
        bail!("获取 Token 失败: HTTP {}: {}", status.as_u16(), body_text);
    }
}

async fn bind_kiro_callback_listeners() -> anyhow::Result<Vec<TcpListener>> {
    if let Ok(bind) = std::env::var("KIRO_SSO_CALLBACK_BIND") {
        let host = bind
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        if !host.is_empty() {
            let listener = TcpListener::bind((host.as_str(), KIRO_REDIRECT_PORT))
                .await
                .with_context(|| {
                    format!(
                        "cannot bind {}:{} for SSO callback",
                        host, KIRO_REDIRECT_PORT
                    )
                })?;
            return Ok(vec![listener]);
        }
    }

    let primary = TcpListener::bind(("127.0.0.1", KIRO_REDIRECT_PORT))
        .await
        .with_context(|| {
            format!(
                "cannot bind 127.0.0.1:{} for SSO callback",
                KIRO_REDIRECT_PORT
            )
        })?;
    let mut listeners = vec![primary];
    match TcpListener::bind(("::1", KIRO_REDIRECT_PORT)).await {
        Ok(listener) => listeners.push(listener),
        Err(err) => tracing::debug!(
            "Kiro SSO secondary callback bind [::1]:{} skipped: {}",
            KIRO_REDIRECT_PORT,
            err
        ),
    }
    Ok(listeners)
}

pub async fn start_kiro_sso_login(
    region: Option<String>,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<KiroSsoStart> {
    cleanup_kiro_sso_sessions();
    let region = clean_region(region);
    let verifier = generate_code_verifier();
    let challenge = generate_code_challenge(&verifier);
    let state = uuid::Uuid::new_v4().to_string();
    let session_id = uuid::Uuid::new_v4().to_string();
    let listeners = bind_kiro_callback_listeners().await?;

    let session = KiroSsoSession {
        verifier: verifier.clone(),
        state: state.clone(),
        region,
        expires_at: Utc::now() + Duration::from_std(KIRO_SSO_LOGIN_TIMEOUT)?,
        result: Arc::new(Mutex::new(None)),
        leg2: Arc::new(Mutex::new(None)),
        shutdown: Arc::new(Mutex::new(Vec::new())),
    };

    let proxy = proxy.cloned();
    let tls_backend = config.tls_backend;
    for listener in listeners {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        session.shutdown.lock().push(shutdown_tx);
        tokio::spawn(run_kiro_sso_listener(
            listener,
            session.clone(),
            shutdown_rx,
            tls_backend,
            proxy.clone(),
        ));
    }

    let sign_in_url = Url::parse_with_params(
        KIRO_SIGN_IN_BASE_URL,
        &[
            ("state", state.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("redirect_uri", KIRO_REDIRECT_URI),
            ("redirect_from", KIRO_REDIRECT_FROM),
        ],
    )?
    .to_string();

    kiro_sso_sessions()
        .lock()
        .insert(session_id.clone(), session);
    let timeout_session_id = session_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(KIRO_SSO_LOGIN_TIMEOUT).await;
        cancel_kiro_sso_login(&timeout_session_id);
    });

    Ok(KiroSsoStart {
        session_id,
        sign_in_url,
        interval: 2,
    })
}

pub async fn poll_kiro_sso_login(
    session_id: &str,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<KiroSsoPoll> {
    let session = {
        let sessions = kiro_sso_sessions().lock();
        sessions
            .get(session_id)
            .cloned()
            .context("session not found or expired")?
    };

    if Utc::now() > session.expires_at {
        cancel_kiro_sso_login(session_id);
        bail!("SSO login timed out after 10 minutes");
    }

    let capture = session.result.lock().take();
    let Some(capture) = capture else {
        return Ok(KiroSsoPoll::Pending);
    };

    cancel_kiro_sso_login(session_id);
    let client = auth_client(config, proxy, 60)?;
    match capture {
        KiroSsoCapture::Error(error) => bail!(error),
        KiroSsoCapture::Social { code } => {
            let (access_token, refresh_token, _expires_in, _profile_arn) =
                exchange_social_code(&client, &code, &session.verifier).await?;
            Ok(KiroSsoPoll::Completed(FlowCredential {
                refresh_token,
                auth_method: "social".to_string(),
                provider: Some("Kiro SSO".to_string()),
                client_id: None,
                client_secret: None,
                token_endpoint: None,
                issuer_url: None,
                scopes: None,
                region: Some(session.region),
                email: extract_email_from_jwt(&access_token),
            }))
        }
        KiroSsoCapture::ExternalIdp {
            code,
            token_endpoint,
            issuer_url,
            client_id,
            scopes,
            redirect_uri,
            code_verifier,
        } => {
            let (access_token, refresh_token, _expires_in) = exchange_external_idp_code(
                &client,
                &token_endpoint,
                &client_id,
                &code,
                &code_verifier,
                &redirect_uri,
                &scopes,
            )
            .await?;
            Ok(KiroSsoPoll::Completed(FlowCredential {
                refresh_token,
                auth_method: "external_idp".to_string(),
                provider: Some("AzureAD".to_string()),
                client_id: Some(client_id),
                client_secret: None,
                token_endpoint: Some(token_endpoint),
                issuer_url: Some(issuer_url),
                scopes: Some(scopes),
                region: Some(session.region),
                email: extract_email_from_jwt(&access_token),
            }))
        }
    }
}

pub fn cancel_kiro_sso_login(session_id: &str) {
    let session = kiro_sso_sessions().lock().remove(session_id);
    if let Some(session) = session {
        for tx in session.shutdown.lock().drain(..) {
            let _ = tx.send(());
        }
    }
}

async fn run_kiro_sso_listener(
    listener: TcpListener,
    session: KiroSsoSession,
    mut shutdown_rx: oneshot::Receiver<()>,
    tls_backend: TlsBackend,
    proxy: Option<ProxyConfig>,
) {
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => break,
            accepted = listener.accept() => {
                match accepted {
                    Ok((socket, _)) => {
                        let session = session.clone();
                        let proxy = proxy.clone();
                        tokio::spawn(async move {
                            if let Err(err) = handle_kiro_callback(socket, session, tls_backend, proxy).await {
                                tracing::debug!("Kiro SSO callback handling failed: {}", err);
                            }
                        });
                    }
                    Err(err) => {
                        tracing::debug!("Kiro SSO callback listener stopped: {}", err);
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_kiro_callback(
    mut socket: TcpStream,
    session: KiroSsoSession,
    tls_backend: TlsBackend,
    proxy: Option<ProxyConfig>,
) -> anyhow::Result<()> {
    let mut buf = Vec::new();
    let mut tmp = [0_u8; 1024];
    loop {
        let n = socket.read(&mut tmp).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 16 * 1024 {
            break;
        }
    }
    let request = String::from_utf8_lossy(&buf);
    let Some(first_line) = request.lines().next() else {
        return Ok(());
    };
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    if method != "GET" {
        write_http_response(&mut socket, 405, "text/plain", "Method not allowed").await?;
        return Ok(());
    }
    let callback_url = parse_callback_target(target)?;
    let path = callback_url.path().to_string();
    let issuer_url = query_param(&callback_url, "issuer_url").unwrap_or_default();
    let login_option = query_param(&callback_url, "login_option").unwrap_or_default();

    if path != KIRO_OAUTH_CALLBACK_PATH
        && (login_option.eq_ignore_ascii_case("external_idp") || !issuer_url.is_empty())
    {
        let already_started = session.leg2.lock().is_some();
        if already_started {
            write_no_content(&mut socket).await?;
            return Ok(());
        }

        let client_id = query_param(&callback_url, "client_id").unwrap_or_default();
        let scopes = query_param(&callback_url, "scopes").unwrap_or_default();
        let login_hint = query_param(&callback_url, "login_hint").unwrap_or_default();
        if client_id.is_empty() {
            deliver_kiro_capture(
                &session,
                KiroSsoCapture::Error("invalid external IdP descriptor".to_string()),
            );
            write_callback_page(&mut socket, false).await?;
            return Ok(());
        }

        let client = build_client(proxy.as_ref(), 60, tls_backend)?;
        let (auth_endpoint, token_endpoint) = match oidc_discover(&client, &issuer_url).await {
            Ok(endpoints) => endpoints,
            Err(err) => {
                deliver_kiro_capture(&session, KiroSsoCapture::Error(err.to_string()));
                write_callback_page(&mut socket, false).await?;
                return Ok(());
            }
        };

        let verifier = generate_code_verifier();
        let state2 = uuid::Uuid::new_v4().to_string();
        let redirect_uri = format!("{}{}", KIRO_REDIRECT_URI, KIRO_OAUTH_CALLBACK_PATH);
        let leg_already_started = {
            let mut leg2 = session.leg2.lock();
            if leg2.is_some() {
                true
            } else {
                *leg2 = Some(KiroLeg2 {
                    state: state2.clone(),
                    verifier: verifier.clone(),
                    token_endpoint: token_endpoint.clone(),
                    issuer_url: issuer_url.clone(),
                    client_id: client_id.clone(),
                    scopes: scopes.clone(),
                    redirect_uri: redirect_uri.clone(),
                });
                false
            }
        };
        if leg_already_started {
            write_no_content(&mut socket).await?;
            return Ok(());
        }
        let auth_url = external_idp_authorize_url(
            &auth_endpoint,
            &client_id,
            &redirect_uri,
            &scopes,
            &generate_code_challenge(&verifier),
            &state2,
            &login_hint,
        )?;
        write_redirect(&mut socket, &auth_url).await?;
        return Ok(());
    }

    if path == KIRO_OAUTH_CALLBACK_PATH {
        let code = query_param(&callback_url, "code").unwrap_or_default();
        let state = query_param(&callback_url, "state").unwrap_or_default();
        let err = query_param(&callback_url, "error").unwrap_or_default();
        let leg2 = session.leg2.lock().clone();
        let Some(leg2) = leg2 else {
            write_no_content(&mut socket).await?;
            return Ok(());
        };
        if state != leg2.state {
            write_no_content(&mut socket).await?;
            return Ok(());
        }
        if !err.is_empty() {
            let desc = query_param(&callback_url, "error_description").unwrap_or_default();
            deliver_kiro_capture(
                &session,
                KiroSsoCapture::Error(format!(
                    "external IdP authorization error: {} {}",
                    err, desc
                )),
            );
            write_callback_page(&mut socket, false).await?;
            return Ok(());
        }
        if code.is_empty() {
            write_no_content(&mut socket).await?;
            return Ok(());
        }
        deliver_kiro_capture(
            &session,
            KiroSsoCapture::ExternalIdp {
                code,
                token_endpoint: leg2.token_endpoint,
                issuer_url: leg2.issuer_url,
                client_id: leg2.client_id,
                scopes: leg2.scopes,
                redirect_uri: leg2.redirect_uri,
                code_verifier: leg2.verifier,
            },
        );
        write_callback_page(&mut socket, true).await?;
        return Ok(());
    }

    let code = query_param(&callback_url, "code").unwrap_or_default();
    let state = query_param(&callback_url, "state").unwrap_or_default();
    let err = query_param(&callback_url, "error").unwrap_or_default();
    if code.is_empty() && err.is_empty() {
        write_no_content(&mut socket).await?;
        return Ok(());
    }
    if state != session.state {
        write_no_content(&mut socket).await?;
        return Ok(());
    }
    if !err.is_empty() {
        let desc = query_param(&callback_url, "error_description").unwrap_or_default();
        deliver_kiro_capture(
            &session,
            KiroSsoCapture::Error(format!("SSO authorization error: {} {}", err, desc)),
        );
        write_callback_page(&mut socket, false).await?;
        return Ok(());
    }
    deliver_kiro_capture(&session, KiroSsoCapture::Social { code });
    write_callback_page(&mut socket, true).await?;
    Ok(())
}

fn deliver_kiro_capture(session: &KiroSsoSession, capture: KiroSsoCapture) {
    let mut result = session.result.lock();
    if result.is_none() {
        *result = Some(capture);
    }
}

fn parse_callback_target(target: &str) -> anyhow::Result<Url> {
    if target.starts_with("http://") || target.starts_with("https://") {
        return Ok(Url::parse(target)?);
    }
    Ok(Url::parse(&format!("http://localhost{}", target))?)
}

fn query_param(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.trim().to_string())
}

async fn write_no_content(socket: &mut TcpStream) -> anyhow::Result<()> {
    socket
        .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .await?;
    Ok(())
}

async fn write_redirect(socket: &mut TcpStream, location: &str) -> anyhow::Result<()> {
    let response = format!(
        "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        location
    );
    socket.write_all(response.as_bytes()).await?;
    Ok(())
}

async fn write_callback_page(socket: &mut TcpStream, ok: bool) -> anyhow::Result<()> {
    let msg = if ok {
        "Kiro sign-in complete. You can close this tab and return to the admin panel."
    } else {
        "Kiro sign-in failed. Return to the admin panel and try again."
    };
    write_http_response(socket, 200, "text/html; charset=utf-8", &format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Kiro Sign-In</title></head><body style=\"font-family:sans-serif;padding:2rem\"><p>{}</p></body></html>",
        msg
    ))
    .await
}

async fn write_http_response(
    socket: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> anyhow::Result<()> {
    let reason = match status {
        200 => "OK",
        405 => "Method Not Allowed",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        reason,
        content_type,
        body.len(),
        body
    );
    socket.write_all(response.as_bytes()).await?;
    Ok(())
}

fn validate_external_idp_endpoint(raw_url: &str) -> anyhow::Result<()> {
    let url = Url::parse(raw_url.trim()).context("invalid external IdP URL")?;
    if url.scheme() != "https" {
        bail!("external IdP URL must be https");
    }
    let host = url
        .host_str()
        .map(|h| h.to_ascii_lowercase())
        .context("external IdP URL has no host")?;
    if host.parse::<IpAddr>().is_ok() {
        bail!("external IdP host must not be an IP literal");
    }
    if ALLOWED_EXTERNAL_IDP_SUFFIXES
        .iter()
        .any(|suffix| host.ends_with(suffix))
    {
        return Ok(());
    }
    bail!("external IdP host {:?} is not allow-listed", host);
}

#[derive(Debug, Deserialize)]
struct OidcDiscoveryDocument {
    authorization_endpoint: String,
    token_endpoint: String,
}

async fn oidc_discover(client: &Client, issuer_url: &str) -> anyhow::Result<(String, String)> {
    validate_external_idp_endpoint(issuer_url)?;
    let doc_url = format!(
        "{}/.well-known/openid-configuration",
        issuer_url.trim().trim_end_matches('/')
    );
    let response = client
        .get(doc_url)
        .header("Accept", "application/json")
        .send()
        .await?;
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("OIDC discovery failed (status {})", status.as_u16());
    }
    let doc: OidcDiscoveryDocument = serde_json::from_str(&body_text)?;
    validate_external_idp_endpoint(&doc.authorization_endpoint)
        .context("discovered authorization_endpoint rejected")?;
    validate_external_idp_endpoint(&doc.token_endpoint)
        .context("discovered token_endpoint rejected")?;
    Ok((doc.authorization_endpoint, doc.token_endpoint))
}

fn external_idp_authorize_url(
    auth_endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &str,
    challenge: &str,
    state: &str,
    login_hint: &str,
) -> anyhow::Result<String> {
    let mut params = vec![
        ("client_id", client_id),
        ("response_type", "code"),
        ("redirect_uri", redirect_uri),
        ("scope", scopes),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("response_mode", "query"),
        ("state", state),
    ];
    if !login_hint.trim().is_empty() {
        params.push(("login_hint", login_hint));
    }
    Ok(Url::parse_with_params(auth_endpoint, &params)?.to_string())
}

#[derive(Debug, Deserialize)]
struct ExternalIdpTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn exchange_external_idp_code(
    client: &Client,
    token_endpoint: &str,
    client_id: &str,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
    scopes: &str,
) -> anyhow::Result<(String, String, i64)> {
    let mut form = vec![
        ("client_id", client_id),
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", code_verifier),
    ];
    if !scopes.trim().is_empty() {
        form.push(("scope", scopes));
    }
    let response = client
        .post(token_endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .await?;
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    let out: ExternalIdpTokenResponse =
        serde_json::from_str(&body_text).unwrap_or(ExternalIdpTokenResponse {
            access_token: None,
            refresh_token: None,
            expires_in: None,
            error: None,
            error_description: None,
        });
    if !status.is_success() {
        bail!(
            "enterprise SSO token exchange failed (status {}): {} {}",
            status.as_u16(),
            out.error.unwrap_or_default(),
            out.error_description.unwrap_or_default()
        );
    }
    let access_token = out
        .access_token
        .context("external IdP response missing access_token")?;
    let refresh_token = out
        .refresh_token
        .context("external IdP response missing refresh_token")?;
    Ok((access_token, refresh_token, out.expires_in.unwrap_or(3600)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SocialTokenResponse {
    access_token: String,
    refresh_token: String,
    profile_arn: Option<String>,
    expires_in: Option<i64>,
}

async fn exchange_social_code(
    client: &Client,
    code: &str,
    code_verifier: &str,
) -> anyhow::Result<(String, String, i64, Option<String>)> {
    let payload = serde_json::json!({
        "code": code.trim(),
        "code_verifier": code_verifier,
        "redirect_uri": KIRO_REDIRECT_URI,
    });
    let response = client
        .post(KIRO_SOCIAL_TOKEN_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&payload)
        .send()
        .await?;
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!(
            "SSO token exchange failed (status {}): {}",
            status.as_u16(),
            body_text
        );
    }
    let out: SocialTokenResponse = serde_json::from_str(&body_text)?;
    Ok((
        out.access_token,
        out.refresh_token,
        out.expires_in.unwrap_or(3600),
        out.profile_arn,
    ))
}

fn cleanup_builder_id_sessions() {
    let now = Utc::now();
    builder_id_sessions()
        .lock()
        .retain(|_, session| session.expires_at > now);
}

fn cleanup_iam_sso_sessions() {
    let now = Utc::now();
    iam_sso_sessions()
        .lock()
        .retain(|_, session| session.expires_at > now);
}

fn cleanup_kiro_sso_sessions() {
    let now = Utc::now();
    let expired: Vec<String> = kiro_sso_sessions()
        .lock()
        .iter()
        .filter_map(|(id, session)| (session.expires_at <= now).then(|| id.clone()))
        .collect();
    for id in expired {
        cancel_kiro_sso_login(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_email_from_jwt_uses_preferred_username() {
        let payload = URL_SAFE_NO_PAD.encode(r#"{"preferred_username":"a@example.com"}"#);
        let token = format!("h.{}.s", payload);
        assert_eq!(
            extract_email_from_jwt(&token).as_deref(),
            Some("a@example.com")
        );
    }

    #[test]
    fn test_external_idp_authorize_url_omits_empty_login_hint() {
        let url = external_idp_authorize_url(
            "https://login.microsoftonline.com/t/oauth2/v2.0/authorize",
            "client",
            "http://localhost:3128/oauth/callback",
            "scope1 scope2",
            "challenge",
            "state",
            "",
        )
        .unwrap();
        assert!(url.contains("client_id=client"));
        assert!(!url.contains("login_hint="));
    }
}
