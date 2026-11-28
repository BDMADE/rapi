use crate::auth::{enforce_permission, AuthActor, RequireAdmin};
use crate::error::HttpApiError;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use cms_core::{Actor, ApiError, ApiResponse, MediaFileDto, MediaFolderDto, MediaListFilter};
use cms_services::{
    CropImageInput, ServiceContainer, UpdateMediaMetadataInput, UploadFileMetadata,
};
use serde::Deserialize;

const UPLOAD_MAX_BODY_BYTES: usize = 50 * 1024 * 1024; // 50 MB

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MediaQueryParams {
    folder_id: Option<String>, // "root" | numeric id | omitted (all)
    mime_prefix: Option<String>,
    #[serde(alias = "_q")]
    q: Option<String>,
    page: Option<u64>,
    page_size: Option<u64>,
    force: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateFolderBody {
    name: String,
    parent_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RenameFolderBody {
    name: String,
}

pub fn media_routes() -> Router<ServiceContainer> {
    let upload_routes = Router::new()
        .route("/api/upload", post(public_upload_handler))
        .route("/admin/media/upload", post(admin_upload_handler))
        .layer(DefaultBodyLimit::max(UPLOAD_MAX_BODY_BYTES));

    Router::new()
        .merge(upload_routes)
        // Hardened static media serving
        .route("/uploads/{*key}", get(serve_uploaded_file_handler))
        // Strapi v5 Content API upload read/delete endpoints
        .route("/api/upload/files", get(public_list_files_handler))
        .route(
            "/api/upload/files/{id}",
            get(public_get_file_handler).delete(public_delete_file_handler),
        )
        // Admin Media Library endpoints
        .route("/admin/media/files", get(admin_list_files_handler))
        .route(
            "/admin/media/files/{id}",
            get(admin_get_file_handler)
                .put(admin_update_file_handler)
                .delete(admin_delete_file_handler),
        )
        .route("/admin/media/files/{id}/crop", post(admin_crop_file_handler))
        .route(
            "/admin/media/folders",
            get(admin_list_folders_handler).post(admin_create_folder_handler),
        )
        .route(
            "/admin/media/folders/{id}",
            put(admin_rename_folder_handler).delete(admin_delete_folder_handler),
        )
}

fn actor_id(actor: &Actor) -> Option<String> {
    match actor {
        Actor::AdminUser { user_id, .. } => Some(user_id.clone()),
        Actor::AdminToken { token_id, .. } | Actor::ApiToken { token_id, .. } => {
            Some(token_id.clone())
        }
        Actor::Public { .. } => None,
    }
}

fn is_safe_inline_mime(mime: &str) -> bool {
    matches!(
        mime,
        "image/png" | "image/jpeg" | "image/webp" | "image/gif" | "application/pdf"
    ) || mime.starts_with("video/")
        || mime.starts_with("audio/")
}

async fn serve_uploaded_file_handler(
    State(services): State<ServiceContainer>,
    Path(key): Path<String>,
) -> Result<Response, HttpApiError> {
    let clean_key = key.trim_start_matches('/');
    let (bytes, mime) = services.media.read_stored_bytes(clean_key).await?;

    let mut res = (StatusCode::OK, bytes).into_response();
    let headers = res.headers_mut();
    if let Ok(ct) = HeaderValue::from_str(&mime) {
        headers.insert(header::CONTENT_TYPE, ct);
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; img-src 'self'; media-src 'self'; sandbox"),
    );
    if !is_safe_inline_mime(&mime) {
        headers.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment"),
        );
    }
    Ok(res)
}

async fn parse_and_store_multipart(
    services: &ServiceContainer,
    mut multipart: Multipart,
    actor: Option<String>,
) -> Result<Vec<MediaFileDto>, HttpApiError> {
    let mut shared_meta = UploadFileMetadata::default();
    let mut pending_files: Vec<(String, Option<String>, Vec<u8>)> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| HttpApiError(ApiError::BadRequest(format!("Invalid multipart body: {e}"))))?
    {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "fileInfo" {
            let txt = field
                .text()
                .await
                .map_err(|e| HttpApiError(ApiError::BadRequest(format!("Read fileInfo: {e}"))))?;
            if let Ok(parsed) = serde_json::from_str::<UploadFileMetadata>(&txt) {
                shared_meta = parsed;
            }
        } else if field_name == "folderId" {
            if let Ok(txt) = field.text().await {
                shared_meta.folder_id = txt.trim().parse::<i64>().ok();
            }
        } else if field.file_name().is_some() || field_name == "files" || field_name == "file" {
            let fname = field.file_name().unwrap_or("upload.bin").to_string();
            let ctype = field.content_type().map(String::from);
            let data = field
                .bytes()
                .await
                .map_err(|e| HttpApiError(ApiError::BadRequest(format!("Read file bytes: {e}"))))?;
            pending_files.push((fname, ctype, data.to_vec()));
        }
    }

    if pending_files.is_empty() {
        return Err(HttpApiError(ApiError::BadRequest(
            "No files provided in multipart upload (expected field 'files')".into(),
        )));
    }

    let mut uploaded = Vec::with_capacity(pending_files.len());
    for (fname, ctype, bytes) in pending_files {
        let dto = services
            .media
            .upload_file(
                &fname,
                ctype.as_deref(),
                &bytes,
                shared_meta.clone(),
                actor.clone(),
            )
            .await?;
        uploaded.push(dto);
    }
    Ok(uploaded)
}

async fn public_upload_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    multipart: Multipart,
) -> Result<Json<Vec<MediaFileDto>>, HttpApiError> {
    if !actor.can("plugin::upload.assets.create", None) && !actor.can("api::upload.create", None) {
        return Err(HttpApiError(ApiError::Forbidden(
            "Missing permission to upload media".into(),
        )));
    }
    let uploaded = parse_and_store_multipart(&services, multipart, actor_id(&actor)).await?;
    // Strapi v5 `POST /api/upload` returns a top-level JSON array of uploaded file descriptors
    Ok(Json(uploaded))
}

async fn public_list_files_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Query(q): Query<MediaQueryParams>,
) -> Result<Json<Vec<MediaFileDto>>, HttpApiError> {
    if !actor.can("plugin::upload.read", None) && !actor.can("api::upload.find", None) {
        return Err(HttpApiError(ApiError::Forbidden(
            "Missing permission to list media files".into(),
        )));
    }
    let filter = build_media_filter(&q);
    let (files, _) = services.media.list_files(&filter).await?;
    Ok(Json(files))
}

async fn public_get_file_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Path(id): Path<i64>,
) -> Result<Json<MediaFileDto>, HttpApiError> {
    if !actor.can("plugin::upload.read", None) && !actor.can("api::upload.find", None) {
        return Err(HttpApiError(ApiError::Forbidden(
            "Missing permission to read media file".into(),
        )));
    }
    let file = services.media.get_file(id).await?;
    Ok(Json(file))
}

async fn public_delete_file_handler(
    State(services): State<ServiceContainer>,
    AuthActor(actor): AuthActor,
    Path(id): Path<i64>,
    Query(q): Query<MediaQueryParams>,
) -> Result<Json<MediaFileDto>, HttpApiError> {
    if !actor.can("plugin::upload.assets.create", None)
        && !actor.can("api::upload.destroy", None)
    {
        return Err(HttpApiError(ApiError::Forbidden(
            "Missing permission to delete media file".into(),
        )));
    }
    let deleted = services
        .media
        .delete_file(id, q.force.unwrap_or(false))
        .await?;
    Ok(Json(deleted))
}

// --- Admin Media Library Handlers ---

fn build_media_filter(q: &MediaQueryParams) -> MediaListFilter {
    let folder_id = match q.folder_id.as_deref() {
        Some("root") => Some(None),
        Some(num_str) => num_str.parse::<i64>().ok().map(Some),
        None => None,
    };
    MediaListFilter {
        folder_id,
        mime_prefix: q.mime_prefix.clone(),
        search_query: q.q.clone(),
        page: q.page.unwrap_or(1),
        page_size: q.page_size.unwrap_or(24),
    }
}

async fn admin_upload_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    multipart: Multipart,
) -> Result<Json<ApiResponse<Vec<MediaFileDto>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::upload.assets.create", None)?;
    let uploaded = parse_and_store_multipart(&services, multipart, actor_id(&actor)).await?;
    Ok(Json(ApiResponse::new(uploaded)))
}

async fn admin_list_files_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Query(q): Query<MediaQueryParams>,
) -> Result<Json<ApiResponse<Vec<MediaFileDto>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::upload.read", None)?;
    let filter = build_media_filter(&q);
    let (files, pagination) = services.media.list_files(&filter).await?;
    Ok(Json(ApiResponse::with_pagination(files, pagination)))
}

async fn admin_get_file_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<MediaFileDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::upload.read", None)?;
    let file = services.media.get_file(id).await?;
    Ok(Json(ApiResponse::new(file)))
}

async fn admin_update_file_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<i64>,
    Json(input): Json<UpdateMediaMetadataInput>,
) -> Result<Json<ApiResponse<MediaFileDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::upload.assets.create", None)?;
    let updated = services.media.update_file_metadata(id, input).await?;
    Ok(Json(ApiResponse::new(updated)))
}

async fn admin_crop_file_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<i64>,
    Json(input): Json<CropImageInput>,
) -> Result<Json<ApiResponse<MediaFileDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::upload.assets.create", None)?;
    let cropped = services.media.crop_and_resize(id, input).await?;
    Ok(Json(ApiResponse::new(cropped)))
}

async fn admin_delete_file_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<i64>,
    Query(q): Query<MediaQueryParams>,
) -> Result<Json<ApiResponse<MediaFileDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::upload.assets.create", None)?;
    let deleted = services.media.delete_file(id, q.force.unwrap_or(false)).await?;
    Ok(Json(ApiResponse::new(deleted)))
}

async fn admin_list_folders_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
) -> Result<Json<ApiResponse<Vec<MediaFolderDto>>>, HttpApiError> {
    enforce_permission(&actor, "plugin::upload.read", None)?;
    let folders = services.media.list_folders(None).await?;
    Ok(Json(ApiResponse::new(folders)))
}

async fn admin_create_folder_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Json(body): Json<CreateFolderBody>,
) -> Result<Json<ApiResponse<MediaFolderDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::upload.assets.create", None)?;
    let folder = services
        .media
        .create_folder(&body.name, body.parent_id)
        .await?;
    Ok(Json(ApiResponse::new(folder)))
}

async fn admin_rename_folder_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<i64>,
    Json(body): Json<RenameFolderBody>,
) -> Result<Json<ApiResponse<MediaFolderDto>>, HttpApiError> {
    enforce_permission(&actor, "plugin::upload.assets.create", None)?;
    let folder = services.media.rename_folder(id, &body.name).await?;
    Ok(Json(ApiResponse::new(folder)))
}

async fn admin_delete_folder_handler(
    State(services): State<ServiceContainer>,
    RequireAdmin(actor): RequireAdmin,
    Path(id): Path<i64>,
) -> Result<Json<ApiResponse<serde_json::Value>>, HttpApiError> {
    enforce_permission(&actor, "plugin::upload.assets.create", None)?;
    services.media.delete_folder(id).await?;
    Ok(Json(ApiResponse::new(serde_json::json!({ "deleted": id }))))
}
