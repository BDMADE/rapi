use crate::auth::{enforce_permission, AuthActor, RequireAdmin};
use crate::error::HttpApiError;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use cms_core::{
    Actor, AiCompletionOutputDto, AiSettingsDto, ApiError, ApiResponse, AuthTokenResponse,
    CustomFieldDto, InstalledPluginDto, MarketplacePluginDto, PublicSsoProviderDto, SsoProviderDto,
    TokenType,
};
use cms_services::{
    AiCompletionInput, CreateSsoProviderInput, InstallCustomWasmInput, InvokePluginInput,
    RegisterCustomFieldInput, ServiceContainer, SsoAuthorizeUrlDto, SsoCallbackInput,
    UpdateAiSettingsInput, UpdatePluginInput, UpdateSsoProviderInput,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SsoAuthorizeQuery {
    redirect_uri: String,
}

pub fn extensibility_routes() -> Router<ServiceContainer> {
    Router::new()
        // Custom Fields
        .route(
            "/admin/custom-fields",
            get(list_custom_fields_handler).post(register_custom_field_handler),
        )
        .route(
            "/admin/custom-fields/{uid}",
            delete(delete_custom_field_handler),
        )
        // Plugins & Sandboxed WASM Marketplace
        .route("/admin/plugins", get(list_installed_plugins_handler))
        .route(
            "/admin/plugins/marketplace",
            get(list_marketplace_plugins_handler),
        )
        .route(
            "/admin/plugins/marketplace/{name}/install",
            post(install_marketplace_plugin_handler),
        )
        .route(
            "/admin/plugins/wasm",
            post(install_custom_wasm_plugin_handler),
        )
        .route(
            "/admin/plugins/{name}",
            put(update_plugin_handler).delete(uninstall_plugin_handler),
        )
        .route(
            "/admin/plugins/{name}/invoke",
            post(invoke_plugin_handler),
        )
        // JSON-RPC 2.0 MCP Server
        .route("/api/mcp", post(mcp_rpc_handler))
        // OIDC SSO
        .route(
            "/admin/sso/providers",
            get(list_public_sso_providers_handler),
        )
        .route(
            "/admin/sso/config",
            get(list_sso_config_handler).post(create_sso_provider_handler),
        )
        .route(
            "/admin/sso/config/{id}",
            put(update_sso_provider_handler).delete(delete_sso_provider_handler),
        )
        .route(
            "/admin/auth/sso/{id}/authorize",
            get(begin_sso_authorize_handler),
        )
        .route(
            "/admin/auth/sso/{id}/callback",
            post(complete_sso_callback_handler),
        )
        // Opt-in AI Assistant
        .route(
            "/admin/ai/config",
            get(get_ai_settings_handler).put(update_ai_settings_handler),
        )
        .route("/admin/ai/complete", post(complete_ai_task_handler))
}

// ============================================================================
// 1. Custom Fields Handlers
// ============================================================================

async fn list_custom_fields_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
) -> Result<Json<ApiResponse<Vec<CustomFieldDto>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::content-type-builder.read", None)?;
    let list = state.extensibility.list_custom_fields().await?;
    Ok(Json(ApiResponse::new(list)))
}

async fn register_custom_field_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Json(input): Json<RegisterCustomFieldInput>,
) -> Result<Json<ApiResponse<CustomFieldDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::plugins.manage", None)?;
    let saved = state.extensibility.register_custom_field(input).await?;
    Ok(Json(ApiResponse::new(saved)))
}

async fn delete_custom_field_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Path(uid): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::plugins.manage", None)?;
    state.extensibility.delete_custom_field(&uid).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": uid }))))
}

// ============================================================================
// 2. Plugins & WASM Marketplace Handlers
// ============================================================================

async fn list_installed_plugins_handler(
    RequireAdmin(_actor): RequireAdmin,
    State(state): State<ServiceContainer>,
) -> Result<Json<ApiResponse<Vec<InstalledPluginDto>>>, HttpApiError> {
    let list = state.extensibility.list_installed_plugins().await?;
    Ok(Json(ApiResponse::new(list)))
}

async fn list_marketplace_plugins_handler(
    RequireAdmin(_actor): RequireAdmin,
    State(state): State<ServiceContainer>,
) -> Result<Json<ApiResponse<Vec<MarketplacePluginDto>>>, HttpApiError> {
    let list = state.extensibility.list_marketplace_plugins().await?;
    Ok(Json(ApiResponse::new(list)))
}

async fn install_marketplace_plugin_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<InstalledPluginDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::plugins.manage", None)?;
    let installed = state.extensibility.install_marketplace_plugin(&name).await?;
    Ok(Json(ApiResponse::new(installed)))
}

async fn install_custom_wasm_plugin_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Json(input): Json<InstallCustomWasmInput>,
) -> Result<Json<ApiResponse<InstalledPluginDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::plugins.manage", None)?;
    let installed = state
        .extensibility
        .install_custom_wasm_plugin(input)
        .await?;
    Ok(Json(ApiResponse::new(installed)))
}

async fn update_plugin_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Path(name): Path<String>,
    Json(input): Json<UpdatePluginInput>,
) -> Result<Json<ApiResponse<InstalledPluginDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::plugins.manage", None)?;
    let updated = state.extensibility.update_plugin(&name, input).await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn uninstall_plugin_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::plugins.manage", None)?;
    state.extensibility.uninstall_plugin(&name).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": name }))))
}

async fn invoke_plugin_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Path(name): Path<String>,
    Json(input): Json<InvokePluginInput>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::plugins.manage", None)?;
    let out = state.extensibility.invoke_plugin(&name, input).await?;
    Ok(Json(ApiResponse::new(out)))
}

// ============================================================================
// 3. JSON-RPC 2.0 MCP Endpoint (`/api/mcp`)
// ============================================================================

async fn mcp_rpc_handler(
    AuthActor(actor): AuthActor,
    State(state): State<ServiceContainer>,
    Json(rpc_body): Json<serde_json::Value>,
) -> Result<axum::response::Response, HttpApiError> {
    let allowed = match &actor {
        Actor::AdminUser { .. } | Actor::AdminToken { .. } => {
            actor.can("admin::mcp.execute", None)
        }
        Actor::ApiToken { token_type, .. } => {
            *token_type == TokenType::ReadOnly || actor.can("admin::mcp.execute", None)
        }
        Actor::EndUser { .. } | Actor::Public { .. } => false,
    };

    if !allowed {
        return Err(HttpApiError(ApiError::Unauthorized(
            "Valid Admin JWT, Admin Token, or API Token with MCP access required".into(),
        )));
    }

    match state.extensibility.handle_mcp_rpc(&actor, rpc_body).await {
        Some(resp_json) => Ok((StatusCode::OK, Json(resp_json)).into_response()),
        None => Ok(StatusCode::ACCEPTED.into_response()),
    }
}

// ============================================================================
// 4. OIDC SSO Handlers
// ============================================================================

async fn list_public_sso_providers_handler(
    State(state): State<ServiceContainer>,
) -> Result<Json<ApiResponse<Vec<PublicSsoProviderDto>>>, HttpApiError> {
    let list = state.extensibility.list_public_sso_providers().await?;
    Ok(Json(ApiResponse::new(list)))
}

async fn list_sso_config_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
) -> Result<Json<ApiResponse<Vec<SsoProviderDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::sso.update", None)?;
    let list = state.extensibility.list_sso_providers().await?;
    Ok(Json(ApiResponse::new(list)))
}

async fn create_sso_provider_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Json(input): Json<CreateSsoProviderInput>,
) -> Result<Json<ApiResponse<SsoProviderDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::sso.update", None)?;
    let created = state.extensibility.create_sso_provider(input).await?;
    Ok(Json(ApiResponse::new(created)))
}

async fn update_sso_provider_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Path(id): Path<String>,
    Json(input): Json<UpdateSsoProviderInput>,
) -> Result<Json<ApiResponse<SsoProviderDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::sso.update", None)?;
    let updated = state.extensibility.update_sso_provider(&id, input).await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn delete_sso_provider_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::sso.update", None)?;
    state.extensibility.delete_sso_provider(&id).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": id }))))
}

async fn begin_sso_authorize_handler(
    State(state): State<ServiceContainer>,
    Path(id): Path<String>,
    Query(query): Query<SsoAuthorizeQuery>,
) -> Result<Json<ApiResponse<SsoAuthorizeUrlDto>>, HttpApiError> {
    let out = state
        .extensibility
        .begin_sso_authorize(&id, &query.redirect_uri)
        .await?;
    Ok(Json(ApiResponse::new(out)))
}

async fn complete_sso_callback_handler(
    State(state): State<ServiceContainer>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<SsoCallbackInput>,
) -> Result<Json<ApiResponse<AuthTokenResponse>>, HttpApiError> {
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let out = state
        .extensibility
        .complete_sso_callback(&id, input, ua, None)
        .await?;
    Ok(Json(ApiResponse::new(out)))
}

// ============================================================================
// 5. Opt-in AI Assistant Handlers
// ============================================================================

async fn get_ai_settings_handler(
    RequireAdmin(_actor): RequireAdmin,
    State(state): State<ServiceContainer>,
) -> Result<Json<ApiResponse<AiSettingsDto>>, HttpApiError> {
    let dto = state.extensibility.get_ai_settings().await?;
    Ok(Json(ApiResponse::new(dto)))
}

async fn update_ai_settings_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Json(input): Json<UpdateAiSettingsInput>,
) -> Result<Json<ApiResponse<AiSettingsDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::plugins.manage", None)?;
    let updated = state.extensibility.update_ai_settings(input).await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn complete_ai_task_handler(
    RequireAdmin(actor): RequireAdmin,
    State(state): State<ServiceContainer>,
    Json(input): Json<AiCompletionInput>,
) -> Result<Json<ApiResponse<AiCompletionOutputDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::ai.use", None)?;
    let out = state.extensibility.complete_ai_task(input).await?;
    Ok(Json(ApiResponse::new(out)))
}
