use crate::auth::{
    hash_password, random_hex_32, sha256_hex, validate_email, validate_password, verify_password,
    AuthService,
};
use crate::content::ContentService;
use crate::governance::{CreateLocaleInput, GovernanceService};
use crate::schema::SchemaService;
use chrono::{Duration as ChronoDuration, Utc};
use cms_core::{
    sanitize_csv_cell, validate_webhook_url, ApiError, ApiResult, AppConfig, ContentKind,
    ContentTypeSchema, DataTransferSummaryDto, DocumentQuery, DocumentStatus, EmailOutboxDto,
    EmailSettingsStatusDto, EmailTemplateDto, EndUserAuthResponse, EndUserDto, FieldDefinition,
    FieldType, FieldValidation, JobMessage, PopulateSpec, QueueBackend, UserRoleDto,
    WebhookDeliveryDto, WebhookDto, SUPPORTED_WEBHOOK_EVENTS,
};
use cms_db::{
    IntegrationsRepository, RecordEmailOutboxInput, RecordWebhookDeliveryInput,
};
use hmac::{Hmac, Mac};
use lettre::message::{header::ContentType, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

type HmacSha256 = Hmac<Sha256>;

// ============================================================================
// Input DTOs
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndUserRegisterInput {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndUserLoginInput {
    pub identifier: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgotPasswordInput {
    pub email: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetPasswordInput {
    pub code: String,
    pub password: String,
    pub password_confirmation: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminCreateEndUserInput {
    pub username: String,
    pub email: String,
    pub password: String,
    #[serde(default = "default_auth_role")]
    pub role_id: String,
    #[serde(default = "default_true")]
    pub confirmed: bool,
    #[serde(default)]
    pub blocked: bool,
}

fn default_auth_role() -> String {
    "urole_authenticated".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUpdateEndUserInput {
    pub blocked: Option<bool>,
    pub confirmed: Option<bool>,
    pub role_id: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWebhookInput {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub events: Vec<String>,
    pub secret: Option<String>,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWebhookInput {
    pub name: Option<String>,
    pub url: Option<String>,
    pub headers: Option<BTreeMap<String, String>>,
    pub events: Option<Vec<String>>,
    pub secret: Option<Option<String>>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertEmailTemplateInput {
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTestEmailInput {
    pub to: String,
    #[serde(default)]
    pub subject: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformBundleDto {
    pub version: String,
    pub exported_at: String,
    pub locales: Vec<cms_core::LocaleDto>,
    pub schemas: Vec<ContentTypeSchema>,
    pub documents: Vec<ExportedDocumentItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedDocumentItem {
    pub content_type_uid: String,
    pub document_id: String,
    pub locale: String,
    pub status: String,
    pub data: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone)]
pub struct IntegrationsService {
    repo: IntegrationsRepository,
    config: Arc<AppConfig>,
    auth: AuthService,
    schema: SchemaService,
    content: ContentService,
    governance: GovernanceService,
    queue: Arc<dyn QueueBackend>,
    http_client: reqwest::Client,
}

impl IntegrationsService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: IntegrationsRepository,
        config: Arc<AppConfig>,
        auth: AuthService,
        schema: SchemaService,
        content: ContentService,
        governance: GovernanceService,
        queue: Arc<dyn QueueBackend>,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none()) // Prevent HTTP redirect SSRF bypass!
            .user_agent("RAPI-Webhook/0.1")
            .build()
            .unwrap_or_default();

        Self {
            repo,
            config,
            auth,
            schema,
            content,
            governance,
            queue,
            http_client,
        }
    }

    // =========================================================================
    // 1. End-User Auth (`/api/auth/local/*`) & Admin User-Permissions Management
    // =========================================================================

    pub async fn register_end_user(
        &self,
        input: EndUserRegisterInput,
    ) -> ApiResult<EndUserAuthResponse> {
        validate_username(&input.username)?;
        validate_email(&input.email)?;
        validate_password(&input.password)?;

        let pw_hash = hash_password(&input.password)?;
        let user = self
            .repo
            .create_end_user(
                input.username.trim(),
                input.email.trim(),
                &pw_hash,
                "urole_authenticated",
                true,
                false,
            )
            .await?;

        let jwt = self.auth.issue_end_user_jwt(&user.id, &user.email)?;

        // Enqueue welcome email asynchronously via `rapi:jobs:email`
        let _ = self
            .queue
            .enqueue(JobMessage::new(
                "rapi:jobs:email",
                serde_json::json!({
                    "templateCode": "welcome",
                    "to": user.email,
                    "vars": {
                        "username": user.username,
                        "email": user.email,
                    }
                }),
            ))
            .await;

        Ok(EndUserAuthResponse { jwt, user })
    }

    pub async fn login_end_user(
        &self,
        input: EndUserLoginInput,
    ) -> ApiResult<EndUserAuthResponse> {
        let Some(rec) = self
            .repo
            .find_end_user_by_identifier(&input.identifier)
            .await?
        else {
            return Err(ApiError::BadRequest("Invalid identifier or password".into()));
        };

        if !verify_password(&input.password, &rec.password_hash)? {
            return Err(ApiError::BadRequest("Invalid identifier or password".into()));
        }
        if rec.blocked {
            return Err(ApiError::Forbidden(
                "Your account has been blocked by an administrator".into(),
            ));
        }
        if !rec.confirmed {
            return Err(ApiError::Forbidden(
                "Your account email address is not confirmed".into(),
            ));
        }

        let dto = rec.into_dto();
        let jwt = self.auth.issue_end_user_jwt(&dto.id, &dto.email)?;
        Ok(EndUserAuthResponse { jwt, user: dto })
    }

    /// Generates a single-use SHA-256 hashed reset token (1h TTL) and sends the `reset_password` email.
    /// Always succeeds silently for unknown/blocked emails to prevent account enumeration.
    pub async fn forgot_end_user_password(&self, input: ForgotPasswordInput) -> ApiResult<()> {
        validate_email(&input.email)?;
        let Some(rec) = self.repo.find_end_user_by_identifier(&input.email).await? else {
            return Ok(());
        };
        if rec.blocked {
            return Ok(());
        }

        let raw_code = random_hex_32();
        let token_hash = sha256_hex(&raw_code);
        let expires_at = (Utc::now() + ChronoDuration::hours(1))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        self.repo
            .store_end_user_reset_token(&rec.id, &token_hash, &expires_at)
            .await?;

        let mut vars = BTreeMap::new();
        vars.insert("username".to_string(), rec.username);
        vars.insert("email".to_string(), rec.email.clone());
        vars.insert("code".to_string(), raw_code);

        self.send_templated_email("reset_password", &rec.email, &vars)
            .await?;
        Ok(())
    }

    pub async fn reset_end_user_password(
        &self,
        input: ResetPasswordInput,
    ) -> ApiResult<EndUserAuthResponse> {
        if input.code.trim().is_empty() {
            return Err(ApiError::Validation {
                message: "Reset code is required".into(),
                details: serde_json::json!({ "field": "code" }),
            });
        }
        if input.password != input.password_confirmation {
            return Err(ApiError::Validation {
                message: "Passwords do not match".into(),
                details: serde_json::json!({ "field": "passwordConfirmation" }),
            });
        }
        validate_password(&input.password)?;

        let code_hash = sha256_hex(input.code.trim());
        let now_iso = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let new_pw_hash = hash_password(&input.password)?;

        let user = self
            .repo
            .consume_end_user_reset_token(&code_hash, &now_iso, &new_pw_hash)
            .await?;
        let jwt = self.auth.issue_end_user_jwt(&user.id, &user.email)?;
        Ok(EndUserAuthResponse { jwt, user })
    }

    pub async fn get_end_user_me(&self, user_id: &str) -> ApiResult<EndUserDto> {
        self.repo
            .find_end_user_by_id(user_id)
            .await?
            .map(|r| r.into_dto())
            .ok_or_else(|| ApiError::NotFound("End user not found".into()))
    }

    pub async fn list_end_users(&self) -> ApiResult<Vec<EndUserDto>> {
        self.repo.list_end_users().await
    }

    pub async fn create_end_user_admin(
        &self,
        input: AdminCreateEndUserInput,
    ) -> ApiResult<EndUserDto> {
        validate_username(&input.username)?;
        validate_email(&input.email)?;
        validate_password(&input.password)?;
        if !self.repo.user_role_exists(&input.role_id).await? {
            return Err(ApiError::BadRequest(format!(
                "User role '{}' does not exist",
                input.role_id
            )));
        }
        let pw_hash = hash_password(&input.password)?;
        self.repo
            .create_end_user(
                input.username.trim(),
                input.email.trim(),
                &pw_hash,
                &input.role_id,
                input.confirmed,
                input.blocked,
            )
            .await
    }

    pub async fn update_end_user_admin(
        &self,
        id: &str,
        input: AdminUpdateEndUserInput,
    ) -> ApiResult<EndUserDto> {
        let pw_hash = if let Some(pw) = input.password.as_deref().filter(|s| !s.is_empty()) {
            validate_password(pw)?;
            Some(hash_password(pw)?)
        } else {
            None
        };
        self.repo
            .update_end_user(
                id,
                input.blocked,
                input.confirmed,
                input.role_id.as_deref(),
                pw_hash.as_deref(),
            )
            .await
    }

    pub async fn delete_end_user_admin(&self, id: &str) -> ApiResult<()> {
        self.repo.delete_end_user(id).await
    }

    pub async fn list_user_roles(&self) -> ApiResult<Vec<UserRoleDto>> {
        self.repo.list_user_roles().await
    }

    // =========================================================================
    // 2. Webhooks (HMAC-SHA256 Signing, SSRF Guard & Retry Worker)
    // =========================================================================

    pub async fn list_webhooks(&self) -> ApiResult<Vec<WebhookDto>> {
        self.repo.list_webhooks().await
    }

    pub async fn create_webhook(&self, input: CreateWebhookInput) -> ApiResult<WebhookDto> {
        let name = input.name.trim();
        if name.is_empty() || name.len() > 100 {
            return Err(ApiError::Validation {
                message: "Webhook name must be 1..100 characters".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }
        validate_webhook_url(&input.url, self.config.is_production())?;
        validate_webhook_events(&input.events)?;

        let clean_secret = input
            .secret
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        self.repo
            .create_webhook(
                name,
                input.url.trim(),
                &input.headers,
                &input.events,
                clean_secret,
                input.is_enabled,
            )
            .await
    }

    pub async fn update_webhook(
        &self,
        id: &str,
        input: UpdateWebhookInput,
    ) -> ApiResult<WebhookDto> {
        if let Some(url) = &input.url {
            validate_webhook_url(url, self.config.is_production())?;
        }
        if let Some(events) = &input.events {
            validate_webhook_events(events)?;
        }
        let secret_change = input.secret.as_ref().map(|opt| {
            opt.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        });

        self.repo
            .update_webhook(
                id,
                input.name.as_deref(),
                input.url.as_deref(),
                input.headers.as_ref(),
                input.events.as_deref(),
                secret_change,
                input.is_enabled,
            )
            .await
    }

    pub async fn delete_webhook(&self, id: &str) -> ApiResult<()> {
        self.repo.delete_webhook(id).await
    }

    pub async fn list_webhook_deliveries(&self, id: &str) -> ApiResult<Vec<WebhookDeliveryDto>> {
        self.repo.list_webhook_deliveries(id).await
    }

    /// Enqueues async delivery jobs for all enabled webhooks subscribed to `event`.
    pub async fn emit_webhook_event(
        &self,
        event: &str,
        model: Option<&str>,
        entry: serde_json::Value,
    ) {
        let Ok(matching) = self.repo.list_enabled_webhooks_for_event(event).await else {
            return;
        };
        if matching.is_empty() {
            return;
        }
        let created_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        for wh in matching {
            let payload = serde_json::json!({
                "webhookId": wh.dto.id,
                "event": event,
                "body": {
                    "event": event,
                    "createdAt": created_at,
                    "model": model,
                    "entry": entry,
                }
            });
            let _ = self
                .queue
                .enqueue(JobMessage::new("rapi:jobs:webhook", payload))
                .await;
        }
    }

    /// Synchronously executes a test delivery against `webhook_id` so the admin sees immediate feedback.
    pub async fn trigger_test_webhook(&self, webhook_id: &str) -> ApiResult<WebhookDeliveryDto> {
        let body = serde_json::json!({
            "event": "trigger-test",
            "createdAt": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "model": null,
            "entry": { "ping": "rapi-webhook-test" }
        });
        let (delivery, _) = self
            .execute_webhook_delivery(webhook_id, "trigger-test", &body, 1)
            .await?;
        Ok(delivery)
    }

    /// Background worker entrypoint executing a queued webhook delivery (returning `Err` on failure to trigger backoff retry).
    pub async fn process_webhook_job(&self, job: &JobMessage) -> ApiResult<()> {
        let Some(wh_id) = job.payload.get("webhookId").and_then(|v| v.as_str()) else {
            return Ok(());
        };
        let event = job
            .payload
            .get("event")
            .and_then(|v| v.as_str())
            .unwrap_or("entry.update");
        let body = job
            .payload
            .get("body")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        let attempt = (job.attempts as i64).max(1);
        let (delivery, err_opt) = self
            .execute_webhook_delivery(wh_id, event, &body, attempt)
            .await?;

        if !delivery.success {
            return Err(ApiError::Internal(
                err_opt.unwrap_or_else(|| format!("Webhook returned HTTP {:?}", delivery.status_code)),
            ));
        }
        Ok(())
    }

    async fn execute_webhook_delivery(
        &self,
        webhook_id: &str,
        event: &str,
        body_val: &serde_json::Value,
        attempt: i64,
    ) -> ApiResult<(WebhookDeliveryDto, Option<String>)> {
        let wh = self
            .repo
            .get_webhook_record(webhook_id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Webhook '{webhook_id}' not found")))?;

        validate_webhook_url(&wh.dto.url, self.config.is_production())?;
        if self.config.is_production() {
            verify_webhook_dns_not_private(&wh.dto.url).await?;
        }

        let raw_body = serde_json::to_string(body_val).unwrap_or_else(|_| "{}".to_string());
        let mut req = self
            .http_client
            .post(&wh.dto.url)
            .header("Content-Type", "application/json")
            .header("X-Strapi-Event", event)
            .header("X-Rapi-Event", event);

        for (hk, hv) in &wh.dto.headers {
            let clean_k = hk.trim();
            if !clean_k.is_empty() && !is_reserved_webhook_header(clean_k) {
                req = req.header(clean_k, hv.trim());
            }
        }

        if let Some(secret) = wh.secret.as_deref() {
            let sig = compute_hmac_sha256_hex(secret, raw_body.as_bytes())?;
            req = req.header("X-Rapi-Signature", format!("sha256={sig}"));
        }

        let started = Instant::now();
        let send_res = req.body(raw_body.clone()).send().await;
        let duration_ms = started.elapsed().as_millis() as i64;

        let (status_code, success, resp_snippet, err_reason) = match send_res {
            Ok(resp) => {
                let code = resp.status().as_u16() as i64;
                let ok = resp.status().is_success();
                let txt = resp.text().await.unwrap_or_default();
                let truncated: String = txt.chars().take(1024).collect();
                (
                    Some(code),
                    ok,
                    Some(truncated),
                    if ok {
                        None
                    } else {
                        Some(format!("HTTP {code}"))
                    },
                )
            }
            Err(e) => {
                let msg = e.to_string();
                (None, false, Some(msg.clone()), Some(msg))
            }
        };

        let delivery = self
            .repo
            .record_webhook_delivery(RecordWebhookDeliveryInput {
                webhook_id: webhook_id.to_string(),
                event: event.to_string(),
                status_code,
                success,
                attempts: attempt,
                request_json: raw_body,
                response_body: resp_snippet,
                duration_ms: Some(duration_ms),
            })
            .await?;

        Ok((delivery, err_reason))
    }

    // =========================================================================
    // 3. Email Service (`lettre` SMTP + Templates + Outbox)
    // =========================================================================

    pub fn get_email_settings_status(&self) -> EmailSettingsStatusDto {
        let smtp_configured = self.config.smtp_host.is_some();
        EmailSettingsStatusDto {
            provider: if smtp_configured {
                "lettre-smtp".to_string()
            } else {
                "local-outbox".to_string()
            },
            smtp_configured,
            smtp_host: self.config.smtp_host.clone(),
            smtp_port: self.config.smtp_port,
            from_address: self.config.smtp_from.clone(),
        }
    }

    pub async fn list_email_templates(&self) -> ApiResult<Vec<EmailTemplateDto>> {
        self.repo.list_email_templates().await
    }

    pub async fn upsert_email_template(
        &self,
        code: &str,
        input: UpsertEmailTemplateInput,
    ) -> ApiResult<EmailTemplateDto> {
        if code.trim().is_empty() || input.subject.trim().is_empty() {
            return Err(ApiError::Validation {
                message: "Template code and subject are required".into(),
                details: serde_json::json!({}),
            });
        }
        self.repo
            .upsert_email_template(
                code.trim(),
                input.subject.trim(),
                &input.html_body,
                &input.text_body,
            )
            .await
    }

    pub async fn list_email_outbox(&self) -> ApiResult<Vec<EmailOutboxDto>> {
        self.repo.list_email_outbox().await
    }

    pub async fn send_test_email(&self, input: SendTestEmailInput) -> ApiResult<EmailOutboxDto> {
        validate_email(&input.to)?;
        let subject = input
            .subject
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("RAPI Test Email");
        self.deliver_email(
            input.to.trim(),
            subject,
            "<h2>RAPI Email Test</h2><p>Your email delivery pipeline is working properly.</p>",
            "RAPI Email Test: Your email delivery pipeline is working properly.",
        )
        .await
    }

    pub async fn send_templated_email(
        &self,
        code: &str,
        to: &str,
        vars: &BTreeMap<String, String>,
    ) -> ApiResult<EmailOutboxDto> {
        let tpl = self
            .repo
            .get_email_template_by_code(code)
            .await?
            .unwrap_or(EmailTemplateDto {
                id: format!("tpl_{code}"),
                code: code.to_string(),
                subject: format!("Notification ({code})"),
                html_body: "<p>{{ code }}</p>".to_string(),
                text_body: "{{ code }}".to_string(),
                updated_at: String::new(),
            });

        let subject = render_template_vars(&tpl.subject, vars);
        let html = render_template_vars(&tpl.html_body, vars);
        let text = render_template_vars(&tpl.text_body, vars);

        self.deliver_email(to, &subject, &html, &text).await
    }

    pub async fn process_email_job(&self, job: &JobMessage) -> ApiResult<()> {
        let Some(to) = job.payload.get("to").and_then(|v| v.as_str()) else {
            return Ok(());
        };
        let tpl_code = job
            .payload
            .get("templateCode")
            .and_then(|v| v.as_str())
            .unwrap_or("welcome");

        let mut vars = BTreeMap::new();
        if let Some(obj) = job.payload.get("vars").and_then(|v| v.as_object()) {
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    vars.insert(k.clone(), s.to_string());
                }
            }
        }
        let _ = self.send_templated_email(tpl_code, to, &vars).await?;
        Ok(())
    }

    async fn deliver_email(
        &self,
        to: &str,
        subject: &str,
        html_body: &str,
        text_body: &str,
    ) -> ApiResult<EmailOutboxDto> {
        let from_mbox: Mailbox = self
            .config
            .smtp_from
            .parse()
            .map_err(|e| ApiError::Validation {
                message: format!("Invalid SMTP_FROM address: {e}"),
                details: serde_json::json!({}),
            })?;
        let to_mbox: Mailbox = to.parse().map_err(|e| ApiError::Validation {
            message: format!("Invalid recipient email address '{to}': {e}"),
            details: serde_json::json!({ "field": "to" }),
        })?;

        let email_msg = Message::builder()
            .from(from_mbox)
            .to(to_mbox)
            .subject(subject)
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(text_body.to_string()),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html_body.to_string()),
                    ),
            )
            .map_err(|e| ApiError::Internal(format!("Build email message failed: {e}")))?;

        if let Some(smtp_host) = &self.config.smtp_host {
            let mut builder = AsyncSmtpTransport::<Tokio1Executor>::relay(smtp_host)
                .map_err(|e| ApiError::Internal(format!("SMTP relay init failed: {e}")))?
                .port(self.config.smtp_port);

            if let (Some(u), Some(p)) = (&self.config.smtp_username, &self.config.smtp_password) {
                builder = builder.credentials(Credentials::new(u.clone(), p.clone()));
            }
            let mailer = builder.build();
            match mailer.send(email_msg).await {
                Ok(_) => {
                    self.repo
                        .record_email_outbox(RecordEmailOutboxInput {
                            to_address: to.to_string(),
                            from_address: self.config.smtp_from.clone(),
                            subject: subject.to_string(),
                            html_body: html_body.to_string(),
                            text_body: text_body.to_string(),
                            transport: "smtp".to_string(),
                            status: "sent".to_string(),
                            error_message: None,
                        })
                        .await
                }
                Err(err) => {
                    let msg = err.to_string();
                    let _ = self
                        .repo
                        .record_email_outbox(RecordEmailOutboxInput {
                            to_address: to.to_string(),
                            from_address: self.config.smtp_from.clone(),
                            subject: subject.to_string(),
                            html_body: html_body.to_string(),
                            text_body: text_body.to_string(),
                            transport: "smtp".to_string(),
                            status: "failed".to_string(),
                            error_message: Some(msg.clone()),
                        })
                        .await;
                    Err(ApiError::Internal(format!("SMTP send failed: {msg}")))
                }
            }
        } else {
            // Zero-config local mode: persist constructed MIME message to SQLite `email_outbox`
            self.repo
                .record_email_outbox(RecordEmailOutboxInput {
                    to_address: to.to_string(),
                    from_address: self.config.smtp_from.clone(),
                    subject: subject.to_string(),
                    html_body: html_body.to_string(),
                    text_body: text_body.to_string(),
                    transport: "local".to_string(),
                    status: "sent".to_string(),
                    error_message: None,
                })
                .await
        }
    }

    // =========================================================================
    // 4. Data Management / Transfer (JSON Bundle, CSV, Strapi v5 Importer)
    // =========================================================================

    pub async fn export_json_bundle(&self) -> ApiResult<PlatformBundleDto> {
        let locales = self.governance.list_locales().await?;
        let schemas = self.schema.list_schemas().await?;
        let mut documents = Vec::new();

        for schema in schemas.iter().filter(|s| s.kind != ContentKind::Component) {
            let target_locales: Vec<String> = if schema.i18n_enabled {
                locales.iter().map(|l| l.code.clone()).collect()
            } else {
                vec![self.governance.default_locale().await?]
            };

            let statuses = if schema.draft_and_publish {
                vec![DocumentStatus::Draft, DocumentStatus::Published]
            } else {
                vec![DocumentStatus::Published]
            };

            for loc in &target_locales {
                for st in &statuses {
                    let query = DocumentQuery {
                        locale: Some(loc.clone()),
                        status: Some(*st),
                        page: 1,
                        page_size: 10_000,
                        populate: PopulateSpec::None,
                        ..Default::default()
                    };
                    if let Ok((rows, _)) = self.content.find_many_admin_raw(schema, &query).await {
                        for r in rows {
                            if let Some(mut obj) = r.as_object().cloned() {
                                let doc_id = obj
                                    .remove("documentId")
                                    .and_then(|v| v.as_str().map(String::from))
                                    .unwrap_or_default();
                                obj.remove("id");
                                obj.remove("locale");
                                obj.remove("publishedAt");
                                obj.remove("createdAt");
                                obj.remove("updatedAt");
                                if !doc_id.is_empty() {
                                    documents.push(ExportedDocumentItem {
                                        content_type_uid: schema.uid.clone(),
                                        document_id: doc_id,
                                        locale: loc.clone(),
                                        status: st.as_str().to_string(),
                                        data: obj,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(PlatformBundleDto {
            version: "rapi-bundle-v1".to_string(),
            exported_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            locales,
            schemas,
            documents,
        })
    }

    pub async fn import_json_bundle(
        &self,
        bundle: PlatformBundleDto,
        actor_id: Option<&str>,
    ) -> ApiResult<DataTransferSummaryDto> {
        let mut locales_imported = 0;
        let existing_locales = self.governance.list_locales().await?;
        for loc in bundle.locales {
            if !existing_locales.iter().any(|l| l.code == loc.code) {
                let _ = self
                    .governance
                    .create_locale(CreateLocaleInput {
                        code: loc.code,
                        name: loc.name,
                        is_default: false,
                    })
                    .await;
                locales_imported += 1;
            }
        }

        // Import components before collection/single types so component references validate cleanly
        let mut ordered_schemas = bundle.schemas;
        ordered_schemas.sort_by_key(|s| if s.kind == ContentKind::Component { 0 } else { 1 });

        let mut schemas_imported = 0;
        for s in ordered_schemas {
            let uid = s.uid.clone();
            if self.schema.get_schema(&uid).await.is_ok() {
                self.schema.update_schema(&uid, s).await?;
            } else {
                self.schema.create_schema(s).await?;
            }
            schemas_imported += 1;
        }

        // Import drafts before published variants so documentId stays consistent
        let mut ordered_docs = bundle.documents;
        ordered_docs.sort_by_key(|d| if d.status == "draft" { 0 } else { 1 });

        let mut documents_imported = 0;
        let mut id_remap: BTreeMap<String, String> = BTreeMap::new();

        for item in ordered_docs {
            let schema = self.schema.get_schema(&item.content_type_uid).await?;
            let status = if item.status == "published" {
                DocumentStatus::Published
            } else {
                DocumentStatus::Draft
            };
            let mapped_id = id_remap
                .get(&item.document_id)
                .cloned()
                .unwrap_or_else(|| item.document_id.clone());

            let exists = self
                .content
                .find_one(
                    &schema,
                    &mapped_id,
                    &item.locale,
                    if schema.draft_and_publish {
                        DocumentStatus::Draft
                    } else {
                        DocumentStatus::Published
                    },
                    &PopulateSpec::None,
                    &[],
                    false,
                )
                .await
                .is_ok();

            if exists {
                self.content
                    .update_entry(
                        &schema,
                        &mapped_id,
                        &item.locale,
                        status,
                        &item.data,
                        actor_id,
                        false,
                    )
                    .await?;
            } else if schema.kind == ContentKind::SingleType {
                let created = self
                    .content
                    .upsert_single(
                        &schema,
                        &item.locale,
                        status,
                        &item.data,
                        actor_id,
                        false,
                    )
                    .await?;
                if let Some(new_id) = created.get("documentId").and_then(|v| v.as_str()) {
                    id_remap.insert(item.document_id, new_id.to_string());
                }
            } else {
                let created = self
                    .content
                    .create_entry(&schema, &item.locale, status, &item.data, actor_id, false)
                    .await?;
                if let Some(new_id) = created.get("documentId").and_then(|v| v.as_str()) {
                    id_remap.insert(item.document_id, new_id.to_string());
                }
            }
            documents_imported += 1;
        }

        Ok(DataTransferSummaryDto {
            schemas_imported,
            locales_imported,
            documents_imported,
            relations_imported: 0,
        })
    }

    pub async fn export_csv(&self, uid: &str, locale: Option<&str>) -> ApiResult<String> {
        let schema = self.schema.get_schema(uid).await?;
        if schema.kind == ContentKind::Component {
            return Err(ApiError::BadRequest(
                "Components cannot be exported as standalone CSV tables".into(),
            ));
        }
        let loc = match locale.filter(|s| !s.is_empty()) {
            Some(l) => l.to_string(),
            None => self.governance.default_locale().await?,
        };

        let phys_fields: Vec<&FieldDefinition> = schema
            .fields
            .iter()
            .filter(|f| f.field_type.has_physical_column() && f.field_type != FieldType::Password)
            .collect();

        let query = DocumentQuery {
            locale: Some(loc),
            page: 1,
            page_size: 10_000,
            ..Default::default()
        };
        let (rows, _) = self.content.find_admin_entries(uid, &query).await?;

        let mut wtr = csv::Writer::from_writer(Vec::new());
        let mut headers = vec!["documentId".to_string(), "locale".to_string()];
        for f in &phys_fields {
            headers.push(f.name.clone());
        }
        wtr.write_record(&headers)
            .map_err(|e| ApiError::Internal(format!("CSV header write failed: {e}")))?;

        for row in rows {
            let Some(obj) = row.as_object() else {
                continue;
            };
            let mut record = Vec::with_capacity(headers.len());
            record.push(
                obj.get("documentId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            );
            record.push(
                obj.get("locale")
                    .and_then(|v| v.as_str())
                    .unwrap_or("en")
                    .to_string(),
            );

            for f in &phys_fields {
                let cell = match obj.get(&f.name) {
                    None | Some(serde_json::Value::Null) => String::new(),
                    Some(serde_json::Value::String(s)) => sanitize_csv_cell(s),
                    Some(other) => sanitize_csv_cell(&other.to_string()),
                };
                record.push(cell);
            }
            wtr.write_record(&record)
                .map_err(|e| ApiError::Internal(format!("CSV row write failed: {e}")))?;
        }

        let bytes = wtr
            .into_inner()
            .map_err(|e| ApiError::Internal(format!("CSV flush failed: {e}")))?;
        String::from_utf8(bytes)
            .map_err(|e| ApiError::Internal(format!("CSV UTF-8 encode failed: {e}")))
    }

    pub async fn import_csv(
        &self,
        uid: &str,
        locale: Option<&str>,
        csv_text: &str,
        actor_id: Option<&str>,
    ) -> ApiResult<DataTransferSummaryDto> {
        let schema = self.schema.get_schema(uid).await?;
        let default_loc = match locale.filter(|s| !s.is_empty()) {
            Some(l) => l.to_string(),
            None => self.governance.default_locale().await?,
        };

        let mut rdr = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .from_reader(csv_text.as_bytes());

        let headers = rdr
            .headers()
            .map_err(|e| ApiError::BadRequest(format!("Invalid CSV headers: {e}")))?
            .clone();

        let mut imported = 0;
        let target_status = if schema.draft_and_publish {
            DocumentStatus::Draft
        } else {
            DocumentStatus::Published
        };

        for result in rdr.records() {
            let record =
                result.map_err(|e| ApiError::BadRequest(format!("Invalid CSV row: {e}")))?;
            let mut data_map = serde_json::Map::new();
            let mut row_locale = default_loc.clone();

            for (idx, col_name) in headers.iter().enumerate() {
                let raw_cell = record.get(idx).unwrap_or("").trim();
                if col_name == "locale" && !raw_cell.is_empty() {
                    row_locale = raw_cell.to_string();
                    continue;
                }
                if col_name == "documentId" || raw_cell.is_empty() {
                    continue;
                }
                if let Some(field_def) = schema.fields.iter().find(|f| f.name == col_name) {
                    let val = parse_csv_cell_for_field(field_def, raw_cell);
                    data_map.insert(field_def.name.clone(), val);
                }
            }

            if !data_map.is_empty() {
                let validated_locale = self
                    .governance
                    .resolve_and_validate_locale(schema.i18n_enabled, Some(&row_locale), true)
                    .await?;
                self.content
                    .create_entry(
                        &schema,
                        &validated_locale,
                        target_status,
                        &data_map,
                        actor_id,
                        false,
                    )
                    .await?;
                imported += 1;
            }
        }

        Ok(DataTransferSummaryDto {
            schemas_imported: 0,
            locales_imported: 0,
            documents_imported: imported,
            relations_imported: 0,
        })
    }

    /// Imports a Strapi v5 JSON export archive (`{ schemas: [...], entities: [...] }`).
    pub async fn import_strapi_v5_archive(
        &self,
        archive: &serde_json::Value,
        actor_id: Option<&str>,
    ) -> ApiResult<DataTransferSummaryDto> {
        let mut schemas_imported = 0;
        if let Some(raw_schemas) = archive.get("schemas").and_then(|v| v.as_array()) {
            for rs in raw_schemas {
                if let Some(converted) = convert_strapi_v5_schema(rs) {
                    let uid = converted.uid.clone();
                    if self.schema.get_schema(&uid).await.is_ok() {
                        self.schema.update_schema(&uid, converted).await?;
                    } else {
                        self.schema.create_schema(converted).await?;
                    }
                    schemas_imported += 1;
                }
            }
        }

        let mut documents_imported = 0;
        let def_locale = self.governance.default_locale().await?;

        if let Some(entities) = archive.get("entities").and_then(|v| v.as_array()) {
            for ent in entities {
                let Some(uid) = ent
                    .get("type")
                    .or_else(|| ent.get("uid"))
                    .and_then(|v| v.as_str())
                else {
                    continue;
                };
                let Ok(schema) = self.schema.get_schema(uid).await else {
                    continue;
                };
                let Some(data_obj) = ent.get("data").and_then(|v| v.as_object()) else {
                    continue;
                };

                let locale = data_obj
                    .get("locale")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&def_locale);
                let is_pub = data_obj
                    .get("publishedAt")
                    .is_some_and(|v| !v.is_null());
                let status = if is_pub || !schema.draft_and_publish {
                    DocumentStatus::Published
                } else {
                    DocumentStatus::Draft
                };

                let mut clean_data = serde_json::Map::new();
                for f in schema.fields.iter().filter(|f| f.field_type.has_physical_column()) {
                    if let Some(val) = data_obj.get(&f.name) {
                        clean_data.insert(f.name.clone(), val.clone());
                    }
                }

                self.content
                    .create_entry(&schema, locale, status, &clean_data, actor_id, false)
                    .await?;
                documents_imported += 1;
            }
        }

        Ok(DataTransferSummaryDto {
            schemas_imported,
            locales_imported: 0,
            documents_imported,
            relations_imported: 0,
        })
    }
}

// ============================================================================
// Internal Helpers
// ============================================================================

fn is_reserved_webhook_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "content-length"
            | "transfer-encoding"
            | "x-rapi-signature"
            | "x-rapi-event"
            | "x-strapi-event"
    )
}

pub(crate) async fn verify_webhook_dns_not_private(raw_url: &str) -> ApiResult<()> {
    let rest = raw_url
        .trim()
        .strip_prefix("https://")
        .or_else(|| raw_url.trim().strip_prefix("http://"))
        .unwrap_or("");
    let default_port: u16 = if raw_url.trim().starts_with("https://") {
        443
    } else {
        80
    };
    let authority = rest.split('/').next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let lookup_target = if host_port.contains(':') {
        host_port.to_string()
    } else {
        format!("{host_port}:{default_port}")
    };

    let addrs = tokio::net::lookup_host(&lookup_target).await.map_err(|e| {
        ApiError::BadRequest(format!("Webhook DNS resolution failed for '{host_port}': {e}"))
    })?;
    for sock in addrs {
        let blocked = match sock.ip() {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.octets()[0] == 0
            }
            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
        };
        if blocked {
            return Err(ApiError::BadRequest(
                "Webhook host resolves to a private, loopback, or link-local IP address".into(),
            ));
        }
    }
    Ok(())
}

fn validate_username(username: &str) -> ApiResult<()> {
    let t = username.trim();
    if t.len() < 3
        || t.len() > 48
        || !t
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(ApiError::Validation {
            message: "Username must be 3..48 ASCII alphanumeric, '_', '-', or '.' characters"
                .into(),
            details: serde_json::json!({ "field": "username" }),
        });
    }
    Ok(())
}

fn validate_webhook_events(events: &[String]) -> ApiResult<()> {
    if events.is_empty() {
        return Err(ApiError::Validation {
            message: "Select at least one webhook event".into(),
            details: serde_json::json!({ "field": "events" }),
        });
    }
    for ev in events {
        if ev != "*" && !SUPPORTED_WEBHOOK_EVENTS.contains(&ev.as_str()) {
            return Err(ApiError::Validation {
                message: format!("Unsupported webhook event '{ev}'"),
                details: serde_json::json!({ "event": ev }),
            });
        }
    }
    Ok(())
}

fn compute_hmac_sha256_hex(secret: &str, payload: &[u8]) -> ApiResult<String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| ApiError::Internal(format!("HMAC init failed: {e}")))?;
    mac.update(payload);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn render_template_vars(tpl: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = tpl.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{{ {k} }}}}"), v);
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

fn parse_csv_cell_for_field(field: &FieldDefinition, raw: &str) -> serde_json::Value {
    let unescaped = raw.strip_prefix('\'').unwrap_or(raw);
    match field.field_type {
        FieldType::Integer => unescaped
            .parse::<i64>()
            .map(serde_json::Value::from)
            .unwrap_or_else(|_| serde_json::Value::String(unescaped.to_string())),
        FieldType::Float | FieldType::Decimal => unescaped
            .parse::<f64>()
            .map(serde_json::Value::from)
            .unwrap_or_else(|_| serde_json::Value::String(unescaped.to_string())),
        FieldType::Boolean => serde_json::Value::Bool(matches!(
            unescaped.to_ascii_lowercase().as_str(),
            "true" | "1" | "yes"
        )),
        FieldType::Json | FieldType::Blocks => serde_json::from_str(unescaped)
            .unwrap_or_else(|_| serde_json::Value::String(unescaped.to_string())),
        _ => serde_json::Value::String(unescaped.to_string()),
    }
}

fn convert_strapi_v5_schema(raw: &serde_json::Value) -> Option<ContentTypeSchema> {
    let uid = raw.get("uid")?.as_str()?.to_string();
    let kind_str = raw
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("collectionType");
    let kind = match kind_str {
        "singleType" => ContentKind::SingleType,
        "component" => ContentKind::Component,
        _ => ContentKind::CollectionType,
    };

    let info = raw.get("info").and_then(|v| v.as_object());
    let singular_name = info
        .and_then(|i| i.get("singularName"))
        .or_else(|| raw.get("singularName"))
        .and_then(|v| v.as_str())
        .unwrap_or("item")
        .to_string();
    let plural_name = info
        .and_then(|i| i.get("pluralName"))
        .or_else(|| raw.get("pluralName"))
        .and_then(|v| v.as_str())
        .unwrap_or("items")
        .to_string();
    let display_name = info
        .and_then(|i| i.get("displayName"))
        .or_else(|| raw.get("displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or(&singular_name)
        .to_string();

    let draft_and_publish = raw
        .get("options")
        .and_then(|o| o.get("draftAndPublish"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let i18n_enabled = raw
        .get("pluginOptions")
        .and_then(|p| p.get("i18n"))
        .and_then(|i| i.get("localized"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut fields = Vec::new();
    if let Some(attrs) = raw.get("attributes").and_then(|v| v.as_object()) {
        for (name, attr) in attrs {
            let t_str = attr.get("type").and_then(|v| v.as_str()).unwrap_or("string");
            let Some(field_type) = FieldType::parse_str(t_str) else {
                continue;
            };
            if !field_type.has_physical_column() {
                continue;
            }
            let required = attr
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let unique = attr
                .get("unique")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let localized = attr
                .get("pluginOptions")
                .and_then(|p| p.get("i18n"))
                .and_then(|i| i.get("localized"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            fields.push(FieldDefinition {
                name: name.clone(),
                field_type,
                required,
                unique,
                localized,
                private: attr.get("private").and_then(|v| v.as_bool()).unwrap_or(false),
                default_value: attr.get("default").cloned(),
                validation: FieldValidation::default(),
                relation: None,
                component: None,
                custom_field_uid: None,
            });
        }
    }

    Some(ContentTypeSchema {
        uid,
        kind,
        category: raw.get("category").and_then(|v| v.as_str()).map(String::from),
        singular_name,
        plural_name,
        display_name,
        description: String::new(),
        icon: "file-text".to_string(),
        draft_and_publish,
        i18n_enabled,
        plugin_options: serde_json::json!({}),
        options: serde_json::json!({}),
        fields,
        created_at: String::new(),
        updated_at: String::new(),
    })
}
