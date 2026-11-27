use crate::error::HttpApiError;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::header;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use cms_core::{
    builtin_permission_catalog, Actor, AdminRoleDto, AdminUserDto, ApiError, ApiResponse,
    ApiTokenDto, AuthTokenResponse, CreatedTokenDto, PermissionCatalogItem, PermissionRule,
};
use cms_services::{
    CreateAdminUserInput, CreateTokenInput, LoginInput, RegisterAdminInput, ServiceContainer,
    UpdateAdminUserInput, UpsertRoleInput,
};
use serde::{Deserialize, Serialize};

/// Axum extractor resolving any valid Bearer token (Admin JWT, Admin Token, or API Token)
/// or falling back to `Actor::Public` when no Authorization header is supplied.
pub struct AuthActor(pub Actor);

impl FromRequestParts<ServiceContainer> for AuthActor {
    type Rejection = HttpApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ServiceContainer,
    ) -> Result<Self, Self::Rejection> {
        let Some(auth_val) = parts.headers.get(header::AUTHORIZATION) else {
            let public = state.auth.resolve_public_actor().await?;
            return Ok(Self(public));
        };

        let raw = auth_val
            .to_str()
            .map_err(|_| HttpApiError(ApiError::Unauthorized("Malformed Authorization header".into())))?;

        let Some(token) = raw.strip_prefix("Bearer ").or_else(|| raw.strip_prefix("bearer ")) else {
            return Err(HttpApiError(ApiError::Unauthorized(
                "Authorization header must use Bearer scheme".into(),
            )));
        };

        let actor = state.auth.resolve_bearer_actor(token).await?;
        Ok(Self(actor))
    }
}

/// Extractor requiring an authenticated Admin actor (Admin JWT or Admin Token).
pub struct RequireAdmin(pub Actor);

impl FromRequestParts<ServiceContainer> for RequireAdmin {
    type Rejection = HttpApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ServiceContainer,
    ) -> Result<Self, Self::Rejection> {
        let AuthActor(actor) = AuthActor::from_request_parts(parts, state).await?;
        if !actor.is_admin() {
            return Err(HttpApiError(ApiError::Unauthorized(
                "Administrator authentication required".into(),
            )));
        }
        Ok(Self(actor))
    }
}

pub fn enforce_permission(
    actor: &Actor,
    action: &str,
    target_uid: Option<&str>,
) -> Result<(), HttpApiError> {
    if actor.can(action, target_uid) {
        Ok(())
    } else {
        Err(HttpApiError(ApiError::Forbidden(format!(
            "Missing required permission: {action}"
        ))))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefreshBody {
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdatePermissionsBody {
    permissions: Vec<PermissionRule>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeDto {
    user: AdminUserDto,
    permissions: Vec<PermissionRule>,
}

pub fn admin_auth_routes() -> Router<ServiceContainer> {
    Router::new()
        // Public onboarding & auth endpoints (rate limited)
        .route("/admin/register-admin", post(register_admin_handler))
        .route("/admin/login", post(login_handler))
        .route("/admin/refresh-token", post(refresh_token_handler))
        .route("/admin/logout", post(logout_handler))
        // Authenticated profile
        .route("/admin/users/me", get(me_handler).put(update_me_handler))
        // Admin Users CRUD
        .route(
            "/admin/users",
            get(list_users_handler).post(create_user_handler),
        )
        .route(
            "/admin/users/{id}",
            put(update_user_handler).delete(delete_user_handler),
        )
        // Admin Roles & Permissions Matrix
        .route(
            "/admin/roles",
            get(list_roles_handler).post(create_role_handler),
        )
        .route(
            "/admin/roles/{id}",
            put(update_role_handler).delete(delete_role_handler),
        )
        .route(
            "/admin/roles/{id}/permissions",
            get(get_role_permissions_handler).put(update_role_permissions_handler),
        )
        .route("/admin/permissions/catalog", get(permission_catalog_handler))
        .route(
            "/admin/public-permissions",
            get(get_public_permissions_handler).put(update_public_permissions_handler),
        )
        // API Tokens & Admin Tokens
        .route(
            "/admin/api-tokens",
            get(list_api_tokens_handler).post(create_api_token_handler),
        )
        .route("/admin/api-tokens/{id}", delete(delete_api_token_handler))
        .route(
            "/admin/admin-tokens",
            get(list_admin_tokens_handler).post(create_admin_token_handler),
        )
        .route("/admin/admin-tokens/{id}", delete(delete_admin_token_handler))
}

fn client_key(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "local".to_string())
}

fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok())
}

async fn register_admin_handler(
    State(services): State<ServiceContainer>,
    headers: HeaderMap,
    Json(input): Json<RegisterAdminInput>,
) -> Result<Json<ApiResponse<AuthTokenResponse>>, HttpApiError> {
    let ip = client_key(&headers);
    services
        .auth
        .check_rate_limit("register", &ip, 10, 60)
        .await?;
    let res = services
        .auth
        .register_first_admin(input, user_agent(&headers), Some(&ip))
        .await?;
    Ok(Json(ApiResponse::new(res)))
}

async fn login_handler(
    State(services): State<ServiceContainer>,
    headers: HeaderMap,
    Json(input): Json<LoginInput>,
) -> Result<Json<ApiResponse<AuthTokenResponse>>, HttpApiError> {
    let ip = client_key(&headers);
    let email_key = input.email.trim().to_lowercase();
    // Enforce both per-email (immune to X-Forwarded-For spoofing) and per-IP rate limits
    services
        .auth
        .check_rate_limit("login_email", &email_key, 10, 60)
        .await?;
    services
        .auth
        .check_rate_limit("login_ip", &ip, 30, 60)
        .await?;
    let res = services
        .auth
        .login_admin(input, user_agent(&headers), Some(&ip))
        .await?;
    Ok(Json(ApiResponse::new(res)))
}

async fn refresh_token_handler(
    State(services): State<ServiceContainer>,
    headers: HeaderMap,
    Json(body): Json<RefreshBody>,
) -> Result<Json<ApiResponse<AuthTokenResponse>>, HttpApiError> {
    let ip = client_key(&headers);
    let res = services
        .auth
        .refresh_admin_token(&body.refresh_token, user_agent(&headers), Some(&ip))
        .await?;
    Ok(Json(ApiResponse::new(res)))
}

async fn logout_handler(
    State(services): State<ServiceContainer>,
    Json(body): Json<RefreshBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    services.auth.logout(&body.refresh_token).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "ok": true }))))
}

async fn me_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<MeDto>>, HttpApiError> {
    match actor {
        Actor::AdminUser { user_id, .. } => {
            let (user, permissions) = services.auth.get_admin_profile(&user_id).await?;
            Ok(Json(ApiResponse::new(MeDto { user, permissions })))
        }
        _ => Err(HttpApiError(ApiError::BadRequest(
            "Endpoint requires a user session JWT".into(),
        ))),
    }
}

async fn update_me_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(mut input): Json<UpdateAdminUserInput>,
) -> Result<Json<ApiResponse<AdminUserDto>>, HttpApiError> {
    let Actor::AdminUser { user_id, .. } = actor else {
        return Err(HttpApiError(ApiError::BadRequest(
            "Endpoint requires a user session JWT".into(),
        )));
    };
    // Prevent self-escalation of roles or active status through /me
    input.role_ids = None;
    input.is_active = None;
    let user = services.auth.update_admin_user(&user_id, input).await?;
    Ok(Json(ApiResponse::new(user)))
}

async fn list_users_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<AdminUserDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::users.read", None)?;
    let users = services.auth.list_admin_users().await?;
    Ok(Json(ApiResponse::new(users)))
}

async fn create_user_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(input): Json<CreateAdminUserInput>,
) -> Result<Json<ApiResponse<AdminUserDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::users.create", None)?;
    let user = services.auth.create_admin_user(input).await?;
    Ok(Json(ApiResponse::new(user)))
}

async fn update_user_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
    Json(input): Json<UpdateAdminUserInput>,
) -> Result<Json<ApiResponse<AdminUserDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::users.update", None)?;
    let user = services.auth.update_admin_user(&id, input).await?;
    Ok(Json(ApiResponse::new(user)))
}

async fn delete_user_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::users.delete", None)?;
    let acting_id = match &actor {
        Actor::AdminUser { user_id, .. } => Some(user_id.as_str()),
        _ => None,
    };
    services.auth.delete_admin_user(&id, acting_id).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": id }))))
}

async fn list_roles_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<AdminRoleDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::roles.read", None)?;
    let roles = services.auth.list_roles().await?;
    Ok(Json(ApiResponse::new(roles)))
}

async fn create_role_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(input): Json<UpsertRoleInput>,
) -> Result<Json<ApiResponse<AdminRoleDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::roles.create", None)?;
    let role = services.auth.create_role(input).await?;
    Ok(Json(ApiResponse::new(role)))
}

async fn update_role_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
    Json(input): Json<UpsertRoleInput>,
) -> Result<Json<ApiResponse<AdminRoleDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::roles.update", None)?;
    let role = services.auth.update_role(&id, input).await?;
    Ok(Json(ApiResponse::new(role)))
}

async fn delete_role_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::roles.delete", None)?;
    services.auth.delete_role(&id).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": id }))))
}

async fn get_role_permissions_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<PermissionRule>>>, HttpApiError> {
    enforce_permission(&actor, "admin::roles.read", None)?;
    let perms = services.auth.get_role_permissions(&id).await?;
    Ok(Json(ApiResponse::new(perms)))
}

async fn update_role_permissions_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
    Json(body): Json<UpdatePermissionsBody>,
) -> Result<Json<ApiResponse<Vec<PermissionRule>>>, HttpApiError> {
    enforce_permission(&actor, "admin::roles.update", None)?;
    let perms = services
        .auth
        .update_role_permissions(&id, &body.permissions)
        .await?;
    Ok(Json(ApiResponse::new(perms)))
}

async fn permission_catalog_handler(
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<PermissionCatalogItem>>>, HttpApiError> {
    enforce_permission(&actor, "admin::roles.read", None)?;
    Ok(Json(ApiResponse::new(builtin_permission_catalog())))
}

async fn get_public_permissions_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<PermissionRule>>>, HttpApiError> {
    enforce_permission(&actor, "admin::roles.read", None)?;
    let perms = services.auth.get_public_permissions().await?;
    Ok(Json(ApiResponse::new(perms)))
}

async fn update_public_permissions_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(body): Json<UpdatePermissionsBody>,
) -> Result<Json<ApiResponse<Vec<PermissionRule>>>, HttpApiError> {
    enforce_permission(&actor, "admin::roles.update", None)?;
    let perms = services
        .auth
        .update_public_permissions(&body.permissions)
        .await?;
    Ok(Json(ApiResponse::new(perms)))
}

async fn list_api_tokens_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<ApiTokenDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::api-tokens.read", None)?;
    let tokens = services.auth.list_tokens(false).await?;
    Ok(Json(ApiResponse::new(tokens)))
}

async fn create_api_token_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(input): Json<CreateTokenInput>,
) -> Result<Json<ApiResponse<CreatedTokenDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::api-tokens.create", None)?;
    let created = services.auth.create_token(false, None, input).await?;
    Ok(Json(ApiResponse::new(created)))
}

async fn delete_api_token_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::api-tokens.delete", None)?;
    services.auth.delete_token(false, &id).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": id }))))
}

async fn list_admin_tokens_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<ApiTokenDto>>>, HttpApiError> {
    enforce_permission(&actor, "admin::api-tokens.read", None)?;
    let tokens = services.auth.list_tokens(true).await?;
    Ok(Json(ApiResponse::new(tokens)))
}

async fn create_admin_token_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(input): Json<CreateTokenInput>,
) -> Result<Json<ApiResponse<CreatedTokenDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::api-tokens.create", None)?;
    let creator_id = match &actor {
        Actor::AdminUser { user_id, .. } => Some(user_id.as_str()),
        _ => None,
    };
    let created = services.auth.create_token(true, creator_id, input).await?;
    Ok(Json(ApiResponse::new(created)))
}

async fn delete_admin_token_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "admin::api-tokens.delete", None)?;
    services.auth.delete_token(true, &id).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": id }))))
}
