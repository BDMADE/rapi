use crate::error::{ApiError, ApiResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::IpAddr;

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserRoleDto {
    pub id: String,
    pub role_type: String,
    pub name: String,
    pub description: String,
    pub user_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EndUserDto {
    pub id: String,
    pub document_id: String,
    pub username: String,
    pub email: String,
    pub provider: String,
    pub confirmed: bool,
    pub blocked: bool,
    pub role_id: String,
    pub role_name: String,
    pub role_type: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Strapi v5 `/api/auth/local` response shape: `{ jwt, user }`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EndUserAuthResponse {
    pub jwt: String,
    pub user: EndUserDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookDto {
    pub id: String,
    pub name: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub events: Vec<String>,
    pub has_secret: bool,
    pub is_enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WebhookDeliveryDto {
    pub id: String,
    pub webhook_id: String,
    pub event: String,
    pub status_code: Option<i64>,
    pub success: bool,
    pub attempts: i64,
    pub request: serde_json::Value,
    pub response_body: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmailTemplateDto {
    pub id: String,
    pub code: String,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmailOutboxDto {
    pub id: String,
    pub to_address: String,
    pub from_address: String,
    pub subject: String,
    pub transport: String,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmailSettingsStatusDto {
    pub provider: String,
    pub smtp_configured: bool,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub from_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DataTransferSummaryDto {
    pub schemas_imported: usize,
    pub locales_imported: usize,
    pub documents_imported: usize,
    pub relations_imported: usize,
}

pub const SUPPORTED_WEBHOOK_EVENTS: &[&str] = &[
    "entry.create",
    "entry.update",
    "entry.delete",
    "entry.publish",
    "entry.unpublish",
    "media.create",
    "media.delete",
    "release.publish",
];

/// Validates a webhook target URL and blocks SSRF to loopback/private/link-local/metadata IPs in production.
pub fn validate_webhook_url(raw_url: &str, block_private_hosts: bool) -> ApiResult<()> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() || trimmed.len() > 512 {
        return Err(ApiError::Validation {
            message: "Webhook URL must be 1..512 characters".into(),
            details: serde_json::json!({ "field": "url" }),
        });
    }
    let rest = if let Some(r) = trimmed.strip_prefix("https://") {
        r
    } else if let Some(r) = trimmed.strip_prefix("http://") {
        r
    } else {
        return Err(ApiError::Validation {
            message: "Webhook URL must use http:// or https://".into(),
            details: serde_json::json!({ "field": "url" }),
        });
    };

    let authority = rest.split('/').next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = if host_port.starts_with('[') {
        host_port
            .trim_start_matches('[')
            .split(']')
            .next()
            .unwrap_or("")
    } else {
        host_port.split(':').next().unwrap_or("")
    };

    if host.is_empty() {
        return Err(ApiError::Validation {
            message: "Webhook URL is missing a valid hostname".into(),
            details: serde_json::json!({ "field": "url" }),
        });
    }

    if block_private_hosts {
        let lower = host.to_ascii_lowercase();
        if lower == "localhost"
            || lower.ends_with(".localhost")
            || lower.ends_with(".internal")
            || lower == "metadata.google.internal"
        {
            return Err(ApiError::Validation {
                message: "Webhook URL cannot target internal or localhost hosts in production"
                    .into(),
                details: serde_json::json!({ "field": "url" }),
            });
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            let blocked = match ip {
                IpAddr::V4(v4) => {
                    v4.is_loopback()
                        || v4.is_private()
                        || v4.is_link_local()
                        || v4.is_unspecified()
                        || v4.octets()[0] == 0
                }
                IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
            };
            if blocked {
                return Err(ApiError::Validation {
                    message: "Webhook URL cannot target loopback, private, or metadata IP addresses in production"
                        .into(),
                    details: serde_json::json!({ "field": "url" }),
                });
            }
        }
    }
    Ok(())
}

/// Sanitizes CSV cell strings to prevent spreadsheet formula injection (`=`, `+`, `-`, `@` prefixes).
pub fn sanitize_csv_cell(val: &str) -> String {
    let first = val.chars().next();
    if matches!(first, Some('=' | '+' | '@' | '\t' | '\r'))
        || (first == Some('-') && !val.chars().nth(1).is_some_and(|c| c.is_ascii_digit()))
    {
        format!("'{val}")
    } else {
        val.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_ssrf_guard_and_csv_formula_sanitization_work() {
        validate_webhook_url("https://hooks.example.com/rapi", true).unwrap();
        assert!(validate_webhook_url("http://169.254.169.254/latest/meta-data", true).is_err());
        assert!(validate_webhook_url("http://127.0.0.1:8080/hook", true).is_err());
        assert!(validate_webhook_url("http://localhost:4000/hook", true).is_err());
        validate_webhook_url("http://127.0.0.1:4000/hook", false).unwrap();

        assert_eq!(sanitize_csv_cell("=CMD|' /C calc'!A0"), "'=CMD|' /C calc'!A0");
        assert_eq!(sanitize_csv_cell("-42"), "-42");
        assert_eq!(sanitize_csv_cell("Normal title"), "Normal title");
    }
}
