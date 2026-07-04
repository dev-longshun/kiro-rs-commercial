//! Commercial proxy pool model.
//!
//! The pool stores reusable proxy definitions and health data. Credential
//! binding is persisted on the credential itself as credential-level proxy
//! settings, so this module intentionally has no account-pool scheduling.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::http_client::{ProxyConfig, build_client};
use crate::model::config::TlsBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolEntry {
    pub id: u32,
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub enabled: bool,
    pub healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default)]
    pub consecutive_failures: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyPoolEntryPublic {
    pub id: u32,
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub has_password: bool,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub consecutive_failures: u32,
}

impl From<&ProxyPoolEntry> for ProxyPoolEntryPublic {
    fn from(entry: &ProxyPoolEntry) -> Self {
        Self {
            id: entry.id,
            name: entry.name.clone(),
            url: entry.url.clone(),
            username: entry.username.clone(),
            has_password: entry.password.is_some(),
            tags: entry.tags.clone(),
            enabled: entry.enabled,
            healthy: entry.healthy,
            last_checked_at: entry.last_checked_at.clone(),
            latency_ms: entry.latency_ms,
            exit_ip: entry.exit_ip.clone(),
            last_error: entry.last_error.clone(),
            consecutive_failures: entry.consecutive_failures,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ProxyPoolFile {
    proxies: Vec<ProxyPoolEntry>,
}

pub struct ProxyPoolManager {
    entries: RwLock<Vec<ProxyPoolEntry>>,
    next_id: AtomicU32,
    file_path: PathBuf,
    tls_backend: TlsBackend,
}

impl ProxyPoolManager {
    pub fn load(path: PathBuf, tls_backend: TlsBackend) -> anyhow::Result<Self> {
        let (entries, max_id) = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let file: ProxyPoolFile = serde_json::from_str(&content)?;
            let max_id = file.proxies.iter().map(|entry| entry.id).max().unwrap_or(0);
            (file.proxies, max_id)
        } else {
            (Vec::new(), 0)
        };

        Ok(Self {
            entries: RwLock::new(entries),
            next_id: AtomicU32::new(max_id + 1),
            file_path: path,
            tls_backend,
        })
    }

    fn save(&self) -> anyhow::Result<()> {
        let file = ProxyPoolFile {
            proxies: self.entries.read().clone(),
        };
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.file_path, serde_json::to_string_pretty(&file)?)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<ProxyPoolEntryPublic> {
        self.entries
            .read()
            .iter()
            .map(ProxyPoolEntryPublic::from)
            .collect()
    }

    pub fn get(&self, id: u32) -> Option<ProxyPoolEntry> {
        self.entries
            .read()
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    pub fn add(
        &self,
        name: String,
        url: String,
        username: Option<String>,
        password: Option<String>,
        tags: Vec<String>,
    ) -> anyhow::Result<ProxyPoolEntryPublic> {
        let entry = ProxyPoolEntry {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            name,
            url,
            username,
            password,
            tags,
            enabled: true,
            healthy: true,
            last_checked_at: None,
            latency_ms: None,
            exit_ip: None,
            last_error: None,
            consecutive_failures: 0,
        };

        {
            let mut entries = self.entries.write();
            if entries
                .iter()
                .any(|existing| existing.url == entry.url && existing.username == entry.username)
            {
                anyhow::bail!("代理 URL 已存在: {}", entry.url);
            }
            entries.push(entry.clone());
        }
        self.save()?;
        Ok(ProxyPoolEntryPublic::from(&entry))
    }

    pub fn update(
        &self,
        id: u32,
        name: Option<String>,
        url: Option<String>,
        username: Option<Option<String>>,
        password: Option<Option<String>>,
        tags: Option<Vec<String>>,
    ) -> anyhow::Result<Option<ProxyPoolEntryPublic>> {
        let result = {
            let mut entries = self.entries.write();
            let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
                return Ok(None);
            };
            if let Some(name) = name {
                entry.name = name;
            }
            if let Some(url) = url {
                entry.url = url;
            }
            if let Some(username) = username {
                entry.username = username;
            }
            if let Some(password) = password {
                entry.password = password;
            }
            if let Some(tags) = tags {
                entry.tags = tags;
            }
            entry.clone()
        };
        self.save()?;
        Ok(Some(ProxyPoolEntryPublic::from(&result)))
    }

    pub fn delete(&self, id: u32) -> anyhow::Result<Option<ProxyPoolEntry>> {
        let removed = {
            let mut entries = self.entries.write();
            let Some(index) = entries.iter().position(|entry| entry.id == id) else {
                return Ok(None);
            };
            entries.remove(index)
        };
        self.save()?;
        Ok(Some(removed))
    }

    pub fn delete_unhealthy(&self) -> anyhow::Result<Vec<ProxyPoolEntry>> {
        let removed = {
            let mut entries = self.entries.write();
            let mut removed = Vec::new();
            entries.retain(|entry| {
                let should_remove = entry.enabled && !entry.healthy;
                if should_remove {
                    removed.push(entry.clone());
                }
                !should_remove
            });
            removed
        };
        if !removed.is_empty() {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn delete_all(&self) -> anyhow::Result<Vec<ProxyPoolEntry>> {
        let removed = {
            let mut entries = self.entries.write();
            std::mem::take(&mut *entries)
        };
        if !removed.is_empty() {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn set_enabled(&self, id: u32, enabled: bool) -> anyhow::Result<bool> {
        let updated = {
            let mut entries = self.entries.write();
            let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
                return Ok(false);
            };
            entry.enabled = enabled;
            true
        };
        self.save()?;
        Ok(updated)
    }

    pub fn disable_high_latency(&self, threshold_ms: u64) -> anyhow::Result<usize> {
        let count = {
            let mut entries = self.entries.write();
            let mut count = 0;
            for entry in entries.iter_mut() {
                if entry.enabled && entry.latency_ms.map_or(false, |lat| lat > threshold_ms) {
                    entry.enabled = false;
                    count += 1;
                }
            }
            count
        };
        if count > 0 {
            self.save()?;
        }
        Ok(count)
    }

    pub async fn check_single(&self, id: u32) -> anyhow::Result<Option<ProxyPoolEntryPublic>> {
        let Some(entry) = self.get(id) else {
            return Ok(None);
        };
        let proxy = entry.to_proxy_config();
        let started = std::time::Instant::now();
        let result = Self::probe_proxy(&proxy, self.tls_backend).await;
        let latency_ms = started.elapsed().as_millis() as u64;
        let checked_at = chrono::Utc::now().to_rfc3339();

        let public = {
            let mut entries = self.entries.write();
            let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
                return Ok(None);
            };
            entry.last_checked_at = Some(checked_at);
            entry.latency_ms = Some(latency_ms);
            match result {
                Ok(exit_ip) => {
                    entry.healthy = true;
                    entry.exit_ip = exit_ip;
                    entry.last_error = None;
                    entry.consecutive_failures = 0;
                }
                Err(err) => {
                    entry.healthy = false;
                    entry.last_error = Some(err.to_string());
                    entry.consecutive_failures += 1;
                }
            }
            ProxyPoolEntryPublic::from(&*entry)
        };
        self.save()?;
        Ok(Some(public))
    }

    pub async fn check_exit_ip_for(
        &self,
        entry: Option<&ProxyPoolEntry>,
    ) -> (Option<String>, Option<String>, u64) {
        let started = std::time::Instant::now();
        let proxy = entry.map(ProxyPoolEntry::to_proxy_config);
        match Self::probe_proxy_opt(proxy.as_ref(), self.tls_backend).await {
            Ok(ip) => (ip, None, started.elapsed().as_millis() as u64),
            Err(err) => (
                None,
                Some(err.to_string()),
                started.elapsed().as_millis() as u64,
            ),
        }
    }

    async fn probe_proxy(
        proxy: &ProxyConfig,
        tls_backend: TlsBackend,
    ) -> anyhow::Result<Option<String>> {
        Self::probe_proxy_opt(Some(proxy), tls_backend).await
    }

    async fn probe_proxy_opt(
        proxy: Option<&ProxyConfig>,
        tls_backend: TlsBackend,
    ) -> anyhow::Result<Option<String>> {
        let client = build_client(proxy, 15, tls_backend)?;
        let response = client
            .get("https://api.ipify.org?format=json")
            .send()
            .await?;
        let body = response.text().await.unwrap_or_default();
        let ip = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("ip")
                    .and_then(|ip| ip.as_str())
                    .map(|ip| ip.to_string())
            });
        Ok(ip)
    }
}

impl ProxyPoolEntry {
    pub fn to_proxy_config(&self) -> ProxyConfig {
        let mut proxy = ProxyConfig::new(&self.url);
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            proxy = proxy.with_auth(username, password);
        }
        proxy
    }
}
