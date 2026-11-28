use crate::error::{ApiError, ApiResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MediaFormatVariant {
    pub name: String,
    pub hash: String,
    pub ext: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub size: f64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MediaFileDto {
    pub id: i64,
    pub document_id: String,
    pub name: String,
    pub alternative_text: Option<String>,
    pub caption: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub formats: serde_json::Value,
    pub hash: String,
    pub ext: String,
    pub mime: String,
    /// Size in kilobytes (Strapi v5 convention).
    pub size: f64,
    pub url: String,
    pub preview_url: Option<String>,
    pub provider: String,
    pub folder_id: Option<i64>,
    pub folder_path: String,
    pub usage_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MediaFolderDto {
    pub id: i64,
    pub name: String,
    pub path_id: i64,
    pub path: String,
    pub parent_id: Option<i64>,
    pub file_count: i64,
    pub child_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct MediaListFilter {
    /// `Some(None)` = root folder `/`, `Some(Some(id))` = specific folder, `None` = all folders
    pub folder_id: Option<Option<i64>>,
    pub mime_prefix: Option<String>,
    pub search_query: Option<String>,
    pub page: u64,
    pub page_size: u64,
}

#[async_trait]
pub trait StorageProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;
    async fn put(&self, key: &str, bytes: &[u8]) -> ApiResult<String>;
    async fn get(&self, key: &str) -> ApiResult<Vec<u8>>;
    async fn delete(&self, key: &str) -> ApiResult<()>;
}

/// Local filesystem storage provider writing sanitized keys under `upload_dir`.
#[derive(Debug, Clone)]
pub struct LocalDiskProvider {
    root: PathBuf,
}

impl LocalDiskProvider {
    pub fn new(root: impl Into<PathBuf>) -> ApiResult<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root).map_err(|e| {
            ApiError::Internal(format!(
                "Failed to initialize upload directory '{}': {e}",
                root.display()
            ))
        })?;
        Ok(Self { root })
    }

    fn resolve_safe_path(&self, key: &str) -> ApiResult<PathBuf> {
        validate_storage_key(key)?;
        Ok(self.root.join(key))
    }
}

#[async_trait]
impl StorageProvider for LocalDiskProvider {
    fn provider_name(&self) -> &'static str {
        "local"
    }

    async fn put(&self, key: &str, bytes: &[u8]) -> ApiResult<String> {
        let target = self.resolve_safe_path(key)?;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ApiError::Internal(format!("mkdir upload parent failed: {e}")))?;
        }
        tokio::fs::write(&target, bytes)
            .await
            .map_err(|e| ApiError::Internal(format!("write upload file failed: {e}")))?;
        Ok(format!("/uploads/{key}"))
    }

    async fn get(&self, key: &str) -> ApiResult<Vec<u8>> {
        let target = self.resolve_safe_path(key)?;
        tokio::fs::read(&target).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ApiError::NotFound(format!("Uploaded file '{key}' not found"))
            } else {
                ApiError::Internal(format!("read upload file failed: {e}"))
            }
        })
    }

    async fn delete(&self, key: &str) -> ApiResult<()> {
        let target = self.resolve_safe_path(key)?;
        match tokio::fs::remove_file(&target).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(ApiError::Internal(format!("delete upload file failed: {e}"))),
        }
    }
}

/// Ensures storage keys are strictly single-segment or safe relative subpaths with zero traversal.
pub fn validate_storage_key(key: &str) -> ApiResult<()> {
    if key.is_empty() || key.len() > 200 || key.contains('\\') || key.starts_with('/') {
        return Err(ApiError::BadRequest("Invalid media storage key".into()));
    }
    let path = Path::new(key);
    for comp in path.components() {
        match comp {
            Component::Normal(seg) => {
                let s = seg.to_str().unwrap_or("");
                if s.is_empty()
                    || s.starts_with('.')
                    || !s
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
                {
                    return Err(ApiError::BadRequest("Unsafe characters in storage key".into()));
                }
            }
            _ => {
                return Err(ApiError::BadRequest(
                    "Path traversal components are forbidden".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Sanitizes an untrusted client filename into `(stem_slug, lowercase_dot_ext)`.
pub fn sanitize_filename(original: &str) -> (String, String) {
    let file_name = Path::new(original)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("asset");

    let (raw_stem, raw_ext) = match file_name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && !e.is_empty() && e.len() <= 10 => (s, e),
        _ => (file_name, "bin"),
    };

    let stem: String = raw_stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .take(48)
        .collect();
    let clean_stem = stem.trim_matches('_');
    let final_stem = if clean_stem.is_empty() {
        "file".to_string()
    } else {
        clean_stem.to_string()
    };

    let ext: String = raw_ext
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let final_ext = if ext.is_empty() {
        ".bin".to_string()
    } else {
        format!(".{ext}")
    };

    (final_stem, final_ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_key_blocks_traversal_and_sanitizes_filenames() {
        validate_storage_key("ab12cd34_hero.png").unwrap();
        assert!(validate_storage_key("../secret.db").is_err());
        assert!(validate_storage_key("/etc/passwd").is_err());
        assert!(validate_storage_key(".env").is_err());

        let (stem, ext) = sanitize_filename("../../My Hero Banner (Final).PNG");
        assert_eq!(stem, "my_hero_banner__final");
        assert_eq!(ext, ".png");
    }
}
