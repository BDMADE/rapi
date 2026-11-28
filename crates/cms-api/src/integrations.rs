use crate::auth::{enforce_permission, AuthActor, RequireAdmin};
use crate::error::HttpApiError;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use cms_core::{
    Actor, ApiError, ApiResponse, DataTransferSummaryDto, EmailOutboxDto, EmailSettingsStatusDto,
    EmailTemplateDto, EndUserAuthResponse, EndUserDto, PermissionRule, UserRoleDto,
    WebhookDeliveryDto, WebhookDto,
};
use cms_services::{
    AdminCreateEndUserInput, AdminUpdateEndUserInput, CreateWebhookInput, EndUserLoginInput,
    EndUserRegisterInput, ForgotPasswordInput, PlatformBundleDto, ResetPasswordInput,
    SendTestEmailInput, ServiceContainer, UpdateWebhookInput, UpsertEmailTemplateInput,
};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportQueryParams {
    format: Option<String>, // "json" | "csv"
    uid: Option<String>,
    locale: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserRolePermsBody {
    permissions: Vec<PermissionRule>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CsvImportBody {
    csv: String,
    locale: Option<String>,
}

pub fn integrations_routes() -> Router<ServiceContainer> {
    Router::new()
        // --- Strapi v5 End-User Auth (`/api/auth/*`, `/api/users/me`) ---
        .route("/api/auth/local", post(end_user_login_handler))
        .route("/api/auth/local/register", post(end_user_register_handler))
        .route(
            "/api/auth/forgot-password",
            post(end_user_forgot_password_handler),
        )
        .route(
            "/api/auth/reset-password",
            post(end_user_reset_password_handler),
        )
        .route("/api/users/me", get(end_user_me_handler))
        // --- Admin End-Users & User Roles ---
        .route(
            "/admin/end-users",
            get(admin_list_end_users_handler).post(admin_create_end_user_handler),
        )
        .route(
            "/admin/end-users/{id}",
            put(admin_update_end_user_handler).delete(admin_delete_end_user_handler),
        )
        .route("/admin/user-roles", get(admin_list_user_roles_handler))
        .route(
            "/admin/user-roles/{id}/permissions",
            get(admin_get_user_role_perms_handler).put(admin_update_user_role_perms_handler),
        )
        // --- Webhooks ---
        .route(
            "/admin/webhooks",
            get(admin_list_webhooks_handler).post(admin_create_webhook_handler),
        )
        .route(
            "/admin/webhooks/{id}",
            put(admin_update_webhook_handler).delete(admin_delete_webhook_handler),
        )
        .route(
            "/admin/webhooks/{id}/trigger",
            post(admin_trigger_webhook_handler),
        )
        .route(
            "/admin/webhooks/{id}/deliveries",
            get(admin_list_webhook_deliveries_handler),
        )
        // --- Email Plugin ---
        .route("/admin/email/settings", get(admin_email_settings_handler))
        .route("/admin/email/templates", get(admin_list_email_templates_handler))
        .route(
            "/admin/email/templates/{code}",
            put(admin_upsert_email_template_handler),
        )
        .route("/admin/email/test", post(admin_send_test_email_handler))
        .route("/admin/email/outbox", get(admin_list_email_outbox_handler))
        // --- Data Transfer (Export / Import / Strapi v5 Importer) ---
        .route("/admin/data-transfer/export", get(admin_export_data_handler))
        .route(
            "/admin/data-transfer/import/json",
            post(admin_import_json_bundle_handler),
        )
        .route(
            "/admin/data-transfer/import/csv/{uid}",
            post(admin_import_csv_handler),
        )
        .route(
            "/admin/data-transfer/import/strapi-v5",
            post(admin_import_strapi_v5_handler),
        )
}

fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "local".to_string())
}

fn actor_id_str(actor: &Actor) -> Option<&str> {
    match actor {
        Actor::AdminUser { user_id, .. } | Actor::EndUser { user_id, .. } => Some(user_id.as_str()),
        Actor::AdminToken { token_id, .. } | Actor::ApiToken { token_id, .. } => {
            Some(token_id.as_str())
        }
        Actor::Public { .. } => None,
    }
}

// ============================================================================
// End-User Auth Handlers (`/api/auth/local*`, `/api/users/me`)
// ============================================================================

async fn end_user_register_handler(
    State(services): State<ServiceContainer>,
    headers: HeaderMap,
    Json(input): Json<EndUserRegisterInput>,
) -> Result<Json<EndUserAuthResponse>, HttpApiError> {
    let ip = client_ip(&headers);
    services
        .auth
        .check_rate_limit("end_user_register_ip", &ip, 20, 60)
        .await?;
    let res = services.integrations.register_end_user(input).await?;
    Ok(Json(res))
}

async fn end_user_login_handler(
    State(services): State<ServiceContainer>,
    headers: HeaderMap,
    Json(input): Json<EndUserLoginInput>,
) -> Result<Json<EndUserAuthResponse>, HttpApiError> {
    let ip = client_ip(&headers);
    let ident_key = input.identifier.trim().to_lowercase();
    services
        .auth
        .check_rate_limit("end_user_login_ip", &ip, 30, 60)
        .await?;
    services
        .auth
        .check_rate_limit("end_user_login_id", &ident_key, 10, 60)
        .await?;
    let res = services.integrations.login_end_user(input).await?;
    Ok(Json(res))
}

async fn end_user_forgot_password_handler(
    State(services): State<ServiceContainer>,
    headers: HeaderMap,
    Json(input): Json<ForgotPasswordInput>,
) -> Result<Json<serde_json::Value>, HttpApiError> {
    let ip = client_ip(&headers);
    services
        .auth
        .check_rate_limit("end_user_forgot_ip", &ip, 10, 60)
        .await?;
    services.integrations.forgot_end_user_password(input).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn end_user_reset_password_handler(
    State(services): State<ServiceContainer>,
    headers: HeaderMap,
    Json(input): Json<ResetPasswordInput>,
) -> Result<Json<EndUserAuthResponse>, HttpApiError> {
    let ip = client_ip(&headers);
    services
        .auth
        .check_rate_limit("end_user_reset_ip", &ip, 10, 60)
        .await?;
    let res = services.integrations.reset_end_user_password(input).await?;
    Ok(Json(res))
}

async fn end_user_me_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
) -> Result<Json<EndUserDto>, HttpApiError> {
    let Actor::EndUser { user_id, .. } = actor else {
        return Err(HttpApiError(ApiError::Unauthorized(
            "Valid end-user JWT required".into(),
        )));
    };
    let me = services.integrations.get_end_user_me(&user_id).await?;
    Ok(Json(me))
}

// ============================================================================
// Admin End-Users & User Roles Handlers
// ============================================================================

async fn admin_list_end_users_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<EndUserDto>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::users-permissions.users.read", None)?;
    let list = services.integrations.list_end_users().await?;
    Ok(Json(ApiResponse::new(list)))
}

async fn admin_create_end_user_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(input): Json<AdminCreateEndUserInput>,
) -> Result<Json<ApiResponse<EndUserDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::users-permissions.users.update", None)?;
    let created = services.integrations.create_end_user_admin(input).await?;
    Ok(Json(ApiResponse::new(created)))
}

async fn admin_update_end_user_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
    Json(input): Json<AdminUpdateEndUserInput>,
) -> Result<Json<ApiResponse<EndUserDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::users-permissions.users.update", None)?;
    let updated = services
        .integrations
        .update_end_user_admin(&id, input)
        .await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn admin_delete_end_user_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "plugin::users-permissions.users.update", None)?;
    services.integrations.delete_end_user_admin(&id).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": id }))))
}

async fn admin_list_user_roles_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<UserRoleDto>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::users-permissions.users.read", None)?;
    let roles = services.integrations.list_user_roles().await?;
    Ok(Json(ApiResponse::new(roles)))
}

async fn admin_get_user_role_perms_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<PermissionRule>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::users-permissions.users.read", None)?;
    let rules = services.auth.get_user_role_permissions(&id).await?;
    Ok(Json(ApiResponse::new(rules)))
}

async fn admin_update_user_role_perms_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
    Json(body): Json<UpdateUserRolePermsBody>,
) -> Result<Json<ApiResponse<Vec<PermissionRule>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::users-permissions.users.update", None)?;
    let updated = services
        .auth
        .update_user_role_permissions(&id, &body.permissions)
        .await?;
    Ok(Json(ApiResponse::new(updated)))
}

// ============================================================================
// Webhooks Handlers
// ============================================================================

async fn admin_list_webhooks_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<WebhookDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::webhooks.read", None)?;
    let list = services.integrations.list_webhooks().await?;
    Ok(Json(ApiResponse::new(list)))
}

async fn admin_create_webhook_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(input): Json<CreateWebhookInput>,
) -> Result<Json<ApiResponse<WebhookDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::webhooks.update", None)?;
    let created = services.integrations.create_webhook(input).await?;
    Ok(Json(ApiResponse::new(created)))
}

async fn admin_update_webhook_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
    Json(input): Json<UpdateWebhookInput>,
) -> Result<Json<ApiResponse<WebhookDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::webhooks.update", None)?;
    let updated = services.integrations.update_webhook(&id, input).await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn admin_delete_webhook_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::webhooks.update", None)?;
    services.integrations.delete_webhook(&id).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": id }))))
}

async fn admin_trigger_webhook_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<WebhookDeliveryDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::webhooks.update", None)?;
    let delivery = services.integrations.trigger_test_webhook(&id).await?;
    Ok(Json(ApiResponse::new(delivery)))
}

async fn admin_list_webhook_deliveries_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<WebhookDeliveryDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::webhooks.read", None)?;
    let logs = services.integrations.list_webhook_deliveries(&id).await?;
    Ok(Json(ApiResponse::new(logs)))
}

// ============================================================================
// Email Plugin Handlers
// ============================================================================

async fn admin_email_settings_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<EmailSettingsStatusDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::email.update", None)?;
    Ok(Json(ApiResponse::new(
        services.integrations.get_email_settings_status(),
    )))
}

async fn admin_list_email_templates_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<EmailTemplateDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::email.update", None)?;
    let list = services.integrations.list_email_templates().await?;
    Ok(Json(ApiResponse::new(list)))
}

async fn admin_upsert_email_template_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(code): Path<String>,
    Json(input): Json<UpsertEmailTemplateInput>,
) -> Result<Json<ApiResponse<EmailTemplateDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::email.update", None)?;
    let saved = services
        .integrations
        .upsert_email_template(&code, input)
        .await?;
    Ok(Json(ApiResponse::new(saved)))
}

async fn admin_send_test_email_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(input): Json<SendTestEmailInput>,
) -> Result<Json<ApiResponse<EmailOutboxDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::email.update", None)?;
    let sent = services.integrations.send_test_email(input).await?;
    Ok(Json(ApiResponse::new(sent)))
}

async fn admin_list_email_outbox_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<EmailOutboxDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::email.update", None)?;
    let list = services.integrations.list_email_outbox().await?;
    Ok(Json(ApiResponse::new(list)))
}

// ============================================================================
// Data Transfer Handlers
// ============================================================================

async fn admin_export_data_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Query(q): Query<ExportQueryParams>,
) -> Result<Response, HttpApiError> {
    enforce_permission(&actor, "admin::data-transfer.execute", None)?;

    if q.format.as_deref() == Some("csv") {
        let Some(uid) = q.uid.as_deref().filter(|s| !s.is_empty()) else {
            return Err(HttpApiError(ApiError::BadRequest(
                "Query parameter 'uid' is required when format=csv".into(),
            )));
        };
        let csv_text = services
            .integrations
            .export_csv(uid, q.locale.as_deref())
            .await?;
        let mut res = (StatusCode::OK, csv_text).into_response();
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/csv; charset=utf-8"),
        );
        return Ok(res);
    }

    let bundle = services.integrations.export_json_bundle().await?;
    Ok(Json(ApiResponse::new(bundle)).into_response())
}

async fn admin_import_json_bundle_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(bundle): Json<PlatformBundleDto>,
) -> Result<Json<ApiResponse<DataTransferSummaryDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::data-transfer.execute", None)?;
    let summary = services
        .integrations
        .import_json_bundle(bundle, actor_id_str(&actor))
        .await?;
    Ok(Json(ApiResponse::new(summary)))
}

async fn admin_import_csv_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
    Json(body): Json<CsvImportBody>,
) -> Result<Json<ApiResponse<DataTransferSummaryDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::data-transfer.execute", None)?;
    let summary = services
        .integrations
        .import_csv(&uid, body.locale.as_deref(), &body.csv, actor_id_str(&actor))
        .await?;
    Ok(Json(ApiResponse::new(summary)))
}

async fn admin_import_strapi_v5_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(archive): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<DataTransferSummaryDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::data-transfer.execute", None)?;
    let summary = services
        .integrations
        .import_strapi_v5_archive(&archive, actor_id_str(&actor))
        .await?;
    Ok(Json(ApiResponse::new(summary)))
}
