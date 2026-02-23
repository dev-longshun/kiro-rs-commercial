use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// 单个 API Key
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKey {
    pub id: u32,
    pub key: String,
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

fn default_enabled() -> bool {
    true
}

impl ApiKey {
    /// 生成新的 API Key
    pub fn new(id: u32, name: String, expires_at: Option<DateTime<Utc>>) -> Self {
        Self {
            id,
            key: generate_api_key(),
            name,
            enabled: true,
            created_at: Utc::now(),
            expires_at,
        }
    }

    /// 检查 key 是否有效（启用且未过期）
    pub fn is_valid(&self) -> bool {
        if !self.enabled {
            return false;
        }
        if let Some(expires_at) = self.expires_at {
            return Utc::now() < expires_at;
        }
        true
    }

    /// 检查是否已过期
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .map(|exp| Utc::now() >= exp)
            .unwrap_or(false)
    }
}
/// 生成 sk- 前缀的随机 API Key
fn generate_api_key() -> String {
    let id = uuid::Uuid::new_v4();
    format!("sk-{}", id.simple())
}

/// API Key 认证结果
pub enum ApiKeyAuthResult {
    /// 认证通过，携带 key ID 和名称
    Valid { id: u32, name: String },
    /// Key 已被禁用
    Disabled,
    /// Key 已过期
    Expired,
    /// Key 不存在
    NotFound,
}

/// API Key 管理器（线程安全）
pub struct ApiKeyManager {
    keys: RwLock<Vec<ApiKey>>,
    file_path: PathBuf,
}

impl ApiKeyManager {
    /// 从文件加载，文件不存在则创建空列表
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let keys = if path.exists() {
            let content = fs::read_to_string(&path)?;
            if content.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&content)?
            }
        } else {
            Vec::new()
        };
        Ok(Self {
            keys: RwLock::new(keys),
            file_path: path,
        })
    }

    /// 持久化到文件
    fn save(&self) -> anyhow::Result<()> {
        let keys = self.keys.read();
        let content = serde_json::to_string_pretty(&*keys)?;
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.file_path, content)?;
        Ok(())
    }

    /// 验证请求中的 key
    pub fn authenticate(&self, key: &str) -> ApiKeyAuthResult {
        let keys = self.keys.read();
        match keys.iter().find(|k| k.key == key) {
            Some(api_key) => {
                if !api_key.enabled {
                    ApiKeyAuthResult::Disabled
                } else if api_key.is_expired() {
                    ApiKeyAuthResult::Expired
                } else {
                    ApiKeyAuthResult::Valid {
                        id: api_key.id,
                        name: api_key.name.clone(),
                    }
                }
            }
            None => ApiKeyAuthResult::NotFound,
        }
    }
    /// 获取所有 key（克隆）
    pub fn list(&self) -> Vec<ApiKey> {
        self.keys.read().clone()
    }

    /// 创建新 key
    pub fn create(&self, name: String, expires_at: Option<DateTime<Utc>>) -> anyhow::Result<ApiKey> {
        let mut keys = self.keys.write();
        let next_id = keys.iter().map(|k| k.id).max().unwrap_or(0) + 1;
        let api_key = ApiKey::new(next_id, name, expires_at);
        keys.push(api_key.clone());
        drop(keys);
        self.save()?;
        Ok(api_key)
    }

    /// 更新 key（name, enabled, expires_at）
    pub fn update(
        &self,
        id: u32,
        name: Option<String>,
        enabled: Option<bool>,
        expires_at: Option<Option<DateTime<Utc>>>,
    ) -> anyhow::Result<Option<ApiKey>> {
        let mut keys = self.keys.write();
        let Some(api_key) = keys.iter_mut().find(|k| k.id == id) else {
            return Ok(None);
        };
        if let Some(name) = name {
            api_key.name = name;
        }
        if let Some(enabled) = enabled {
            api_key.enabled = enabled;
        }
        if let Some(expires_at) = expires_at {
            api_key.expires_at = expires_at;
        }
        let updated = api_key.clone();
        drop(keys);
        self.save()?;
        Ok(Some(updated))
    }

    /// 删除 key
    pub fn delete(&self, id: u32) -> anyhow::Result<bool> {
        let mut keys = self.keys.write();
        let len_before = keys.len();
        keys.retain(|k| k.id != id);
        let deleted = keys.len() < len_before;
        drop(keys);
        if deleted {
            self.save()?;
        }
        Ok(deleted)
    }

    /// 获取文件路径
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }
// APPEND_MARKER2
}