use crate::error::HttpApiError;
use crate::openapi::ApiDoc;
use axum::extract::State;
use axum::http::{HeaderValue, Method, Uri};
use axum::routing::get;
use axum::{Json, Router};
use cms_core::{ApiError, ApiResponse};
use cms_services::{
    AdminInitDto, HealthDto, ReadinessDto, ServiceContainer, SystemOverviewDto,
};
use serde::Serialize;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiIndexDto {
    pub name: &'static str,
    pub compatibility: &'static str,
    pub version: &'static str,
    pub docs_url: &'static str,
}

pub fn build_router(services: ServiceContainer) -> Router {
    let cors = build_cors_layer(&services.config.cors_origins);

    Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route("/openapi.json", get(openapi_handler))
        .route("/api", get(api_index_handler))
        .route("/admin/init", get(admin_init_handler))
        .route("/admin/system/overview", get(system_overview_handler))
        .fallback(not_found_handler)
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
        .allow_headers(Any);

    if origins.iter().any(|o| o == "*") {
        return base.allow_origin(Any);
    }

    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();

    if parsed.is_empty() {
        base.allow_origin(Any)
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
) -> Result<Json<ApiResponse<SystemOverviewDto>>, HttpApiError> {
    let dto = services.system.overview().await?;
    Ok(Json(ApiResponse::new(dto)))
}

async fn api_index_handler() -> Json<ApiResponse<ApiIndexDto>> {
    Json(ApiResponse::new(ApiIndexDto {
        name: "RAPI Content API",
        compatibility: "Strapi v5",
        version: env!("CARGO_PKG_VERSION"),
        docs_url: "/openapi.json",
    }))
}

async fn openapi_handler() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

async fn not_found_handler(uri: Uri) -> HttpApiError {
    HttpApiError(ApiError::NotFound(format!(
        "Route '{}' was not found",
        uri.path()
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
        let services = ServiceContainer::bootstrap(cfg).await.unwrap();
        build_router(services)
    }

    #[tokio::test]
    async fn health_and_ready_endpoints_succeed() {
        let app = test_router().await;

        let res = app
            .clone()
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

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
}
