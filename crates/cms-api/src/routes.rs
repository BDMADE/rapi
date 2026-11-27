use crate::auth::{admin_auth_routes, enforce_permission, RequireAdmin};
use crate::error::HttpApiError;
use crate::openapi::ApiDoc;
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
        .merge(admin_auth_routes());

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
}
