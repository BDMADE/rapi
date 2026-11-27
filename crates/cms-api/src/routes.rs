use crate::auth::{admin_auth_routes, enforce_permission, RequireAdmin};
use crate::content::content_api_routes;
use crate::error::HttpApiError;
use crate::openapi::ApiDoc;
use crate::schema::admin_schema_routes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode, Uri};
use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};
use cms_core::{ApiError, ApiResponse};
use cms_services::{
    AdminInitDto, HealthDto, ReadinessDto, ServiceContainer, SystemOverviewDto,
};
use serde::Serialize;
use std::time::Duration;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiIndexDto {
    pub name: &'static str,
    pub compatibility: &'static str,
    pub version: &'static str,
    pub docs_url: Option<&'static str>,
}

pub fn build_router(services: ServiceContainer) -> Router {
    let cors = build_cors_layer(&services.config.cors_origins);
    let x_request_id = HeaderName::from_static("x-request-id");
    let timeout_secs = services.config.request_timeout_secs.max(1);
    let max_body_bytes = services.config.max_body_bytes;
    let enable_docs = services.config.enable_docs;

    let mut router = Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route("/api", get(api_index_handler))
        .route("/admin/init", get(admin_init_handler))
        .route("/admin/system/overview", get(system_overview_handler))
        .merge(admin_auth_routes())
        .merge(admin_schema_routes())
        .merge(content_api_routes());

    if enable_docs {
        router = router
            .route("/docs", get(api_docs_handler))
            .route("/openapi.json", get(openapi_handler));
    }

    router
        .fallback(not_found_handler)
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        ))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(timeout_secs),
        ))
        .layer(PropagateRequestIdLayer::new(x_request_id.clone()))
        .layer(SetRequestIdLayer::new(x_request_id, MakeRequestUuid))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(services)
}

fn build_cors_layer(origins: &[String]) -> CorsLayer {
    let base = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            HeaderName::from_static("x-request-id"),
        ]);

    if origins.iter().any(|o| o == "*") {
        return base.allow_origin(Any);
    }

    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();

    // Fail-closed: if no valid origins were configured, do not emit wildcard CORS headers.
    if parsed.is_empty() {
        base
    } else {
        base.allow_origin(parsed)
    }
}

#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Liveness check", body = HealthDto))
)]
pub async fn health_handler(State(services): State<ServiceContainer>) -> Json<HealthDto> {
    Json(services.system.health())
}

#[utoipa::path(
    get,
    path = "/ready",
    responses((status = 200, description = "Readiness check", body = ReadinessDto))
)]
pub async fn ready_handler(
    State(services): State<ServiceContainer>,
) -> Result<Json<ReadinessDto>, HttpApiError> {
    let dto = services.system.readiness().await?;
    Ok(Json(dto))
}

#[utoipa::path(
    get,
    path = "/admin/init",
    responses((status = 200, description = "Admin onboarding initialization status"))
)]
pub async fn admin_init_handler(
    State(services): State<ServiceContainer>,
) -> Result<Json<ApiResponse<AdminInitDto>>, HttpApiError> {
    let dto = services.system.admin_init().await?;
    Ok(Json(ApiResponse::new(dto)))
}

#[utoipa::path(
    get,
    path = "/admin/system/overview",
    responses((status = 200, description = "Platform statistics and runtime overview"))
)]
pub async fn system_overview_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<SystemOverviewDto>>, HttpApiError> {
    enforce_permission(&actor, "admin::system.overview.read", None)?;
    let dto = services.system.overview().await?;
    Ok(Json(ApiResponse::new(dto)))
}

async fn api_index_handler(
    State(services): State<ServiceContainer>,
) -> Json<ApiResponse<ApiIndexDto>> {
    Json(ApiResponse::new(ApiIndexDto {
        name: "RAPI Content API",
        compatibility: "Strapi v5",
        version: env!("CARGO_PKG_VERSION"),
        docs_url: if services.config.enable_docs {
            Some("/openapi.json")
        } else {
            None
        },
    }))
}

async fn openapi_handler() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

/// Self-contained OpenAPI viewer with zero external CDN scripts and no token localStorage persistence.
async fn api_docs_handler() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>RAPI — OpenAPI Documentation</title>
  <style>
    body{font-family:system-ui,-apple-system,sans-serif;margin:0;background:#fafafa;color:#18181b;padding:24px}
    .wrap{max-width:920px;margin:0 auto}
    h1{font-size:18px;margin:0 0 6px}
    p{font-size:13px;color:#52525b;margin:0 0 18px}
    .card{background:#fff;border:1px solid #e4e4e7;border-radius:6px;overflow:hidden}
    .row{display:flex;align-items:center;gap:12px;padding:10px 14px;border-bottom:1px solid #e4e4e7;font-size:13px}
    .row:last-child{border-bottom:0}
    .m{font-family:ui-monospace,monospace;font-size:11px;font-weight:600;padding:2px 6px;border-radius:4px;background:#eef2ff;color:#4338ca}
    .path{font-family:ui-monospace,monospace;font-weight:500}
    .desc{color:#52525b;margin-left:auto}
  </style>
</head>
<body>
  <div class="wrap">
    <h1>RAPI OpenAPI 3.1 Reference</h1>
    <p>Self-hosted documentation loaded directly from <a href="/openapi.json">/openapi.json</a> (zero external CDN dependencies).</p>
    <div id="list" class="card"><div class="row">Loading specification...</div></div>
  </div>
  <script>
    fetch('/openapi.json').then(r=>r.json()).then(spec=>{
      const el=document.getElementById('list');
      el.innerHTML='';
      for(const [path,methods] of Object.entries(spec.paths||{})){
        for(const [method,op] of Object.entries(methods)){
          const row=document.createElement('div');
          row.className='row';
          const m=document.createElement('span');
          m.className='m';
          m.textContent=method.toUpperCase();
          const p=document.createElement('span');
          p.className='path';
          p.textContent=path;
          const d=document.createElement('span');
          d.className='desc';
          d.textContent=(op&&op.summary)||'';
          row.append(m,p,d);
          el.append(row);
        }
      }
    });
  </script>
</body>
</html>"#,
    )
}

async fn not_found_handler(uri: Uri) -> HttpApiError {
    let path = uri.path();
    let safe_path = if path.len() > 128 {
        format!("{}...", &path[..128])
    } else {
        path.to_string()
    };
    HttpApiError(ApiError::NotFound(format!(
        "Route '{safe_path}' was not found"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use cms_core::AppConfig;
    use tower::ServiceExt;

    async fn test_router() -> Router {
        let mut cfg = AppConfig::from_env();
        cfg.database_url = "sqlite::memory:".to_string();
        cfg.redis_url = None;
        cfg.enable_docs = true;
        let services = ServiceContainer::bootstrap(cfg).await.unwrap();
        build_router(services)
    }

    #[tokio::test]
    async fn health_ready_and_security_headers_succeed() {
        let app = test_router().await;

        let res = app
            .clone()
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().contains_key("x-request-id"));
        assert_eq!(res.headers()["x-content-type-options"], "nosniff");
        assert_eq!(res.headers()["x-frame-options"], "DENY");
        assert_eq!(res.headers()["referrer-policy"], "no-referrer");

        let docs_res = app
            .clone()
            .oneshot(Request::builder().uri("/docs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(docs_res.status(), StatusCode::OK);

        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");

        let ready_res = app
            .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(ready_res.status(), StatusCode::OK);
        let ready_body = to_bytes(ready_res.into_body(), usize::MAX).await.unwrap();
        let ready_json: serde_json::Value = serde_json::from_slice(&ready_body).unwrap();
        assert_eq!(ready_json["status"], "ready");
        assert_eq!(ready_json["cacheBackend"], "local");
    }

    #[tokio::test]
    async fn production_mode_disables_docs_and_protects_overview() {
        let mut cfg = AppConfig::from_env();
        cfg.env = "production".to_string();
        cfg.jwt_secret = "x".repeat(48);
        cfg.database_url = "sqlite::memory:".to_string();
        cfg.redis_url = None;
        cfg.enable_docs = false;
        cfg.cors_origins = vec![];
        let services = ServiceContainer::bootstrap(cfg).await.unwrap();
        let app = build_router(services);

        let docs_res = app
            .clone()
            .oneshot(Request::builder().uri("/docs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(docs_res.status(), StatusCode::NOT_FOUND);

        let overview_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/system/overview")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(overview_res.status(), StatusCode::UNAUTHORIZED);

        // Fail-closed CORS: empty cors_origins must NOT reflect Access-Control-Allow-Origin: *
        let cors_res = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header("Origin", "https://evil.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(cors_res.headers().get("access-control-allow-origin").is_none());
    }

    #[tokio::test]
    async fn admin_init_and_404_match_strapi_v5_envelope() {
        let app = test_router().await;

        let init_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/init")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(init_res.status(), StatusCode::OK);
        let init_bytes = to_bytes(init_res.into_body(), usize::MAX).await.unwrap();
        let init_json: serde_json::Value = serde_json::from_slice(&init_bytes).unwrap();
        assert_eq!(init_json["data"]["hasAdmin"], false);
        assert_eq!(init_json["data"]["defaultLocale"], "en");

        let nf_res = app
            .oneshot(
                Request::builder()
                    .uri("/api/non-existent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(nf_res.status(), StatusCode::NOT_FOUND);
        let nf_bytes = to_bytes(nf_res.into_body(), usize::MAX).await.unwrap();
        let nf_json: serde_json::Value = serde_json::from_slice(&nf_bytes).unwrap();
        assert!(nf_json["data"].is_null());
        assert_eq!(nf_json["error"]["status"], 404);
        assert_eq!(nf_json["error"]["name"], "NotFoundError");
    }

    #[tokio::test]
    async fn full_phase1_auth_rbac_and_tokens_lifecycle() {
        let app = test_router().await;

        // 1. Unauthenticated access to /admin/users fails with 401
        let unauth = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

        // 2. Register first Super Admin
        let reg_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/register-admin")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "email": "ada@rapi.dev",
                            "password": "Password123!",
                            "firstname": "Ada",
                            "lastname": "Lovelace"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reg_res.status(), StatusCode::OK);
        let reg_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(reg_res.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let super_jwt = reg_json["data"]["token"].as_str().unwrap().to_string();
        let refresh_tok = reg_json["data"]["refreshToken"].as_str().unwrap().to_string();
        assert_eq!(reg_json["data"]["user"]["isSuperAdmin"], true);

        // 3. Duplicate first-admin registration must fail with 409 Conflict
        let dup_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/register-admin")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "email": "second@rapi.dev",
                            "password": "Password123!",
                            "firstname": "Second"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(dup_res.status(), StatusCode::CONFLICT);

        // 4. Refresh token rotates and invalidates the old refresh token
        let ref_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/refresh-token")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "refreshToken": refresh_tok }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ref_res.status(), StatusCode::OK);

        let replay_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/refresh-token")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "refreshToken": refresh_tok }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(replay_res.status(), StatusCode::UNAUTHORIZED);

        // 5. Create custom restricted admin user (role_author has zero admin::users.read permission)
        let create_u = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/users")
                    .header("authorization", format!("Bearer {super_jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "email": "author@rapi.dev",
                            "password": "Password123!",
                            "firstname": "Grace",
                            "roleIds": ["role_author"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_u.status(), StatusCode::OK);

        // Login as author
        let author_login = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "email": "author@rapi.dev",
                            "password": "Password123!"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(author_login.status(), StatusCode::OK);
        let author_json: serde_json::Value = serde_json::from_slice(
            &to_bytes(author_login.into_body(), usize::MAX).await.unwrap(),
        )
        .unwrap();
        let author_jwt = author_json["data"]["token"].as_str().unwrap().to_string();

        // Author cannot read admin users (403 Forbidden)
        let forbidden = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/users")
                    .header("authorization", format!("Bearer {author_jwt}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

        // Grant `admin::users.read` to `role_author` via RBAC matrix
        let grant = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/admin/roles/role_author/permissions")
                    .header("authorization", format!("Bearer {super_jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "permissions": [{ "action": "admin::users.read" }]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(grant.status(), StatusCode::OK);

        // Author can now read admin users (200 OK)
        let allowed = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/users")
                    .header("authorization", format!("Bearer {author_jwt}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);

        // 6. Issue a read-only Admin Token and verify read works while write is 403
        let tok_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/admin-tokens")
                    .header("authorization", format!("Bearer {super_jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "CI ReadOnly",
                            "description": "Read telemetry",
                            "type": "read-only"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tok_res.status(), StatusCode::OK);
        let tok_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(tok_res.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let raw_admin_token = tok_json["data"]["accessKey"].as_str().unwrap().to_string();
        assert!(raw_admin_token.starts_with("rapi_adm_"));

        let ro_overview = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/system/overview")
                    .header("authorization", format!("Bearer {raw_admin_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ro_overview.status(), StatusCode::OK);

        let ro_write = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/roles")
                    .header("authorization", format!("Bearer {raw_admin_token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({ "name": "Nope" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ro_write.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn phase1_security_regressions() {
        let app = test_router().await;

        // Seed Super Admin
        let reg = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/register-admin")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "email": "root@rapi.dev",
                            "password": "Password123!",
                            "firstname": "Root"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let reg_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(reg.into_body(), usize::MAX).await.unwrap()).unwrap();
        let jwt = reg_json["data"]["token"].as_str().unwrap().to_string();
        let root_id = reg_json["data"]["user"]["id"].as_str().unwrap().to_string();
        let old_refresh = reg_json["data"]["refreshToken"]
            .as_str()
            .unwrap()
            .to_string();

        // BUG-P1-01: Revoked token must be immediately rejected even after being cached
        let tok_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/admin-tokens")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "name": "Temp Admin Token",
                            "type": "full-access"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let tok_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(tok_res.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let tok_id = tok_json["data"]["id"].as_str().unwrap().to_string();
        let raw_tok = tok_json["data"]["accessKey"].as_str().unwrap().to_string();

        // Warm cache with valid request
        let ok_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/system/overview")
                    .header("authorization", format!("Bearer {raw_tok}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok_res.status(), StatusCode::OK);

        // Revoke token and immediately verify it returns 401
        let del_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/admin/admin-tokens/{tok_id}"))
                    .header("authorization", format!("Bearer {jwt}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(del_res.status(), StatusCode::OK);

        let revoked_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/system/overview")
                    .header("authorization", format!("Bearer {raw_tok}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(revoked_res.status(), StatusCode::UNAUTHORIZED);

        // BUG-P1-06: Deactivating the last active Super Admin must fail with 400
        let lock_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/admin/users/{root_id}"))
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "isActive": false }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(lock_res.status(), StatusCode::BAD_REQUEST);

        // BUG-P1-02: Changing password must immediately invalidate old refresh tokens
        let pw_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/admin/users/me")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "password": "NewPassword456!" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pw_res.status(), StatusCode::OK);

        let stale_ref = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/refresh-token")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "refreshToken": old_refresh }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale_ref.status(), StatusCode::UNAUTHORIZED);

        // BUG-P1-09: Unknown roleId returns 400 BadRequest (not 500)
        let bad_role = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/users")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "email": "badrole@rapi.dev",
                            "password": "Password123!",
                            "firstname": "Bad",
                            "roleIds": ["role_does_not_exist"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad_role.status(), StatusCode::BAD_REQUEST);

        // BUG-P1-10: Updating permissions on non-existent role returns 404
        let missing_role = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/admin/roles/role_missing/permissions")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "permissions": [] }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_role.status(), StatusCode::NOT_FOUND);

        // BUG-P1-04: Per-email rate limit triggers 429 even if X-Forwarded-For is spoofed every attempt
        let mut hit_429 = false;
        for attempt in 0..12 {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/admin/login")
                        .header("x-forwarded-for", format!("10.0.0.{attempt}"))
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "email": "root@rapi.dev",
                                "password": "wrong-password"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            if res.status() == StatusCode::TOO_MANY_REQUESTS {
                hit_429 = true;
                break;
            }
        }
        assert!(hit_429, "Expected per-email rate limit to trigger 429");
    }

    #[tokio::test]
    async fn phase2_content_type_builder_lifecycle_and_ddl_inspection() {
        let app = test_router().await;

        let reg = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/register-admin")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "email": "builder@rapi.dev",
                            "password": "Password123!",
                            "firstname": "Builder"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let reg_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(reg.into_body(), usize::MAX).await.unwrap()).unwrap();
        let jwt = reg_json["data"]["token"].as_str().unwrap().to_string();

        // 1. Create reusable component `shared.seo`
        let comp_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/content-types")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "uid": "shared.seo",
                            "kind": "component",
                            "category": "shared",
                            "singularName": "seo",
                            "pluralName": "seos",
                            "displayName": "SEO Meta",
                            "fields": [
                                { "name": "metaTitle", "fieldType": "string", "required": true },
                                { "name": "metaDescription", "fieldType": "text" }
                            ]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(comp_res.status(), StatusCode::OK);

        // 2. Create `api::category.category` collection type
        let cat_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/content-types")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "uid": "api::category.category",
                            "kind": "collectionType",
                            "singularName": "category",
                            "pluralName": "categories",
                            "displayName": "Category",
                            "fields": [
                                { "name": "name", "fieldType": "string", "required": true, "unique": true }
                            ]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cat_res.status(), StatusCode::OK);

        // 3. Create `api::article.article` with scalar, blocks, enum, media, relation, component, and dynamiczone!
        let art_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/content-types")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "uid": "api::article.article",
                            "kind": "collectionType",
                            "singularName": "article",
                            "pluralName": "articles",
                            "displayName": "Article",
                            "draftAndPublish": true,
                            "i18nEnabled": true,
                            "fields": [
                                { "name": "title", "fieldType": "string", "required": true, "localized": true },
                                { "name": "slug", "fieldType": "uid", "unique": true, "validation": { "targetField": "title" } },
                                { "name": "body", "fieldType": "blocks", "localized": true },
                                { "name": "kind_tag", "fieldType": "enumeration", "validation": { "enumValues": ["news", "tutorial"] } },
                                { "name": "cover", "fieldType": "media" },
                                {
                                    "name": "category",
                                    "fieldType": "relation",
                                    "relation": {
                                        "relationType": "manyToOne",
                                        "targetUid": "api::category.category"
                                    }
                                },
                                {
                                    "name": "seo",
                                    "fieldType": "component",
                                    "component": { "componentUid": "shared.seo", "repeatable": false }
                                },
                                {
                                    "name": "sections",
                                    "fieldType": "dynamiczone",
                                    "component": { "components": ["shared.seo"] }
                                }
                            ]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(art_res.status(), StatusCode::OK);

        // 4. Inspect physical SQLite table for `api::article.article`
        let insp_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/content-types/api::article.article/inspection")
                    .header("authorization", format!("Bearer {jwt}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(insp_res.status(), StatusCode::OK);
        let insp_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(insp_res.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(
            insp_json["data"]["tableName"],
            "ct_api_article_article"
        );
        let cols = insp_json["data"]["columns"].as_array().unwrap();
        assert!(cols.iter().any(|c| c["name"] == "title"));
        assert!(cols.iter().any(|c| c["name"] == "seo"));
        assert!(cols.iter().any(|c| c["name"] == "sections"));
        // Relation & Media are resolved via status-isolated join tables, not physical columns
        assert!(!cols.iter().any(|c| c["name"] == "cover"));
        assert!(!cols.iter().any(|c| c["name"] == "category"));

        // 5. Deleting `shared.seo` while referenced by `api::article.article` must return 409 Conflict
        let del_comp = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/admin/content-types/shared.seo")
                    .header("authorization", format!("Bearer {jwt}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(del_comp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn phase2_schema_regressions() {
        let app = test_router().await;

        let reg = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/register-admin")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "email": "schema-reg@rapi.dev",
                            "password": "Password123!",
                            "firstname": "Reg"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let reg_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(reg.into_body(), usize::MAX).await.unwrap()).unwrap();
        let jwt = reg_json["data"]["token"].as_str().unwrap().to_string();

        // BUG-P2-04: Reserved API slug (`upload`) must be rejected with 400
        let reserved = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/content-types")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "uid": "api::upload.upload",
                            "kind": "collectionType",
                            "singularName": "upload",
                            "pluralName": "uploads",
                            "displayName": "Upload",
                            "fields": []
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reserved.status(), StatusCode::BAD_REQUEST);

        // BUG-P2-05: minLength > maxLength must be rejected with 400
        let bad_bounds = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/content-types")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "uid": "api::note.note",
                            "kind": "collectionType",
                            "singularName": "note",
                            "pluralName": "notes",
                            "displayName": "Note",
                            "fields": [{
                                "name": "title",
                                "fieldType": "string",
                                "validation": { "minLength": 20, "maxLength": 5 }
                            }]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad_bounds.status(), StatusCode::BAD_REQUEST);

        // BUG-P2-02: Transitive circular component nesting (shared.a -> shared.b -> shared.a) blocked
        let comp_b = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/content-types")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "uid": "shared.b",
                            "kind": "component",
                            "singularName": "b",
                            "pluralName": "bs",
                            "displayName": "Comp B",
                            "fields": [{ "name": "label", "fieldType": "string" }]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(comp_b.status(), StatusCode::OK);
        let comp_b_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(comp_b.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        // BUG-P2-08: Components always force draftAndPublish=false
        assert_eq!(comp_b_json["data"]["draftAndPublish"], false);

        let comp_a = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/content-types")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "uid": "shared.a",
                            "kind": "component",
                            "singularName": "a",
                            "pluralName": "as",
                            "displayName": "Comp A",
                            "fields": [{
                                "name": "nestedB",
                                "fieldType": "component",
                                "component": { "componentUid": "shared.b", "repeatable": false }
                            }]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(comp_a.status(), StatusCode::OK);

        // Now updating shared.b to embed shared.a creates A -> B -> A cycle -> must return 400
        let cycle = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/admin/content-types/shared.b")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "uid": "shared.b",
                            "kind": "component",
                            "singularName": "b",
                            "pluralName": "bs",
                            "displayName": "Comp B",
                            "fields": [{
                                "name": "nestedA",
                                "fieldType": "component",
                                "component": { "componentUid": "shared.a", "repeatable": false }
                            }]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cycle.status(), StatusCode::BAD_REQUEST);

        // BUG-P2-06: Metadata-only update on a collection type must NOT log a spurious DDL migration
        let create_post = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/content-types")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "uid": "api::post.post",
                            "kind": "collectionType",
                            "singularName": "post",
                            "pluralName": "posts",
                            "displayName": "Post",
                            "fields": [{ "name": "title", "fieldType": "string" }]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_post.status(), StatusCode::OK);

        let meta_update = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/admin/content-types/api::post.post")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "uid": "api::post.post",
                            "kind": "collectionType",
                            "singularName": "post",
                            "pluralName": "posts",
                            "displayName": "Blog Post",
                            "description": "Updated metadata only",
                            "fields": [{ "name": "title", "fieldType": "string" }]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(meta_update.status(), StatusCode::OK);

        let insp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/content-types/api::post.post/inspection")
                    .header("authorization", format!("Bearer {jwt}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let insp_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(insp.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(insp_json["data"]["migrations"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn phase3_author_publish_relations_filters_fts_and_versions_flow() {
        let app = test_router().await;

        // 1. Register Super Admin
        let reg = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/register-admin")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "email": "editor@rapi.dev",
                            "password": "Password123!",
                            "firstname": "Editor"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let reg_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(reg.into_body(), usize::MAX).await.unwrap()).unwrap();
        let jwt = reg_json["data"]["token"].as_str().unwrap().to_string();

        // 2. Define Component `shared.seo`, Collection `api::tag.tag` (draftAndPublish=false), and Collection `api::article.article` (draftAndPublish=true)
        for schema in [
            serde_json::json!({
                "uid": "shared.seo",
                "kind": "component",
                "category": "shared",
                "singularName": "seo",
                "pluralName": "seos",
                "displayName": "SEO",
                "fields": [{ "name": "metaTitle", "fieldType": "string", "required": true }]
            }),
            serde_json::json!({
                "uid": "api::tag.tag",
                "kind": "collectionType",
                "singularName": "tag",
                "pluralName": "tags",
                "displayName": "Tag",
                "draftAndPublish": false,
                "fields": [{ "name": "name", "fieldType": "string", "required": true }]
            }),
            serde_json::json!({
                "uid": "api::article.article",
                "kind": "collectionType",
                "singularName": "article",
                "pluralName": "articles",
                "displayName": "Article",
                "draftAndPublish": true,
                "fields": [
                    { "name": "title", "fieldType": "string", "required": true },
                    { "name": "slug", "fieldType": "uid", "unique": true, "validation": { "targetField": "title" } },
                    { "name": "views", "fieldType": "integer" },
                    { "name": "secretNote", "fieldType": "string", "private": true },
                    { "name": "body", "fieldType": "blocks" },
                    {
                        "name": "tags",
                        "fieldType": "relation",
                        "relation": { "relationType": "manyToMany", "targetUid": "api::tag.tag" }
                    },
                    {
                        "name": "seo",
                        "fieldType": "component",
                        "component": { "componentUid": "shared.seo", "repeatable": false }
                    },
                    {
                        "name": "blocksZone",
                        "fieldType": "dynamiczone",
                        "component": { "components": ["shared.seo"] }
                    }
                ]
            }),
        ] {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/admin/content-types")
                        .header("authorization", format!("Bearer {jwt}"))
                        .header("content-type", "application/json")
                        .body(Body::from(schema.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }

        // Grant Public `find` & `findOne` on `api::article.article`
        let pub_grant = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/admin/public-permissions")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "permissions": [
                                { "action": "api::article.article.find", "targetUid": "api::article.article" },
                                { "action": "api::article.article.findOne", "targetUid": "api::article.article" }
                            ]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pub_grant.status(), StatusCode::OK);

        // 3. Create two Tags via REST Content API
        let mut tag_ids = Vec::new();
        for tag_name in ["Rust", "Axum"] {
            let t_res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/tags")
                        .header("authorization", format!("Bearer {jwt}"))
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({ "data": { "name": tag_name } }).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(t_res.status(), StatusCode::OK);
            let t_json: serde_json::Value =
                serde_json::from_slice(&to_bytes(t_res.into_body(), usize::MAX).await.unwrap())
                    .unwrap();
            tag_ids.push(t_json["data"]["documentId"].as_str().unwrap().to_string());
        }

        // 4. Author a Draft Article via Admin Content Manager (auto-generates UID slug, links tags, validates component & dynamic zone)
        let draft_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/content-manager/api::article.article")
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "data": {
                                "title": "Building Strapi v5 in Rust",
                                "views": 120,
                                "secretNote": "Internal editorial note",
                                "body": [{ "type": "paragraph", "children": [{ "type": "text", "text": "Zero redeploy dynamic SQLite engine." }] }],
                                "tags": { "connect": [{ "documentId": tag_ids[0] }, { "documentId": tag_ids[1] }] },
                                "seo": { "metaTitle": "Rust CMS SEO" },
                                "blocksZone": [{ "__component": "shared.seo", "metaTitle": "Zone Block" }]
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(draft_res.status(), StatusCode::OK);
        let draft_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(draft_res.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let art_doc_id = draft_json["data"]["documentId"].as_str().unwrap().to_string();
        assert_eq!(draft_json["data"]["slug"], "building-strapi-v5-in-rust");
        assert!(draft_json["data"]["publishedAt"].is_null());

        // 5. Public REST GET /api/articles returns 0 rows before publish, and blocks ?status=draft with 403
        let pre_pub = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/articles")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let pre_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(pre_pub.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(pre_json["data"].as_array().unwrap().len(), 0);

        let pub_draft_attempt = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/articles?status=draft")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pub_draft_attempt.status(), StatusCode::FORBIDDEN);

        // 6. Publish the Article!
        let publish_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/admin/content-manager/api::article.article/{art_doc_id}/publish"
                    ))
                    .header("authorization", format!("Bearer {jwt}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(publish_res.status(), StatusCode::OK);

        // 7. Edit the Draft title ("V2 Draft") AND verify live Published REST response STILL serves V1 ("Building Strapi v5 in Rust")!
        let edit_draft = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!(
                        "/admin/content-manager/api::article.article/{art_doc_id}"
                    ))
                    .header("authorization", format!("Bearer {jwt}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "data": { "title": "V2 Draft Only" } }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(edit_draft.status(), StatusCode::OK);

        // Query REST API with filters, populate=*, and FTS5 `_q=redeploy`!
        let live_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/articles?filters[views][$gte]=100&populate=*&_q=redeploy")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(live_res.status(), StatusCode::OK);
        let live_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(live_res.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let rows = live_json["data"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["documentId"], art_doc_id);
        assert_eq!(rows[0]["title"], "Building Strapi v5 in Rust");
        assert_eq!(rows[0]["tags"].as_array().unwrap().len(), 2);
        // `private: true` field `secretNote` MUST be stripped from Content API!
        assert!(rows[0].get("secretNote").is_none());

        // 8. Verify Version History & Restore
        let ver_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/admin/content-manager/api::article.article/{art_doc_id}/versions"
                    ))
                    .header("authorization", format!("Bearer {jwt}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ver_res.status(), StatusCode::OK);
        let ver_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(ver_res.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let versions = ver_json["data"].as_array().unwrap();
        assert!(versions.len() >= 3); // initial draft + published + v2 draft
    }
}
