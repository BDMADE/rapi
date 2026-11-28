use crate::auth::{enforce_permission, AuthActor, RequireAdmin};
use crate::error::HttpApiError;
use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use cms_core::{
    Actor, ApiError, ApiResponse, AuditLogDto, AuditLogFilter, DocumentLocaleStateDto,
    DocumentStatus, InvalidationEvent, LocaleDto, PopulateSpec, PreviewConfigDto, ReleaseDto,
    ResolvedPreviewDto, WorkflowEntryDto, WorkflowStageDto,
};
use cms_services::{
    AddReleaseEntryInput, CreateLocaleInput, CreateReleaseInput, CreateWorkflowStageInput,
    InsertAuditLogInput, ServiceContainer, UpdateLocaleInput, UpdateReleaseInput,
    UpdateWorkflowStageInput, UpsertPreviewConfigInput, UpsertWorkflowEntryInput,
};
use futures_util::stream::{self, Stream};
use serde::Deserialize;
use std::convert::Infallible;
use std::time::Duration;

const AUDIT_PAYLOAD_STORE_MAX_BYTES: usize = 8 * 1024; // 8 KB stored summary cap in audit_logs

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewQueryParams {
    locale: Option<String>,
    status: Option<String>,
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillFromLocaleBody {
    source_locale: String,
    target_locale: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuditQueryParams {
    action: Option<String>,
    actor_id: Option<String>,
    target_uid: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    page: Option<u64>,
    page_size: Option<u64>,
}

pub fn governance_routes() -> Router<ServiceContainer> {
    Router::new()
        // --- Internationalization (i18n) ---
        .route("/api/i18n/locales", get(public_list_locales_handler))
        .route(
            "/admin/i18n/locales",
            get(admin_list_locales_handler).post(admin_create_locale_handler),
        )
        .route(
            "/admin/i18n/locales/{id}",
            put(admin_update_locale_handler).delete(admin_delete_locale_handler),
        )
        .route(
            "/admin/content-manager/{uid}/{document_id}/locales",
            get(admin_list_doc_locales_handler),
        )
        .route(
            "/admin/content-manager/{uid}/{document_id}/fill-from-locale",
            post(admin_fill_from_locale_handler),
        )
        // --- Preview & Live SSE ---
        .route("/admin/preview/configs", get(admin_list_preview_configs_handler))
        .route(
            "/admin/preview/configs/{uid}",
            put(admin_upsert_preview_config_handler).delete(admin_delete_preview_config_handler),
        )
        .route(
            "/admin/preview/resolve/{uid}/{document_id}",
            get(admin_resolve_preview_handler),
        )
        .route(
            "/admin/preview/render/{uid}/{document_id}",
            get(admin_render_builtin_preview_handler),
        )
        .route("/api/preview/events", get(preview_sse_events_handler))
        // --- Releases ---
        .route(
            "/admin/releases",
            get(admin_list_releases_handler).post(admin_create_release_handler),
        )
        .route(
            "/admin/releases/{id}",
            get(admin_get_release_handler)
                .put(admin_update_release_handler)
                .delete(admin_delete_release_handler),
        )
        .route(
            "/admin/releases/{id}/entries",
            post(admin_add_release_entry_handler),
        )
        .route(
            "/admin/releases/{id}/entries/{entry_id}",
            delete(admin_remove_release_entry_handler),
        )
        .route(
            "/admin/releases/{id}/publish",
            post(admin_publish_release_handler),
        )
        // --- Review Workflows ---
        .route(
            "/admin/workflows/stages",
            get(admin_list_workflow_stages_handler).post(admin_create_workflow_stage_handler),
        )
        .route(
            "/admin/workflows/stages/{id}",
            put(admin_update_workflow_stage_handler).delete(admin_delete_workflow_stage_handler),
        )
        .route(
            "/admin/workflows/entries/{document_id}",
            get(admin_get_workflow_entry_handler).put(admin_update_workflow_entry_handler),
        )
        // --- Audit Logs ---
        .route("/admin/audit-logs", get(admin_list_audit_logs_handler))
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
// Internationalization Handlers
// ============================================================================

async fn public_list_locales_handler(
    State(services): State<ServiceContainer>,
) -> Result<Json<Vec<LocaleDto>>, HttpApiError> {
    let locales = services.governance.list_locales().await?;
    Ok(Json(locales))
}

async fn admin_list_locales_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<LocaleDto>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::i18n.locale.read", None)?;
    let locales = services.governance.list_locales().await?;
    Ok(Json(ApiResponse::new(locales)))
}

async fn admin_create_locale_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(input): Json<CreateLocaleInput>,
) -> Result<Json<ApiResponse<LocaleDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::i18n.locale.update", None)?;
    let created = services.governance.create_locale(input).await?;
    Ok(Json(ApiResponse::new(created)))
}

async fn admin_update_locale_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<i64>,
    Json(input): Json<UpdateLocaleInput>,
) -> Result<Json<ApiResponse<LocaleDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::i18n.locale.update", None)?;
    let updated = services.governance.update_locale(id, input).await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn admin_delete_locale_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<LocaleDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::i18n.locale.update", None)?;
    let deleted = services.governance.delete_locale(id).await?;
    Ok(Json(ApiResponse::new(deleted)))
}

async fn admin_list_doc_locales_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path((uid, document_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Vec<DocumentLocaleStateDto>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-manager.explorer.read", Some(&uid))?;
    let states = services
        .governance
        .list_document_locales(&uid, &document_id)
        .await?;
    Ok(Json(ApiResponse::new(states)))
}

async fn admin_fill_from_locale_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path((uid, document_id)): Path<(String, String)>,
    Json(body): Json<FillFromLocaleBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(
        &actor,
        "plugin::content-manager.explorer.update",
        Some(&uid),
    )?;
    let doc = services
        .governance
        .fill_from_locale(
            &uid,
            &document_id,
            &body.source_locale,
            &body.target_locale,
            actor_id_str(&actor),
        )
        .await?;
    Ok(Json(ApiResponse::new(doc)))
}

// ============================================================================
// Preview & Live SSE Handlers
// ============================================================================

async fn admin_list_preview_configs_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(_actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<PreviewConfigDto>>>, HttpApiError> {
    let configs = services.governance.list_preview_configs().await?;
    Ok(Json(ApiResponse::new(configs)))
}

async fn admin_upsert_preview_config_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
    Json(input): Json<UpsertPreviewConfigInput>,
) -> Result<Json<ApiResponse<PreviewConfigDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::preview.update", None)?;
    let saved = services
        .governance
        .upsert_preview_config(&uid, input)
        .await?;
    Ok(Json(ApiResponse::new(saved)))
}

async fn admin_delete_preview_config_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::preview.update", None)?;
    services.governance.delete_preview_config(&uid).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": uid }))))
}

async fn admin_resolve_preview_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path((uid, document_id)): Path<(String, String)>,
    Query(q): Query<PreviewQueryParams>,
) -> Result<Json<ApiResponse<ResolvedPreviewDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-manager.explorer.read", Some(&uid))?;
    let locale = match q.locale.as_deref() {
        Some(l) if !l.is_empty() => l.to_string(),
        _ => services.governance.default_locale().await?,
    };
    let status = q.status.as_deref().unwrap_or("draft");
    let resolved = services
        .governance
        .resolve_preview_url(&uid, &document_id, &locale, status)
        .await?;
    Ok(Json(ApiResponse::new(resolved)))
}

/// Built-in zero-config HTML preview renderer (allows `SAMEORIGIN` iframe embedding).
async fn admin_render_builtin_preview_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Path((uid, document_id)): Path<(String, String)>,
    Query(q): Query<PreviewQueryParams>,
) -> Result<Response, HttpApiError> {
    let effective_actor = if actor.is_admin() {
        actor
    } else if let Some(tok) = q.token.as_deref().filter(|s| !s.is_empty()) {
        let resolved = services.auth.resolve_bearer_actor(tok).await?;
        if !resolved.is_admin() {
            return Err(HttpApiError(ApiError::Unauthorized(
                "Administrator authentication required for built-in preview".into(),
            )));
        }
        resolved
    } else {
        return Err(HttpApiError(ApiError::Unauthorized(
            "Administrator authentication required for built-in preview".into(),
        )));
    };
    enforce_permission(
        &effective_actor,
        "plugin::content-manager.explorer.read",
        Some(&uid),
    )?;

    let schema = services.schema.get_schema(&uid).await?;
    let locale = match q.locale.as_deref() {
        Some(l) if !l.is_empty() => l.to_string(),
        _ => services.governance.default_locale().await?,
    };
    let doc_status = match q.status.as_deref() {
        Some("published") => DocumentStatus::Published,
        _ => {
            if schema.draft_and_publish {
                DocumentStatus::Draft
            } else {
                DocumentStatus::Published
            }
        }
    };

    let doc = services
        .content
        .find_one(
            &schema,
            &document_id,
            &locale,
            doc_status,
            &PopulateSpec::All,
            &[],
            false,
        )
        .await?;

    let pretty_json = escape_html(
        &serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".to_string()),
    );
    let safe_title = escape_html(&schema.display_name);
    let safe_uid = escape_html(&schema.uid);
    let safe_doc_id = escape_html(&document_id);
    let safe_locale = escape_html(&locale);
    let safe_status = doc_status.as_str();

    let mut field_rows = String::new();
    if let Some(obj) = doc.as_object() {
        for f in &schema.fields {
            if let Some(val) = obj.get(&f.name) {
                let rendered = match val {
                    serde_json::Value::String(s) => escape_html(s),
                    other => format!(
                        "<code>{}</code>",
                        escape_html(&other.to_string())
                    ),
                };
                field_rows.push_str(&format!(
                    "<div class=\"row\"><div class=\"k\">{}</div><div class=\"v\">{}</div></div>",
                    escape_html(&f.name),
                    rendered
                ));
            }
        }
    }

    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>Preview · {safe_title}</title>
<style>
  body {{ font-family: ui-sans-serif, system-ui, -apple-system, sans-serif; margin: 0; padding: 20px; background: #fafafa; color: #18181b; font-size: 13px; }}
  .hdr {{ display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid #e4e4e7; padding-bottom: 10px; margin-bottom: 16px; }}
  .pill {{ font-family: ui-monospace, monospace; font-size: 11px; background: #eef2ff; color: #3730a3; padding: 2px 8px; border-radius: 4px; border: 1px solid #c7d2fe; }}
  .card {{ background: #fff; border: 1px solid #e4e4e7; border-radius: 6px; padding: 14px; margin-bottom: 14px; }}
  .row {{ display: grid; grid-template-columns: 140px 1fr; gap: 10px; padding: 7px 0; border-bottom: 1px solid #f4f4f5; }}
  .row:last-child {{ border-bottom: 0; }}
  .k {{ font-family: ui-monospace, monospace; font-size: 11px; color: #71717a; }}
  .v {{ color: #18181b; word-break: break-word; }}
  pre {{ background: #09090b; color: #e4e4e7; padding: 12px; border-radius: 6px; overflow-x: auto; font-size: 11px; line-height: 1.45; margin: 0; }}
</style>
</head>
<body>
  <div class="hdr">
    <div>
      <strong>{safe_title}</strong>
      <div style="font-family:monospace;font-size:11px;color:#71717a">{safe_uid} · {safe_doc_id}</div>
    </div>
    <div style="display:flex;gap:6px">
      <span class="pill">locale:{safe_locale}</span>
      <span class="pill">status:{safe_status}</span>
    </div>
  </div>
  <div class="card">{field_rows}</div>
  <details>
    <summary style="cursor:pointer;font-size:11px;color:#52525b;margin-bottom:8px">Raw populated JSON</summary>
    <pre>{pretty_json}</pre>
  </details>
</body>
</html>"#
    );

    let mut res = Html(html).into_response();
    res.headers_mut().insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("SAMEORIGIN"),
    );
    Ok(res)
}

/// Server-Sent Events (`/api/preview/events`) streaming `document.changed` events in real time.
async fn preview_sse_events_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Query(q): Query<PreviewQueryParams>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, HttpApiError> {
    let authorized = if matches!(actor, Actor::Public { .. }) {
        if let Some(tok) = q.token.as_deref().filter(|s| !s.is_empty()) {
            let resolved = services.auth.resolve_bearer_actor(tok).await?;
            !matches!(resolved, Actor::Public { .. })
        } else {
            false
        }
    } else {
        true
    };

    if !authorized {
        return Err(HttpApiError(ApiError::Unauthorized(
            "Authentication required for live preview event stream".into(),
        )));
    }

    let rx = services.pubsub.subscribe_local();
    let event_stream = stream::unfold(rx, |mut receiver| async move {
        loop {
            match receiver.recv().await {
                Ok(InvalidationEvent::DocumentChanged { uid, document_id }) => {
                    let data = serde_json::json!({
                        "uid": uid,
                        "documentId": document_id,
                    })
                    .to_string();
                    let ev = Event::default().event("document.changed").data(data);
                    return Some((Ok(ev), receiver));
                }
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    Ok(Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

// ============================================================================
// Releases Handlers
// ============================================================================

async fn admin_list_releases_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<ReleaseDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::releases.read", None)?;
    let list = services.governance.list_releases().await?;
    Ok(Json(ApiResponse::new(list)))
}

async fn admin_get_release_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ReleaseDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::releases.read", None)?;
    let rel = services.governance.get_release(&id).await?;
    Ok(Json(ApiResponse::new(rel)))
}

async fn admin_create_release_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(input): Json<CreateReleaseInput>,
) -> Result<Json<ApiResponse<ReleaseDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::releases.update", None)?;
    let created = services
        .governance
        .create_release(input, actor_id_str(&actor))
        .await?;
    Ok(Json(ApiResponse::new(created)))
}

async fn admin_update_release_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
    Json(input): Json<UpdateReleaseInput>,
) -> Result<Json<ApiResponse<ReleaseDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::releases.update", None)?;
    let updated = services.governance.update_release(&id, input).await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn admin_delete_release_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::releases.update", None)?;
    services.governance.delete_release(&id).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": id }))))
}

async fn admin_add_release_entry_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
    Json(input): Json<AddReleaseEntryInput>,
) -> Result<Json<ApiResponse<ReleaseDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::releases.update", None)?;
    enforce_permission(
        &actor,
        "plugin::content-manager.explorer.publish",
        Some(input.content_type_uid.trim()),
    )?;
    let updated = services.governance.add_release_entry(&id, input).await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn admin_remove_release_entry_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path((id, entry_id)): Path<(String, i64)>,
) -> Result<Json<ApiResponse<ReleaseDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::releases.update", None)?;
    let updated = services
        .governance
        .remove_release_entry(&id, entry_id)
        .await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn admin_publish_release_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ReleaseDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::releases.update", None)?;
    let release = services.governance.get_release(&id).await?;
    for entry in &release.entries {
        enforce_permission(
            &actor,
            "plugin::content-manager.explorer.publish",
            Some(&entry.content_type_uid),
        )?;
    }
    let executed = services
        .governance
        .execute_release(&id, actor_id_str(&actor))
        .await?;
    services
        .integrations
        .emit_webhook_event(
            "release.publish",
            None,
            serde_json::to_value(&executed).unwrap_or_default(),
        )
        .await;
    Ok(Json(ApiResponse::new(executed)))
}

// ============================================================================
// Review Workflows Handlers
// ============================================================================

async fn admin_list_workflow_stages_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<WorkflowStageDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::workflows.read", None)?;
    let stages = services.governance.list_workflow_stages().await?;
    Ok(Json(ApiResponse::new(stages)))
}

async fn admin_create_workflow_stage_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(input): Json<CreateWorkflowStageInput>,
) -> Result<Json<ApiResponse<WorkflowStageDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::workflows.update", None)?;
    let created = services.governance.create_workflow_stage(input).await?;
    Ok(Json(ApiResponse::new(created)))
}

async fn admin_update_workflow_stage_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
    Json(input): Json<UpdateWorkflowStageInput>,
) -> Result<Json<ApiResponse<WorkflowStageDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::workflows.update", None)?;
    let updated = services.governance.update_workflow_stage(&id, input).await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn admin_delete_workflow_stage_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::workflows.update", None)?;
    services.governance.delete_workflow_stage(&id).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": id }))))
}

async fn admin_get_workflow_entry_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(document_id): Path<String>,
    Query(q): Query<PreviewQueryParams>,
) -> Result<Json<ApiResponse<WorkflowEntryDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::workflows.read", None)?;
    let entry = services
        .governance
        .get_workflow_entry(&document_id, q.locale.as_deref())
        .await?;
    Ok(Json(ApiResponse::new(entry)))
}

async fn admin_update_workflow_entry_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(document_id): Path<String>,
    Json(input): Json<UpsertWorkflowEntryInput>,
) -> Result<Json<ApiResponse<WorkflowEntryDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-manager.explorer.update", None)?;
    let entry = services
        .governance
        .update_workflow_entry(&actor, &document_id, input)
        .await?;
    Ok(Json(ApiResponse::new(entry)))
}

// ============================================================================
// Audit Logs Handler & Middleware
// ============================================================================

async fn admin_list_audit_logs_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Query(q): Query<AuditQueryParams>,
) -> Result<Json<ApiResponse<Vec<AuditLogDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::audit-logs.read", None)?;
    let filter = AuditLogFilter {
        action: q.action,
        actor_id: q.actor_id,
        target_uid: q.target_uid,
        date_from: q.date_from,
        date_to: q.date_to,
        page: q.page.unwrap_or(1),
        page_size: q.page_size.unwrap_or(30),
    };
    let (logs, pagination) = services.governance.list_audit_logs(&filter).await?;
    Ok(Json(ApiResponse::with_pagination(logs, pagination)))
}

/// Axum middleware recording all `/admin/*` state-mutating requests (`POST`, `PUT`, `PATCH`, `DELETE`)
/// with automatic secret redaction and actor attribution.
pub async fn audit_log_middleware(
    State(services): State<ServiceContainer>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let is_auditable = path.starts_with("/admin/")
        && matches!(
            method,
            Method::POST | Method::PUT | Method::PATCH | Method::DELETE
        )
        && path != "/admin/refresh-token";

    if !is_auditable {
        return next.run(req).await;
    }

    let auth_hdr = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let ip_address = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string());
    let user_agent = req
        .headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(200).collect::<String>());
    let is_json = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.contains("application/json"));

    let (parts, body) = req.into_parts();
    let max_bytes = services.config.max_body_bytes.max(64 * 1024);
    let (rebuilt_req, captured_json) = if is_json {
        match axum::body::to_bytes(body, max_bytes).await {
            Ok(bytes) => {
                let parsed = if bytes.len() <= AUDIT_PAYLOAD_STORE_MAX_BYTES {
                    serde_json::from_slice::<serde_json::Value>(&bytes)
                        .ok()
                        .map(|v| redact_sensitive_json(v, path.contains("/sso/")))
                } else {
                    Some(serde_json::json!({
                        "_truncated": true,
                        "byteLength": bytes.len()
                    }))
                };
                (Request::from_parts(parts, Body::from(bytes)), parsed)
            }
            Err(_) => {
                return StatusCode::PAYLOAD_TOO_LARGE.into_response();
            }
        }
    } else {
        (Request::from_parts(parts, body), None)
    };

    let response = next.run(rebuilt_req).await;
    let status_code = response.status().as_u16();
    if status_code >= 500 {
        return response;
    }

    let (actor_type, actor_id) = resolve_audit_actor(&services, auth_hdr.as_deref()).await;
    let (action, target_uid, target_id) = classify_admin_action(&method, &path);

    let payload_json = serde_json::json!({
        "method": method.as_str(),
        "path": path,
        "status": status_code,
        "body": captured_json,
    })
    .to_string();

    let _ = services
        .governance
        .record_audit(InsertAuditLogInput {
            action,
            actor_type,
            actor_id,
            target_uid,
            target_id,
            ip_address,
            user_agent,
            payload_json,
        })
        .await;

    response
}

async fn resolve_audit_actor(
    services: &ServiceContainer,
    auth_hdr: Option<&str>,
) -> (String, Option<String>) {
    let Some(raw) = auth_hdr else {
        return ("anonymous".into(), None);
    };
    let Some(tok) = raw.strip_prefix("Bearer ").or_else(|| raw.strip_prefix("bearer ")) else {
        return ("anonymous".into(), None);
    };
    match services.auth.resolve_bearer_actor(tok).await {
        Ok(Actor::AdminUser { email, .. }) => ("admin_user".into(), Some(email)),
        Ok(Actor::AdminToken { name, .. }) => ("admin_token".into(), Some(name)),
        Ok(Actor::ApiToken { name, .. }) => ("api_token".into(), Some(name)),
        _ => ("anonymous".into(), None),
    }
}

fn classify_admin_action(method: &Method, path: &str) -> (String, Option<String>, Option<String>) {
    let segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    // segs[0] == "admin"
    match segs.get(1).copied() {
        Some("login") => ("auth.login".into(), None, None),
        Some("logout") => ("auth.logout".into(), None, None),
        Some("register-admin") => ("auth.register_first_admin".into(), None, None),
        Some("content-manager") => {
            let uid = segs.get(2).map(|s| percent_decode(s));
            let doc_id = segs.get(3).map(|s| (*s).to_string());
            let sub = segs.get(4).copied();
            let act = match (method, sub, segs.get(3).copied()) {
                (&Method::POST, Some("publish"), _) => "entry.publish",
                (&Method::POST, Some("unpublish"), _) => "entry.unpublish",
                (&Method::POST, Some("fill-from-locale"), _) => "entry.fill_locale",
                (&Method::POST, _, Some("bulk-publish")) => "entry.bulk_publish",
                (&Method::POST, _, Some("bulk-delete")) => "entry.bulk_delete",
                (&Method::POST, _, _) => "entry.create",
                (&Method::PUT, _, _) => "entry.update",
                (&Method::DELETE, _, _) => "entry.delete",
                _ => "entry.mutate",
            };
            (act.into(), uid, doc_id)
        }
        Some("content-types") => {
            let uid = segs.get(2).map(|s| percent_decode(s));
            let act = match *method {
                Method::POST => "schema.create",
                Method::PUT => "schema.update",
                Method::DELETE => "schema.delete",
                _ => "schema.mutate",
            };
            (act.into(), uid, None)
        }
        Some("media") => {
            let id = segs.get(3).map(|s| (*s).to_string());
            let act = match (method, segs.get(2).copied(), segs.get(4).copied()) {
                (&Method::POST, Some("upload"), _) => "media.upload",
                (&Method::POST, Some("files"), Some("crop")) => "media.crop",
                (&Method::PUT, Some("files"), _) => "media.update",
                (&Method::DELETE, Some("files"), _) => "media.delete",
                (_, Some("folders"), _) => "media.folder",
                _ => "media.mutate",
            };
            (act.into(), None, id)
        }
        Some("i18n") => ("i18n.locale.mutate".into(), None, segs.get(3).map(|s| (*s).to_string())),
        Some("releases") => {
            let id = segs.get(2).map(|s| (*s).to_string());
            let act = if segs.get(3).copied() == Some("publish") {
                "release.publish"
            } else {
                match *method {
                    Method::POST => "release.create_or_entry",
                    Method::PUT => "release.update",
                    Method::DELETE => "release.delete",
                    _ => "release.mutate",
                }
            };
            (act.into(), None, id)
        }
        Some("workflows") => (
            "workflow.mutate".into(),
            None,
            segs.get(3).map(|s| (*s).to_string()),
        ),
        Some("preview") => (
            "preview.config".into(),
            segs.get(3).map(|s| percent_decode(s)),
            None,
        ),
        Some("users") => (
            format!("admin_user.{}", method.as_str().to_lowercase()),
            None,
            segs.get(2).map(|s| (*s).to_string()),
        ),
        Some("roles") => (
            format!("admin_role.{}", method.as_str().to_lowercase()),
            None,
            segs.get(2).map(|s| (*s).to_string()),
        ),
        Some("api-tokens") | Some("admin-tokens") => (
            format!("token.{}", method.as_str().to_lowercase()),
            None,
            segs.get(2).map(|s| (*s).to_string()),
        ),
        Some("plugins") => (
            format!("plugin.{}", method.as_str().to_lowercase()),
            None,
            segs.get(2).map(|s| (*s).to_string()),
        ),
        Some("custom-fields") => (
            format!("custom_field.{}", method.as_str().to_lowercase()),
            segs.get(2).map(|s| percent_decode(s)),
            None,
        ),
        Some("sso") => (
            format!("sso.{}", method.as_str().to_lowercase()),
            None,
            segs.get(3).map(|s| (*s).to_string()),
        ),
        Some("auth") if segs.get(2).copied() == Some("sso") => (
            "sso.callback".into(),
            None,
            segs.get(3).map(|s| (*s).to_string()),
        ),
        Some("ai") => (
            format!("ai.{}", segs.get(2).copied().unwrap_or("mutate")),
            None,
            None,
        ),
        _ => (format!("admin.{}", method.as_str().to_lowercase()), None, None),
    }
}

fn redact_sensitive_json(val: serde_json::Value, redact_oidc_code: bool) -> serde_json::Value {
    const SENSITIVE_KEYS: &[&str] = &[
        "password",
        "currentpassword",
        "newpassword",
        "secret",
        "token",
        "refreshtoken",
        "accesskey",
        "apikey",
        "api_key",
        "authorization",
        "wasmbase64",
    ];
    match val {
        serde_json::Value::Object(map) => {
            let mut clean = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                let lower = k.to_ascii_lowercase();
                if SENSITIVE_KEYS.iter().any(|sk| lower.contains(sk))
                    || (redact_oidc_code && lower == "code")
                {
                    clean.insert(k, serde_json::Value::String("[REDACTED]".into()));
                } else {
                    clean.insert(k, redact_sensitive_json(v, redact_oidc_code));
                }
            }
            serde_json::Value::Object(clean)
        }
        serde_json::Value::Array(arr) => serde_json::Value::Array(
            arr.into_iter()
                .map(|v| redact_sensitive_json(v, redact_oidc_code))
                .collect(),
        ),
        other => other,
    }
}

fn percent_decode(s: &str) -> String {
    s.replace("%3A", ":").replace("%3a", ":")
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}
