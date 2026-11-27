use crate::auth::{enforce_permission, RequireAdmin};
use crate::error::HttpApiError;
use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use cms_core::{ApiResponse, ContentKind, ContentTypeSchema, SchemaInspectionDto};
use cms_services::ServiceContainer;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaCatalogResponse {
    pub content_types: Vec<ContentTypeSchema>,
    pub components: Vec<ContentTypeSchema>,
}

pub fn admin_schema_routes() -> Router<ServiceContainer> {
    Router::new()
        .route(
            "/admin/content-types",
            get(list_schemas_handler).post(create_schema_handler),
        )
        .route(
            "/admin/content-types/{uid}",
            get(get_schema_handler)
                .put(update_schema_handler)
                .delete(delete_schema_handler),
        )
        .route(
            "/admin/content-types/{uid}/inspection",
            get(inspect_schema_handler),
        )
}

async fn list_schemas_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<SchemaCatalogResponse>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-type-builder.read", None)?;
    let all = services.schema.list_schemas().await?;
    let mut content_types = Vec::new();
    let mut components = Vec::new();
    for s in all {
        if s.kind == ContentKind::Component {
            components.push(s);
        } else {
            content_types.push(s);
        }
    }
    Ok(Json(ApiResponse::new(SchemaCatalogResponse {
        content_types,
        components,
    })))
}

async fn get_schema_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
) -> Result<Json<ApiResponse<ContentTypeSchema>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-type-builder.read", None)?;
    let schema = services.schema.get_schema(&uid).await?;
    Ok(Json(ApiResponse::new(schema)))
}

async fn create_schema_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(payload): Json<ContentTypeSchema>,
) -> Result<Json<ApiResponse<ContentTypeSchema>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-type-builder.update", None)?;
    let created = services.schema.create_schema(payload).await?;
    Ok(Json(ApiResponse::new(created)))
}

async fn update_schema_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
    Json(payload): Json<ContentTypeSchema>,
) -> Result<Json<ApiResponse<ContentTypeSchema>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-type-builder.update", None)?;
    let updated = services.schema.update_schema(&uid, payload).await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn delete_schema_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-type-builder.update", None)?;
    services.schema.delete_schema(&uid).await?;
    Ok(Json(ApiResponse::new(
        serde_json::json!({ "deleted": uid }),
    )))
}

async fn inspect_schema_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(uid): Path<String>,
) -> Result<Json<ApiResponse<SchemaInspectionDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-type-builder.read", None)?;
    let dto = services.schema.inspect_schema(&uid).await?;
    Ok(Json(ApiResponse::new(dto)))
}
