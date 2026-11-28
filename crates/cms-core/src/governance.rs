use crate::error::{ApiError, ApiResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocaleDto {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLocaleStateDto {
    pub locale: String,
    pub has_draft: bool,
    pub has_published: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewConfigDto {
    pub content_type_uid: String,
    pub url_template: String,
    pub is_enabled: bool,
    pub open_mode: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPreviewDto {
    pub url: String,
    pub open_mode: String,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseEntryDto {
    pub id: i64,
    pub release_id: String,
    pub content_type_uid: String,
    pub document_id: String,
    pub locale: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDto {
    pub id: String,
    pub name: String,
    pub scheduled_at: Option<String>,
    pub timezone: String,
    pub status: String,
    pub released_at: Option<String>,
    pub error_message: Option<String>,
    pub entry_count: i64,
    pub entries: Vec<ReleaseEntryDto>,
    pub created_by: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStageDto {
    pub id: String,
    pub name: String,
    pub color: String,
    pub position: i64,
    pub allowed_role_ids: Vec<String>,
    pub require_for_publish: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowEntryDto {
    pub document_id: String,
    pub locale: String,
    pub stage_id: String,
    pub stage_name: String,
    pub stage_color: String,
    pub assignee_id: Option<String>,
    pub assignee_email: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogDto {
    pub id: String,
    pub action: String,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub target_uid: Option<String>,
    pub target_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub payload: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct AuditLogFilter {
    pub action: Option<String>,
    pub actor_id: Option<String>,
    pub target_uid: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub page: u64,
    pub page_size: u64,
}

/// Validates BCP-47 style locale tags (`en`, `fr`, `bn`, `en-US`, `zh-Hans`).
pub fn validate_locale_code(code: &str) -> ApiResult<()> {
    let trimmed = code.trim();
    if trimmed.len() < 2 || trimmed.len() > 16 {
        return Err(ApiError::Validation {
            message: "Locale code must be between 2 and 16 characters (e.g. 'en', 'fr', 'en-US')"
                .into(),
            details: serde_json::json!({ "field": "code" }),
        });
    }
    let parts: Vec<&str> = trimmed.split('-').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(ApiError::Validation {
            message: "Invalid BCP-47 locale format".into(),
            details: serde_json::json!({ "field": "code" }),
        });
    }
    let primary = parts[0];
    if primary.len() < 2 || primary.len() > 3 || !primary.chars().all(|c| c.is_ascii_lowercase()) {
        return Err(ApiError::Validation {
            message: "Primary language subtag must be 2-3 lowercase ASCII letters (e.g. 'en', 'fr')"
                .into(),
            details: serde_json::json!({ "field": "code" }),
        });
    }
    for sub in &parts[1..] {
        if sub.len() < 2 || sub.len() > 8 || !sub.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(ApiError::Validation {
                message: "Locale region/script subtags must be 2-8 ASCII alphanumeric characters"
                    .into(),
                details: serde_json::json!({ "field": "code" }),
            });
        }
    }
    Ok(())
}

/// Validates preview URL templates to block `javascript:`, `data:`, and `vbscript:` injection.
pub fn validate_preview_url_template(tpl: &str) -> ApiResult<()> {
    let t = tpl.trim();
    if t.is_empty() || t.len() > 512 {
        return Err(ApiError::Validation {
            message: "Preview URL template must be 1..512 characters".into(),
            details: serde_json::json!({ "field": "urlTemplate" }),
        });
    }
    if !(t.starts_with("http://") || t.starts_with("https://") || t.starts_with('/')) {
        return Err(ApiError::Validation {
            message: "Preview URL template must start with http://, https://, or /".into(),
            details: serde_json::json!({ "field": "urlTemplate" }),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_bcp47_locales_and_safe_preview_templates() {
        validate_locale_code("en").unwrap();
        validate_locale_code("en-US").unwrap();
        validate_locale_code("zh-Hans").unwrap();
        assert!(validate_locale_code("EN").is_err());
        assert!(validate_locale_code("../etc").is_err());
        assert!(validate_locale_code("a").is_err());

        validate_preview_url_template("https://example.com/preview/{documentId}?locale={locale}")
            .unwrap();
        validate_preview_url_template("/admin/preview/render/{uid}/{documentId}").unwrap();
        assert!(validate_preview_url_template("javascript:alert(1)").is_err());
        assert!(validate_preview_url_template("data:text/html,<h1>x</h1>").is_err());
    }
}
