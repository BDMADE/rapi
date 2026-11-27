use cms_core::{
    DocumentId, DocumentStatus, ErrorBody, ErrorEnvelope, PaginationMeta, PlatformStats,
    ResponseMeta, RuntimeBackendKind,
};
use cms_services::{AdminInitDto, HealthDto, ReadinessDto, SystemOverviewDto};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "RAPI Headless CMS API",
        version = "0.1.0",
        description = "Strapi v5-compatible Headless CMS powered by Rust (Axum + SQLite WAL)"
    ),
    paths(
        crate::routes::health_handler,
        crate::routes::ready_handler,
        crate::routes::admin_init_handler,
        crate::routes::system_overview_handler,
    ),
    components(schemas(
        DocumentId,
        DocumentStatus,
        RuntimeBackendKind,
        PlatformStats,
        PaginationMeta,
        ResponseMeta,
        ErrorEnvelope,
        ErrorBody,
        HealthDto,
        ReadinessDto,
        AdminInitDto,
        SystemOverviewDto,
    ))
)]
pub struct ApiDoc;
