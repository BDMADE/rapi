use crate::pool::DbPools;
use cms_core::{
    ApiError, ApiResult, DocumentId, EmailOutboxDto, EmailTemplateDto, EndUserDto,
    WebhookDeliveryDto, WebhookDto, UserRoleDto,
};
use sqlx::Row;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct EndUserRecord {
    pub id: String,
    pub document_id: String,
    pub username: String,
    pub email: String,
    pub provider: String,
    pub password_hash: String,
    pub reset_password_token: Option<String>,
    pub reset_password_expires_at: Option<String>,
    pub confirmed: bool,
    pub blocked: bool,
    pub role_id: String,
    pub role_name: String,
    pub role_type: String,
    pub created_at: String,
    pub updated_at: String,
}

impl EndUserRecord {
    pub fn into_dto(self) -> EndUserDto {
        EndUserDto {
            id: self.id,
            document_id: self.document_id,
            username: self.username,
            email: self.email,
            provider: self.provider,
            confirmed: self.confirmed,
            blocked: self.blocked,
            role_id: self.role_id,
            role_name: self.role_name,
            role_type: self.role_type,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WebhookRecord {
    pub dto: WebhookDto,
    pub secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecordWebhookDeliveryInput {
    pub webhook_id: String,
    pub event: String,
    pub status_code: Option<i64>,
    pub success: bool,
    pub attempts: i64,
    pub request_json: String,
    pub response_body: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RecordEmailOutboxInput {
    pub to_address: String,
    pub from_address: String,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
    pub transport: String,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Clone)]
pub struct IntegrationsRepository {
    pools: DbPools,
}

impl IntegrationsRepository {
    pub fn new(pools: DbPools) -> Self {
        Self { pools }
    }

    // =========================================================================
    // End-Users & User Roles (`end_users`, `user_roles`)
    // =========================================================================

    pub async fn list_user_roles(&self) -> ApiResult<Vec<UserRoleDto>> {
        let rows = sqlx::query(
            r#"
            SELECT ur.id, ur.role_type, ur.name, ur.description, ur.created_at, ur.updated_at,
                   (SELECT COUNT(*) FROM end_users eu WHERE eu.role_id = ur.id) AS user_count
            FROM user_roles ur
            ORDER BY ur.role_type ASC
            "#,
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_user_roles failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| UserRoleDto {
                id: r.get("id"),
                role_type: r.get("role_type"),
                name: r.get("name"),
                description: r.get("description"),
                user_count: r.get("user_count"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn user_role_exists(&self, role_id: &str) -> ApiResult<bool> {
        let cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_roles WHERE id = ?")
            .bind(role_id)
            .fetch_one(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("user_role_exists failed: {e}")))?;
        Ok(cnt > 0)
    }

    pub async fn list_end_users(&self) -> ApiResult<Vec<EndUserDto>> {
        let rows = sqlx::query(
            r#"
            SELECT eu.id, eu.document_id, eu.username, eu.email, eu.provider, eu.password_hash,
                   eu.reset_password_token, eu.reset_password_expires_at, eu.confirmed, eu.blocked,
                   eu.role_id, ur.name AS role_name, ur.role_type, eu.created_at, eu.updated_at
            FROM end_users eu
            JOIN user_roles ur ON ur.id = eu.role_id
            ORDER BY eu.created_at DESC
            "#,
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_end_users failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| map_end_user_record(r).into_dto())
            .collect())
    }

    pub async fn find_end_user_by_id(&self, id: &str) -> ApiResult<Option<EndUserRecord>> {
        let row = sqlx::query(
            r#"
            SELECT eu.id, eu.document_id, eu.username, eu.email, eu.provider, eu.password_hash,
                   eu.reset_password_token, eu.reset_password_expires_at, eu.confirmed, eu.blocked,
                   eu.role_id, ur.name AS role_name, ur.role_type, eu.created_at, eu.updated_at
            FROM end_users eu
            JOIN user_roles ur ON ur.id = eu.role_id
            WHERE eu.id = ?
            LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("find_end_user_by_id failed: {e}")))?;
        Ok(row.map(map_end_user_record))
    }

    pub async fn find_end_user_by_identifier(
        &self,
        identifier: &str,
    ) -> ApiResult<Option<EndUserRecord>> {
        let lower = identifier.trim().to_lowercase();
        let row = sqlx::query(
            r#"
            SELECT eu.id, eu.document_id, eu.username, eu.email, eu.provider, eu.password_hash,
                   eu.reset_password_token, eu.reset_password_expires_at, eu.confirmed, eu.blocked,
                   eu.role_id, ur.name AS role_name, ur.role_type, eu.created_at, eu.updated_at
            FROM end_users eu
            JOIN user_roles ur ON ur.id = eu.role_id
            WHERE LOWER(eu.email) = ? OR LOWER(eu.username) = ?
            LIMIT 1
            "#,
        )
        .bind(&lower)
        .bind(&lower)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("find_end_user_by_identifier failed: {e}")))?;
        Ok(row.map(map_end_user_record))
    }

    pub async fn create_end_user(
        &self,
        username: &str,
        email: &str,
        password_hash: &str,
        role_id: &str,
        confirmed: bool,
        blocked: bool,
    ) -> ApiResult<EndUserDto> {
        let id = Uuid::now_v7().to_string();
        let doc_id = DocumentId::generate().to_string();

        sqlx::query(
            r#"
            INSERT INTO end_users (
                id, document_id, username, email, provider, password_hash,
                confirmed, blocked, role_id
            )
            VALUES (?, ?, ?, ?, 'local', ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&doc_id)
        .bind(username.trim())
        .bind(email.trim().to_lowercase())
        .bind(password_hash)
        .bind(if confirmed { 1 } else { 0 })
        .bind(if blocked { 1 } else { 0 })
        .bind(role_id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("UNIQUE") {
                ApiError::Conflict("Username or email is already registered".into())
            } else if msg.contains("FOREIGN KEY") {
                ApiError::BadRequest(format!("Invalid user role '{role_id}'"))
            } else {
                ApiError::Internal(format!("create_end_user failed: {e}"))
            }
        })?;

        self.find_end_user_by_id(&id)
            .await?
            .map(EndUserRecord::into_dto)
            .ok_or_else(|| ApiError::Internal("Created end user not found".into()))
    }

    pub async fn update_end_user(
        &self,
        id: &str,
        blocked: Option<bool>,
        confirmed: Option<bool>,
        role_id: Option<&str>,
        password_hash: Option<&str>,
    ) -> ApiResult<EndUserDto> {
        let existing = self
            .find_end_user_by_id(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("End user '{id}' not found")))?;

        let next_blocked = blocked.unwrap_or(existing.blocked);
        let next_confirmed = confirmed.unwrap_or(existing.confirmed);
        let next_role = role_id.unwrap_or(&existing.role_id);
        let next_hash = password_hash.unwrap_or(&existing.password_hash);

        sqlx::query(
            r#"
            UPDATE end_users
            SET blocked = ?, confirmed = ?, role_id = ?, password_hash = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?
            "#,
        )
        .bind(if next_blocked { 1 } else { 0 })
        .bind(if next_confirmed { 1 } else { 0 })
        .bind(next_role)
        .bind(next_hash)
        .bind(id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| {
            if e.to_string().contains("FOREIGN KEY") {
                ApiError::BadRequest(format!("Invalid user role '{next_role}'"))
            } else {
                ApiError::Internal(format!("update_end_user failed: {e}"))
            }
        })?;

        self.find_end_user_by_id(id)
            .await?
            .map(EndUserRecord::into_dto)
            .ok_or_else(|| ApiError::NotFound(format!("End user '{id}' not found")))
    }

    pub async fn delete_end_user(&self, id: &str) -> ApiResult<()> {
        let res = sqlx::query("DELETE FROM end_users WHERE id = ?")
            .bind(id)
            .execute(&self.pools.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("delete_end_user failed: {e}")))?;
        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("End user '{id}' not found")));
        }
        Ok(())
    }

    pub async fn store_end_user_reset_token(
        &self,
        user_id: &str,
        token_hash: &str,
        expires_at: &str,
    ) -> ApiResult<()> {
        sqlx::query(
            "UPDATE end_users SET reset_password_token = ?, reset_password_expires_at = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(token_hash)
        .bind(expires_at)
        .bind(user_id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("store_end_user_reset_token failed: {e}")))?;
        Ok(())
    }

    pub async fn consume_end_user_reset_token(
        &self,
        token_hash: &str,
        now_iso: &str,
        new_password_hash: &str,
    ) -> ApiResult<EndUserDto> {
        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin reset password tx: {e}")))?;

        let user_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM end_users WHERE reset_password_token = ? AND reset_password_expires_at > ? AND blocked = 0 LIMIT 1",
        )
        .bind(token_hash)
        .bind(now_iso)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("lookup reset token failed: {e}")))?;

        let Some(uid) = user_id else {
            return Err(ApiError::BadRequest(
                "Invalid or expired password reset code".into(),
            ));
        };

        sqlx::query(
            r#"
            UPDATE end_users
            SET password_hash = ?, reset_password_token = NULL, reset_password_expires_at = NULL,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?
            "#,
        )
        .bind(new_password_hash)
        .bind(&uid)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("update password in reset tx failed: {e}")))?;

        sqlx::query("DELETE FROM sessions WHERE user_type = 'end_user' AND user_id = ?")
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete end_user sessions failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit reset password failed: {e}")))?;

        self.find_end_user_by_id(&uid)
            .await?
            .map(EndUserRecord::into_dto)
            .ok_or_else(|| ApiError::Internal("Reset user not found".into()))
    }

    // =========================================================================
    // Webhooks & Delivery Logs (`webhooks`, `webhook_deliveries`)
    // =========================================================================

    pub async fn list_webhooks(&self) -> ApiResult<Vec<WebhookDto>> {
        let rows = sqlx::query(
            "SELECT id, name, url, headers_json, events_json, secret, is_enabled, created_at, updated_at FROM webhooks ORDER BY created_at DESC",
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_webhooks failed: {e}")))?;

        Ok(rows.into_iter().map(|r| map_webhook_record(r).dto).collect())
    }

    pub async fn list_enabled_webhooks_for_event(&self, event: &str) -> ApiResult<Vec<WebhookRecord>> {
        let rows = sqlx::query(
            "SELECT id, name, url, headers_json, events_json, secret, is_enabled, created_at, updated_at FROM webhooks WHERE is_enabled = 1",
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_enabled_webhooks failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(map_webhook_record)
            .filter(|w| w.dto.events.iter().any(|ev| ev == "*" || ev == event))
            .collect())
    }

    pub async fn get_webhook_record(&self, id: &str) -> ApiResult<Option<WebhookRecord>> {
        let row = sqlx::query(
            "SELECT id, name, url, headers_json, events_json, secret, is_enabled, created_at, updated_at FROM webhooks WHERE id = ? LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get_webhook_record failed: {e}")))?;
        Ok(row.map(map_webhook_record))
    }

    pub async fn create_webhook(
        &self,
        name: &str,
        url: &str,
        headers: &BTreeMap<String, String>,
        events: &[String],
        secret: Option<&str>,
        is_enabled: bool,
    ) -> ApiResult<WebhookDto> {
        let id = format!("wh_{}", Uuid::now_v7().simple());
        let headers_json = serde_json::to_string(headers).unwrap_or_else(|_| "{}".into());
        let events_json = serde_json::to_string(events).unwrap_or_else(|_| "[]".into());

        sqlx::query(
            "INSERT INTO webhooks (id, name, url, headers_json, events_json, secret, is_enabled) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(url)
        .bind(&headers_json)
        .bind(&events_json)
        .bind(secret)
        .bind(if is_enabled { 1 } else { 0 })
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("create_webhook failed: {e}")))?;

        self.get_webhook_record(&id)
            .await?
            .map(|r| r.dto)
            .ok_or_else(|| ApiError::Internal("Created webhook not found".into()))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_webhook(
        &self,
        id: &str,
        name: Option<&str>,
        url: Option<&str>,
        headers: Option<&BTreeMap<String, String>>,
        events: Option<&[String]>,
        secret: Option<Option<&str>>,
        is_enabled: Option<bool>,
    ) -> ApiResult<WebhookDto> {
        let existing = self
            .get_webhook_record(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Webhook '{id}' not found")))?;

        let next_name = name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.dto.name);
        let next_url = url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.dto.url);
        let next_headers = headers.cloned().unwrap_or(existing.dto.headers);
        let next_events = events
            .map(|e| e.to_vec())
            .unwrap_or(existing.dto.events);
        let next_secret = match secret {
            Some(v) => v.map(String::from),
            None => existing.secret,
        };
        let next_enabled = is_enabled.unwrap_or(existing.dto.is_enabled);

        let headers_json = serde_json::to_string(&next_headers).unwrap_or_else(|_| "{}".into());
        let events_json = serde_json::to_string(&next_events).unwrap_or_else(|_| "[]".into());

        sqlx::query(
            r#"
            UPDATE webhooks
            SET name = ?, url = ?, headers_json = ?, events_json = ?, secret = ?, is_enabled = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?
            "#,
        )
        .bind(next_name)
        .bind(next_url)
        .bind(&headers_json)
        .bind(&events_json)
        .bind(&next_secret)
        .bind(if next_enabled { 1 } else { 0 })
        .bind(id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("update_webhook failed: {e}")))?;

        self.get_webhook_record(id)
            .await?
            .map(|r| r.dto)
            .ok_or_else(|| ApiError::NotFound(format!("Webhook '{id}' not found")))
    }

    pub async fn delete_webhook(&self, id: &str) -> ApiResult<()> {
        let res = sqlx::query("DELETE FROM webhooks WHERE id = ?")
            .bind(id)
            .execute(&self.pools.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("delete_webhook failed: {e}")))?;
        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("Webhook '{id}' not found")));
        }
        Ok(())
    }

    pub async fn record_webhook_delivery(
        &self,
        input: RecordWebhookDeliveryInput,
    ) -> ApiResult<WebhookDeliveryDto> {
        let id = format!("whd_{}", Uuid::now_v7().simple());
        sqlx::query(
            r#"
            INSERT INTO webhook_deliveries (
                id, webhook_id, event, status_code, success, attempts,
                request_json, response_body, duration_ms
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&input.webhook_id)
        .bind(&input.event)
        .bind(input.status_code)
        .bind(if input.success { 1 } else { 0 })
        .bind(input.attempts)
        .bind(&input.request_json)
        .bind(&input.response_body)
        .bind(input.duration_ms)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("record_webhook_delivery failed: {e}")))?;

        let row = sqlx::query(
            "SELECT id, webhook_id, event, status_code, success, attempts, request_json, response_body, duration_ms, created_at FROM webhook_deliveries WHERE id = ?",
        )
        .bind(&id)
        .fetch_one(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("fetch created delivery failed: {e}")))?;

        Ok(map_delivery_row(row))
    }

    pub async fn list_webhook_deliveries(
        &self,
        webhook_id: &str,
    ) -> ApiResult<Vec<WebhookDeliveryDto>> {
        let rows = sqlx::query(
            r#"
            SELECT id, webhook_id, event, status_code, success, attempts,
                   request_json, response_body, duration_ms, created_at
            FROM webhook_deliveries
            WHERE webhook_id = ?
            ORDER BY created_at DESC
            LIMIT 50
            "#,
        )
        .bind(webhook_id)
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_webhook_deliveries failed: {e}")))?;

        Ok(rows.into_iter().map(map_delivery_row).collect())
    }

    // =========================================================================
    // Email Templates & Outbox (`email_templates`, `email_outbox`)
    // =========================================================================

    pub async fn list_email_templates(&self) -> ApiResult<Vec<EmailTemplateDto>> {
        let rows = sqlx::query(
            "SELECT id, code, subject, html_body, text_body, updated_at FROM email_templates ORDER BY code ASC",
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_email_templates failed: {e}")))?;

        Ok(rows.into_iter().map(map_template_row).collect())
    }

    pub async fn get_email_template_by_code(
        &self,
        code: &str,
    ) -> ApiResult<Option<EmailTemplateDto>> {
        let row = sqlx::query(
            "SELECT id, code, subject, html_body, text_body, updated_at FROM email_templates WHERE code = ? LIMIT 1",
        )
        .bind(code)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get_email_template_by_code failed: {e}")))?;
        Ok(row.map(map_template_row))
    }

    pub async fn upsert_email_template(
        &self,
        code: &str,
        subject: &str,
        html_body: &str,
        text_body: &str,
    ) -> ApiResult<EmailTemplateDto> {
        let id = format!("tpl_{}", code.trim().to_lowercase());
        sqlx::query(
            r#"
            INSERT INTO email_templates (id, code, subject, html_body, text_body, updated_at)
            VALUES (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            ON CONFLICT(code) DO UPDATE SET
                subject = excluded.subject,
                html_body = excluded.html_body,
                text_body = excluded.text_body,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&id)
        .bind(code.trim())
        .bind(subject)
        .bind(html_body)
        .bind(text_body)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("upsert_email_template failed: {e}")))?;

        self.get_email_template_by_code(code.trim())
            .await?
            .ok_or_else(|| ApiError::Internal("Saved email template not found".into()))
    }

    pub async fn record_email_outbox(
        &self,
        input: RecordEmailOutboxInput,
    ) -> ApiResult<EmailOutboxDto> {
        let id = format!("eml_{}", Uuid::now_v7().simple());
        sqlx::query(
            r#"
            INSERT INTO email_outbox (
                id, to_address, from_address, subject, html_body, text_body,
                transport, status, error_message
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&input.to_address)
        .bind(&input.from_address)
        .bind(&input.subject)
        .bind(&input.html_body)
        .bind(&input.text_body)
        .bind(&input.transport)
        .bind(&input.status)
        .bind(&input.error_message)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("record_email_outbox failed: {e}")))?;

        let row = sqlx::query(
            "SELECT id, to_address, from_address, subject, transport, status, error_message, created_at FROM email_outbox WHERE id = ?",
        )
        .bind(&id)
        .fetch_one(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("fetch email_outbox failed: {e}")))?;

        Ok(map_outbox_row(row))
    }

    pub async fn list_email_outbox(&self) -> ApiResult<Vec<EmailOutboxDto>> {
        let rows = sqlx::query(
            "SELECT id, to_address, from_address, subject, transport, status, error_message, created_at FROM email_outbox ORDER BY created_at DESC LIMIT 50",
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_email_outbox failed: {e}")))?;
        Ok(rows.into_iter().map(map_outbox_row).collect())
    }
}

fn map_end_user_record(r: sqlx::sqlite::SqliteRow) -> EndUserRecord {
    EndUserRecord {
        id: r.get("id"),
        document_id: r.get("document_id"),
        username: r.get("username"),
        email: r.get("email"),
        provider: r.get("provider"),
        password_hash: r.get("password_hash"),
        reset_password_token: r.get("reset_password_token"),
        reset_password_expires_at: r.get("reset_password_expires_at"),
        confirmed: r.get::<i64, _>("confirmed") != 0,
        blocked: r.get::<i64, _>("blocked") != 0,
        role_id: r.get("role_id"),
        role_name: r.get("role_name"),
        role_type: r.get("role_type"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn map_webhook_record(r: sqlx::sqlite::SqliteRow) -> WebhookRecord {
    let raw_h: String = r.get("headers_json");
    let headers: BTreeMap<String, String> = serde_json::from_str(&raw_h).unwrap_or_default();
    let raw_e: String = r.get("events_json");
    let events: Vec<String> = serde_json::from_str(&raw_e).unwrap_or_default();
    let secret: Option<String> = r
        .get::<Option<String>, _>("secret")
        .filter(|s| !s.is_empty());

    WebhookRecord {
        dto: WebhookDto {
            id: r.get("id"),
            name: r.get("name"),
            url: r.get("url"),
            headers,
            events,
            has_secret: secret.is_some(),
            is_enabled: r.get::<i64, _>("is_enabled") != 0,
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        },
        secret,
    }
}

fn map_delivery_row(r: sqlx::sqlite::SqliteRow) -> WebhookDeliveryDto {
    let raw_req: String = r.get("request_json");
    let request = serde_json::from_str(&raw_req).unwrap_or_else(|_| serde_json::json!({}));
    WebhookDeliveryDto {
        id: r.get("id"),
        webhook_id: r.get("webhook_id"),
        event: r.get("event"),
        status_code: r.get("status_code"),
        success: r.get::<i64, _>("success") != 0,
        attempts: r.get("attempts"),
        request,
        response_body: r.get("response_body"),
        duration_ms: r.get("duration_ms"),
        created_at: r.get("created_at"),
    }
}

fn map_template_row(r: sqlx::sqlite::SqliteRow) -> EmailTemplateDto {
    EmailTemplateDto {
        id: r.get("id"),
        code: r.get("code"),
        subject: r.get("subject"),
        html_body: r.get("html_body"),
        text_body: r.get("text_body"),
        updated_at: r.get("updated_at"),
    }
}

fn map_outbox_row(r: sqlx::sqlite::SqliteRow) -> EmailOutboxDto {
    EmailOutboxDto {
        id: r.get("id"),
        to_address: r.get("to_address"),
        from_address: r.get("from_address"),
        subject: r.get("subject"),
        transport: r.get("transport"),
        status: r.get("status"),
        error_message: r.get("error_message"),
        created_at: r.get("created_at"),
    }
}
