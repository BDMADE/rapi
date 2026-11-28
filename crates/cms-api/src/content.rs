use crate::auth::{enforce_permission, AuthActor, RequireAdmin};
use crate::error::HttpApiError;
use crate::qs::ContentQueryParams;
use axum::extract::{Path, RawQuery, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use cms_core::{
    Actor, ApiError, ApiResponse, ContentKind, ContentVersionDto, DocumentStatus, PopulateSpec,
};
use cms_services::ServiceContainer;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ContentWriteBody {
    pub data: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkIdsBody {
    pub document_ids: Vec<String>,
    #[serde(default)]
    pub locale: Option<String>,
}

pub fn content_api_routes() -> Router<ServiceContainer> {
    Router::new()
        // Strapi v5 Public / Token REST Content API
        .route(
            "/api/{slug}",
            get(rest_find_or_single_handler)
                .post(rest_create_handler)
                .put(rest_put_single_handler)
                .delete(rest_delete_single_handler),
        )
        .route(
            "/api/{slug}/{document_id}",
            get(rest_find_one_handler)
                .put(rest_update_handler)
                .delete(rest_delete_handler),
        )
        // Admin Content Manager API
        .route(
            "/admin/content-manager/{uid}",
            get(admin_list_entries_handler).post(admin_create_entry_handler),
        )
        .route(
            "/admin/content-manager/{uid}/single",
            get(admin_get_single_handler).put(admin_put_single_handler),
        )
        .route(
            "/admin/content-manager/{uid}/bulk-publish",
            post(admin_bulk_publish_handler),
        )
        .route(
            "/admin/content-manager/{uid}/bulk-delete",
            post(admin_bulk_delete_handler),
        )
        .route(
            "/admin/content-manager/{uid}/{document_id}",
            get(admin_get_entry_handler)
                .put(admin_update_entry_handler)
                .delete(admin_delete_entry_handler),
        )
        .route(
            "/admin/content-manager/{uid}/{document_id}/publish",
            post(admin_publish_handler),
        )
        .route(
            "/admin/content-manager/{uid}/{document_id}/unpublish",
            post(admin_unpublish_handler),
        )
        .route(
            "/admin/content-manager/{uid}/{document_id}/versions",
            get(admin_list_versions_handler),
        )
        .route(
            "/admin/content-manager/{uid}/{document_id}/versions/{version_id}/restore",
            post(admin_restore_version_handler),
        )
}

fn actor_id(actor: &Actor) -> Option<&str> {
    match actor {
        Actor::AdminUser { user_id, .. } | Actor::EndUser { user_id, .. } => Some(user_id.as_str()),
        Actor::AdminToken { token_id, .. } | Actor::ApiToken { token_id, .. } => {
            Some(token_id.as_str())
        }
        Actor::Public { .. } => None,
    }
}

fn check_draft_access(actor: &Actor, status: Option<DocumentStatus>) -> Result<(), HttpApiError> {
    if status == Some(DocumentStatus::Draft) && matches!(actor, Actor::Public { .. }) {
        return Err(HttpApiError(ApiError::Forbidden(
            "Authentication required to read draft documents".into(),
        )));
    }
    Ok(())
}

// ============================================================================
// Strapi v5 REST Content API Handlers (`/api/*`)
// ============================================================================

async fn rest_find_or_single_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Path(slug): Path<String>,
    RawQuery(raw_qs): RawQuery,
) -> Result<Json<serde_json::Value>, HttpApiError> {
    let schema = services.content.resolve_api_slug(&slug).await?;
    enforce_permission(&actor, &format!("{}.find", schema.uid), Some(&schema.uid))?;

    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    check_draft_access(&actor, params.status)?;
    let mut doc_query = params.into_document_query();
    let resolved_locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, doc_query.locale.as_deref(), false)
        .await?;
    doc_query.locale = Some(resolved_locale.clone());

    if schema.kind == ContentKind::SingleType {
        let status = doc_query.status.unwrap_or(DocumentStatus::Published);
        let doc = services
            .content
            .find_single(
                &schema,
                &resolved_locale,
                status,
                &doc_query.populate,
                &doc_query.fields,
                true,
            )
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Single type '{slug}' has no published document")))?;
        let value = serde_json::to_value(ApiResponse::new(doc))
            .map_err(|e| HttpApiError(ApiError::Internal(format!("serialize response: {e}"))))?;
        return Ok(Json(value));
    }

    let (items, pagination) = services
        .content
        .find_many_public(&schema, &doc_query)
        .await?;
    let value = serde_json::to_value(ApiResponse::with_pagination(items, pagination))
        .map_err(|e| HttpApiError(ApiError::Internal(format!("serialize response: {e}"))))?;
    Ok(Json(value))
}

async fn rest_find_one_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Path((slug, document_id)): Path<(String, String)>,
    RawQuery(raw_qs): RawQuery,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    let schema = services.content.resolve_api_slug(&slug).await?;
    enforce_permission(
        &actor,
        &format!("{}.findOne", schema.uid),
        Some(&schema.uid),
    )?;

    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    check_draft_access(&actor, params.status)?;
    let doc_query = params.into_document_query();
    let locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, doc_query.locale.as_deref(), false)
        .await?;
    let status = doc_query.status.unwrap_or(DocumentStatus::Published);

    let doc = services
        .content
        .find_one(
            &schema,
            &document_id,
            &locale,
            status,
            &doc_query.populate,
            &doc_query.fields,
            true,
        )
        .await?;
    Ok(Json(ApiResponse::new(doc)))
}

async fn rest_create_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Path(slug): Path<String>,
    RawQuery(raw_qs): RawQuery,
    Json(body): Json<ContentWriteBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    let schema = services.content.resolve_api_slug(&slug).await?;
    if schema.kind != ContentKind::CollectionType {
        return Err(HttpApiError(ApiError::BadRequest(
            "Use PUT /api/:singularApiId to create or update a Single Type".into(),
        )));
    }
    enforce_permission(&actor, &format!("{}.create", schema.uid), Some(&schema.uid))?;

    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, params.locale.as_deref(), true)
        .await?;
    // Strapi v5 REST Content API defaults `POST /api/:plural` to `status=published` unless `?status=draft` is specified
    let target_status = params.status.unwrap_or(DocumentStatus::Published);
    if schema.draft_and_publish && target_status == DocumentStatus::Published {
        services.governance.enforce_new_entry_publish_gate().await?;
    }

    let created = services
        .content
        .create_entry(
            &schema,
            &locale,
            target_status,
            &body.data,
            actor_id(&actor),
            true,
        )
        .await?;
    services
        .integrations
        .emit_webhook_event("entry.create", Some(&schema.uid), created.clone())
        .await;
    Ok(Json(ApiResponse::new(created)))
}

async fn rest_update_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Path((slug, document_id)): Path<(String, String)>,
    RawQuery(raw_qs): RawQuery,
    Json(body): Json<ContentWriteBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    let schema = services.content.resolve_api_slug(&slug).await?;
    enforce_permission(&actor, &format!("{}.update", schema.uid), Some(&schema.uid))?;

    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, params.locale.as_deref(), true)
        .await?;
    let target_status = params.status.unwrap_or(DocumentStatus::Published);
    if schema.draft_and_publish && target_status == DocumentStatus::Published {
        services
            .governance
            .enforce_publish_workflow_gate(&document_id, &locale)
            .await?;
    }

    let updated = services
        .content
        .update_entry(
            &schema,
            &document_id,
            &locale,
            target_status,
            &body.data,
            actor_id(&actor),
            true,
        )
        .await?;
    services
        .integrations
        .emit_webhook_event("entry.update", Some(&schema.uid), updated.clone())
        .await;
    Ok(Json(ApiResponse::new(updated)))
}

async fn rest_delete_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Path((slug, document_id)): Path<(String, String)>,
    RawQuery(raw_qs): RawQuery,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    let schema = services.content.resolve_api_slug(&slug).await?;
    enforce_permission(&actor, &format!("{}.delete", schema.uid), Some(&schema.uid))?;

    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    services
        .content
        .delete_entry(&schema, &document_id, params.locale.as_deref())
        .await?;
    services
        .integrations
        .emit_webhook_event(
            "entry.delete",
            Some(&schema.uid),
            serde_json::json!({ "documentId": document_id }),
        )
        .await;

    Ok(Json(ApiResponse::new(serde_json::json!({
        "documentId": document_id
    }))))
}

async fn rest_put_single_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Path(slug): Path<String>,
    RawQuery(raw_qs): RawQuery,
    Json(body): Json<ContentWriteBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    let schema = services.content.resolve_api_slug(&slug).await?;
    if schema.kind != ContentKind::SingleType {
        return Err(HttpApiError(ApiError::BadRequest(
            "PUT /api/:slug without documentId is only valid for Single Types".into(),
        )));
    }
    enforce_permission(&actor, &format!("{}.update", schema.uid), Some(&schema.uid))?;

    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, params.locale.as_deref(), true)
        .await?;
    let target_status = params.status.unwrap_or(DocumentStatus::Published);

    let doc = services
        .content
        .upsert_single(
            &schema,
            &locale,
            target_status,
            &body.data,
            actor_id(&actor),
            true,
        )
        .await?;
    Ok(Json(ApiResponse::new(doc)))
}

async fn rest_delete_single_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Path(slug): Path<String>,
    RawQuery(raw_qs): RawQuery,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    let schema = services.content.resolve_api_slug(&slug).await?;
    if schema.kind != ContentKind::SingleType {
        return Err(HttpApiError(ApiError::BadRequest(
            "DELETE /api/:slug without documentId is only valid for Single Types".into(),
        )));
    }
    enforce_permission(&actor, &format!("{}.delete", schema.uid), Some(&schema.uid))?;

    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, params.locale.as_deref(), false)
        .await?;
    let check_status = if schema.draft_and_publish {
        DocumentStatus::Draft
    } else {
        DocumentStatus::Published
    };
    let Some(existing) = services
        .content
        .find_single(&schema, &locale, check_status, &PopulateSpec::None, &[], false)
        .await?
    else {
        return Err(HttpApiError(ApiError::NotFound(
            "Single type document does not exist".into(),
        )));
    };
    let doc_id = existing["documentId"].as_str().unwrap_or("").to_string();
    services
        .content
        .delete_entry(&schema, &doc_id, params.locale.as_deref())
        .await?;

    Ok(Json(ApiResponse::new(serde_json::json!({
        "documentId": doc_id
    }))))
}

// ============================================================================
// Admin Content Manager Handlers (`/admin/content-manager/*`)
// ============================================================================

async fn admin_list_entries_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
    RawQuery(raw_qs): RawQuery,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-manager.explorer.read", Some(&uid))?;
    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let mut query = params.into_document_query();
    if query.locale.is_none() {
        query.locale = Some(services.governance.default_locale().await?);
    }
    if query.populate == PopulateSpec::None {
        query.populate = PopulateSpec::All;
    }

    let (items, pagination) = services.content.find_admin_entries(&uid, &query).await?;
    Ok(Json(ApiResponse::with_pagination(items, pagination)))
}

async fn admin_create_entry_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
    RawQuery(raw_qs): RawQuery,
    Json(body): Json<ContentWriteBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(
        &actor,
        "plugin::content-manager.explorer.create",
        Some(&uid),
    )?;
    let schema = services.schema.get_schema(&uid).await?;
    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, params.locale.as_deref(), true)
        .await?;
    let target_status = params.status.unwrap_or(if schema.draft_and_publish {
        DocumentStatus::Draft
    } else {
        DocumentStatus::Published
    });
    if schema.draft_and_publish && target_status == DocumentStatus::Published {
        services.governance.enforce_new_entry_publish_gate().await?;
    }

    let doc = services
        .content
        .create_entry(
            &schema,
            &locale,
            target_status,
            &body.data,
            actor_id(&actor),
            false,
        )
        .await?;
    services
        .integrations
        .emit_webhook_event("entry.create", Some(&schema.uid), doc.clone())
        .await;
    Ok(Json(ApiResponse::new(doc)))
}

async fn admin_get_single_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
    RawQuery(raw_qs): RawQuery,
) -> Result<Json<ApiResponse<Option<serde_json::Value>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-manager.explorer.read", Some(&uid))?;
    let schema = services.schema.get_schema(&uid).await?;
    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, params.locale.as_deref(), false)
        .await?;
    let status = params.status.unwrap_or(if schema.draft_and_publish {
        DocumentStatus::Draft
    } else {
        DocumentStatus::Published
    });

    let doc = services
        .content
        .find_single(&schema, &locale, status, &PopulateSpec::All, &[], false)
        .await?;
    Ok(Json(ApiResponse::new(doc)))
}

async fn admin_put_single_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
    RawQuery(raw_qs): RawQuery,
    Json(body): Json<ContentWriteBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(
        &actor,
        "plugin::content-manager.explorer.update",
        Some(&uid),
    )?;
    let schema = services.schema.get_schema(&uid).await?;
    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, params.locale.as_deref(), true)
        .await?;
    let status = params.status.unwrap_or(if schema.draft_and_publish {
        DocumentStatus::Draft
    } else {
        DocumentStatus::Published
    });

    let doc = services
        .content
        .upsert_single(
            &schema,
            &locale,
            status,
            &body.data,
            actor_id(&actor),
            false,
        )
        .await?;
    Ok(Json(ApiResponse::new(doc)))
}

async fn admin_get_entry_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path((uid, document_id)): Path<(String, String)>,
    RawQuery(raw_qs): RawQuery,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-manager.explorer.read", Some(&uid))?;
    let schema = services.schema.get_schema(&uid).await?;
    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, params.locale.as_deref(), false)
        .await?;
    let status = params.status.unwrap_or(if schema.draft_and_publish {
        DocumentStatus::Draft
    } else {
        DocumentStatus::Published
    });

    let doc = services
        .content
        .find_one(
            &schema,
            &document_id,
            &locale,
            status,
            &PopulateSpec::All,
            &[],
            false,
        )
        .await?;
    Ok(Json(ApiResponse::new(doc)))
}

async fn admin_update_entry_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path((uid, document_id)): Path<(String, String)>,
    RawQuery(raw_qs): RawQuery,
    Json(body): Json<ContentWriteBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(
        &actor,
        "plugin::content-manager.explorer.update",
        Some(&uid),
    )?;
    let schema = services.schema.get_schema(&uid).await?;
    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = services
        .governance
        .resolve_and_validate_locale(schema.i18n_enabled, params.locale.as_deref(), true)
        .await?;
    let status = params.status.unwrap_or(if schema.draft_and_publish {
        DocumentStatus::Draft
    } else {
        DocumentStatus::Published
    });
    if schema.draft_and_publish && status == DocumentStatus::Published {
        services
            .governance
            .enforce_publish_workflow_gate(&document_id, &locale)
            .await?;
    }

    let doc = services
        .content
        .update_entry(
            &schema,
            &document_id,
            &locale,
            status,
            &body.data,
            actor_id(&actor),
            false,
        )
        .await?;
    services
        .integrations
        .emit_webhook_event("entry.update", Some(&schema.uid), doc.clone())
        .await;
    Ok(Json(ApiResponse::new(doc)))
}

async fn admin_delete_entry_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path((uid, document_id)): Path<(String, String)>,
    RawQuery(raw_qs): RawQuery,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(
        &actor,
        "plugin::content-manager.explorer.delete",
        Some(&uid),
    )?;
    let schema = services.schema.get_schema(&uid).await?;
    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    services
        .content
        .delete_entry(&schema, &document_id, params.locale.as_deref())
        .await?;
    services
        .integrations
        .emit_webhook_event(
            "entry.delete",
            Some(&schema.uid),
            serde_json::json!({ "documentId": document_id }),
        )
        .await;
    Ok(Json(ApiResponse::new(
        serde_json::json!({ "deleted": document_id }),
    )))
}

async fn admin_publish_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path((uid, document_id)): Path<(String, String)>,
    RawQuery(raw_qs): RawQuery,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(
        &actor,
        "plugin::content-manager.explorer.publish",
        Some(&uid),
    )?;
    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = match params.locale.as_deref() {
        Some(l) if !l.is_empty() => l.to_string(),
        _ => services.governance.default_locale().await?,
    };
    services
        .governance
        .enforce_publish_workflow_gate(&document_id, &locale)
        .await?;
    let doc = services
        .content
        .publish_entry(&uid, &document_id, &locale, actor_id(&actor))
        .await?;
    services
        .integrations
        .emit_webhook_event("entry.publish", Some(&uid), doc.clone())
        .await;
    Ok(Json(ApiResponse::new(doc)))
}

async fn admin_unpublish_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path((uid, document_id)): Path<(String, String)>,
    RawQuery(raw_qs): RawQuery,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(
        &actor,
        "plugin::content-manager.explorer.publish",
        Some(&uid),
    )?;
    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = match params.locale.as_deref() {
        Some(l) if !l.is_empty() => l.to_string(),
        _ => services.governance.default_locale().await?,
    };
    let doc = services
        .content
        .unpublish_entry(&uid, &document_id, &locale)
        .await?;
    Ok(Json(ApiResponse::new(doc)))
}

async fn admin_bulk_publish_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
    Json(body): Json<BulkIdsBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(
        &actor,
        "plugin::content-manager.explorer.publish",
        Some(&uid),
    )?;
    let locale = match body.locale.as_deref() {
        Some(l) if !l.is_empty() => l.to_string(),
        _ => services.governance.default_locale().await?,
    };
    let mut count = 0;
    for doc_id in &body.document_ids {
        services
            .governance
            .enforce_publish_workflow_gate(doc_id, &locale)
            .await?;
        services
            .content
            .publish_entry(&uid, doc_id, &locale, actor_id(&actor))
            .await?;
        count += 1;
    }
    Ok(Json(ApiResponse::new(
        serde_json::json!({ "publishedCount": count }),
    )))
}

async fn admin_bulk_delete_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
    Json(body): Json<BulkIdsBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(
        &actor,
        "plugin::content-manager.explorer.delete",
        Some(&uid),
    )?;
    let schema = services.schema.get_schema(&uid).await?;
    let mut count = 0;
    for doc_id in &body.document_ids {
        services
            .content
            .delete_entry(&schema, doc_id, body.locale.as_deref())
            .await?;
        count += 1;
    }
    Ok(Json(ApiResponse::new(
        serde_json::json!({ "deletedCount": count }),
    )))
}

async fn admin_list_versions_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path((uid, document_id)): Path<(String, String)>,
    RawQuery(raw_qs): RawQuery,
) -> Result<Json<ApiResponse<Vec<ContentVersionDto>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-manager.explorer.read", Some(&uid))?;
    let params = ContentQueryParams::parse_query_str(raw_qs.as_deref())?;
    let locale = match params.locale.as_deref() {
        Some(l) if !l.is_empty() => l.to_string(),
        _ => services.governance.default_locale().await?,
    };
    let versions = services
        .content
        .list_versions(&uid, &document_id, &locale)
        .await?;
    Ok(Json(ApiResponse::new(versions)))
}

async fn admin_restore_version_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path((uid, document_id, version_id)): Path<(String, String, String)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(
        &actor,
        "plugin::content-manager.explorer.update",
        Some(&uid),
    )?;
    let doc = services
        .content
        .restore_version(&uid, &document_id, &version_id, actor_id(&actor))
        .await?;
    Ok(Json(ApiResponse::new(doc)))
}
