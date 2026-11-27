use cms_core::{
    sanitize_filename, validate_storage_key, ApiError, ApiResult, JobMessage, MediaFileDto,
    MediaFolderDto, MediaFormatVariant, MediaListFilter, PaginationMeta, QueueBackend,
    StorageProvider,
};
use cms_db::{InsertMediaInput, MediaRepository};
use image::{DynamicImage, ImageFormat};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

const BLOCKED_EXTENSIONS: &[&str] = &[
    ".html", ".htm", ".xhtml", ".shtml", ".js", ".mjs", ".cjs", ".ts", ".sh", ".bash", ".exe",
    ".dll", ".bat", ".cmd", ".ps1", ".vbs", ".php", ".phtml", ".jsp", ".asp", ".aspx", ".svgz",
];

const MAX_IMAGE_DIMENSION: u32 = 10_000;
const MAX_IMAGE_DECODE_ALLOC: u64 = 128 * 1024 * 1024; // 128 MB decompression bomb cap

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadFileMetadata {
    pub name: Option<String>,
    pub alternative_text: Option<String>,
    pub caption: Option<String>,
    pub folder_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMediaMetadataInput {
    pub name: Option<String>,
    pub alternative_text: Option<String>,
    pub caption: Option<String>,
    pub folder_id: Option<i64>,
    #[serde(default)]
    pub move_to_root: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropImageInput {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub resize_width: Option<u32>,
    pub resize_height: Option<u32>,
}

#[derive(Clone)]
pub struct MediaService {
    repo: MediaRepository,
    storage: Arc<dyn StorageProvider>,
    queue: Arc<dyn QueueBackend>,
}

impl MediaService {
    pub fn new(
        repo: MediaRepository,
        storage: Arc<dyn StorageProvider>,
        queue: Arc<dyn QueueBackend>,
    ) -> Self {
        Self {
            repo,
            storage,
            queue,
        }
    }

    pub async fn upload_file(
        &self,
        original_filename: &str,
        declared_mime: Option<&str>,
        bytes: &[u8],
        meta: UploadFileMetadata,
        actor_id: Option<String>,
    ) -> ApiResult<MediaFileDto> {
        if bytes.is_empty() {
            return Err(ApiError::BadRequest("Cannot upload an empty file".into()));
        }

        let (stem, ext) = sanitize_filename(original_filename);
        if BLOCKED_EXTENSIONS.iter().any(|b| b.eq_ignore_ascii_case(&ext)) {
            return Err(ApiError::Validation {
                message: format!("File extension '{ext}' is blocked for security"),
                details: serde_json::json!({ "ext": ext }),
            });
        }

        let guessed_mime = mime_guess::from_ext(ext.trim_start_matches('.'))
            .first_or_octet_stream()
            .essence_str()
            .to_string();
        // Never trust untrusted client-declared MIME types over magic bytes / sanitized extension
        let mime = match image::guess_format(bytes) {
            Ok(ImageFormat::Png) => "image/png".to_string(),
            Ok(ImageFormat::Jpeg) => "image/jpeg".to_string(),
            Ok(ImageFormat::WebP) => "image/webp".to_string(),
            Ok(ImageFormat::Gif) => "image/gif".to_string(),
            _ => {
                let _ = declared_mime;
                guessed_mime
            }
        };

        let full_hash = sha256_hex(bytes);
        let short_hash = &full_hash[..16];
        let size_kb = round_kb(bytes.len());
        let display_name = meta
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| format!("{stem}{ext}"));

        // SHA-256 deduplication: if an identical file hash is already stored, reuse its physical object & dimensions
        if let Some(existing) = self.repo.find_by_hash(&full_hash).await? {
            return self
                .repo
                .insert_file(InsertMediaInput {
                    name: display_name,
                    alternative_text: meta.alternative_text,
                    caption: meta.caption,
                    width: existing.width,
                    height: existing.height,
                    formats_json: existing.formats.to_string(),
                    hash: full_hash,
                    ext,
                    mime: existing.mime,
                    size_kb,
                    url: existing.url,
                    provider: self.storage.provider_name().to_string(),
                    folder_id: meta.folder_id,
                    created_by: actor_id,
                })
                .await;
        }

        let storage_key = format!("{short_hash}_{stem}{ext}");
        let url = self.storage.put(&storage_key, bytes).await?;

        let mut width: Option<i64> = None;
        let mut height: Option<i64> = None;
        let mut formats_map: BTreeMap<String, MediaFormatVariant> = BTreeMap::new();
        let mut needs_responsive_worker = false;

        if is_processable_raster(&mime, &ext) {
            if let Ok(img) = decode_safe_image(bytes) {
                let w = img.width();
                let h = img.height();
                width = Some(w as i64);
                height = Some(h as i64);

                if w > 245 || h > 156 {
                    if let Ok(thumb_variant) =
                        self.generate_variant(&img, "thumbnail", 245, 156, short_hash, &stem, &ext, &mime)
                            .await
                    {
                        formats_map.insert("thumbnail".into(), thumb_variant);
                    }
                }
                if w > 500 {
                    needs_responsive_worker = true;
                }
            }
        }

        let formats_json =
            serde_json::to_string(&formats_map).unwrap_or_else(|_| "{}".to_string());

        let inserted = self
            .repo
            .insert_file(InsertMediaInput {
                name: display_name,
                alternative_text: meta.alternative_text,
                caption: meta.caption,
                width,
                height,
                formats_json,
                hash: full_hash,
                ext,
                mime,
                size_kb,
                url,
                provider: self.storage.provider_name().to_string(),
                folder_id: meta.folder_id,
                created_by: actor_id,
            })
            .await?;

        if needs_responsive_worker {
            let _ = self
                .queue
                .enqueue(JobMessage::new(
                    "rapi:jobs:media",
                    serde_json::json!({ "fileId": inserted.id }),
                ))
                .await;
        }

        Ok(inserted)
    }

    /// Background worker entrypoint generating `small` (500px), `medium` (750px), and `large` (1000px) formats.
    pub async fn process_responsive_formats(&self, file_id: i64) -> ApiResult<()> {
        let Some(file) = self.repo.get_file(file_id).await? else {
            return Ok(());
        };
        let Some(key) = file.url.strip_prefix("/uploads/") else {
            return Ok(());
        };
        let bytes = self.storage.get(key).await?;
        let Ok(img) = decode_safe_image(&bytes) else {
            return Ok(());
        };

        let mut formats_map: BTreeMap<String, MediaFormatVariant> =
            serde_json::from_value(file.formats).unwrap_or_default();
        let short_hash = &file.hash[..file.hash.len().min(16)];
        let (stem, _) = sanitize_filename(&file.name);

        for (label, max_dim) in [("small", 500_u32), ("medium", 750_u32), ("large", 1000_u32)] {
            if img.width() > max_dim {
                if let Ok(variant) = self
                    .generate_variant(
                        &img,
                        label,
                        max_dim,
                        max_dim,
                        short_hash,
                        &stem,
                        &file.ext,
                        &file.mime,
                    )
                    .await
                {
                    formats_map.insert(label.to_string(), variant);
                }
            }
        }

        let formats_json =
            serde_json::to_string(&formats_map).unwrap_or_else(|_| "{}".to_string());
        self.repo.update_formats_json(file_id, &formats_json).await
    }

    pub async fn crop_and_resize(
        &self,
        id: i64,
        input: CropImageInput,
    ) -> ApiResult<MediaFileDto> {
        if input.width == 0 || input.height == 0 {
            return Err(ApiError::Validation {
                message: "Crop width and height must be greater than 0".into(),
                details: serde_json::json!({}),
            });
        }

        let existing = self
            .repo
            .get_file(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Media file {id} not found")))?;

        if !is_processable_raster(&existing.mime, &existing.ext) {
            return Err(ApiError::BadRequest(
                "Only PNG, JPEG, WebP, and GIF images can be cropped".into(),
            ));
        }

        let old_key = existing
            .url
            .strip_prefix("/uploads/")
            .ok_or_else(|| ApiError::Internal("Unexpected media URL format".into()))?
            .to_string();

        let src_bytes = self.storage.get(&old_key).await?;
        let img = decode_safe_image(&src_bytes)?;

        if input.x >= img.width() || input.y >= img.height() {
            return Err(ApiError::Validation {
                message: "Crop origin (x, y) is outside image dimensions".into(),
                details: serde_json::json!({ "imgWidth": img.width(), "imgHeight": img.height() }),
            });
        }

        let clamped_w = input.width.min(img.width() - input.x);
        let clamped_h = input.height.min(img.height() - input.y);
        let mut edited = img.crop_imm(input.x, input.y, clamped_w, clamped_h);

        if let (Some(rw), Some(rh)) = (input.resize_width, input.resize_height) {
            if rw == 0 || rh == 0 || rw > 8192 || rh > 8192 {
                return Err(ApiError::Validation {
                    message: "Resize dimensions must be between 1 and 8192 pixels".into(),
                    details: serde_json::json!({ "resizeWidth": rw, "resizeHeight": rh }),
                });
            }
            edited = edited.resize_exact(rw, rh, image::imageops::FilterType::Lanczos3);
        }

        let fmt = image_format_for_ext(&existing.ext);
        let mut encoded = Vec::new();
        edited
            .write_to(&mut Cursor::new(&mut encoded), fmt)
            .map_err(|e| ApiError::Internal(format!("encode cropped image failed: {e}")))?;

        let new_hash = sha256_hex(&encoded);
        let short_hash = &new_hash[..16];
        let (stem, _) = sanitize_filename(&existing.name);
        let new_key = format!("{short_hash}_{stem}{}", existing.ext);
        let new_url = self.storage.put(&new_key, &encoded).await?;

        let mut formats_map: BTreeMap<String, MediaFormatVariant> = BTreeMap::new();
        if edited.width() > 245 || edited.height() > 156 {
            if let Ok(thumb) = self
                .generate_variant(
                    &edited,
                    "thumbnail",
                    245,
                    156,
                    short_hash,
                    &stem,
                    &existing.ext,
                    &existing.mime,
                )
                .await
            {
                formats_map.insert("thumbnail".into(), thumb);
            }
        }
        let formats_json =
            serde_json::to_string(&formats_map).unwrap_or_else(|_| "{}".to_string());

        let updated = self
            .repo
            .update_file_binary_details(
                id,
                edited.width() as i64,
                edited.height() as i64,
                round_kb(encoded.len()),
                &new_hash,
                &new_url,
                &formats_json,
            )
            .await?;

        // Clean up previous physical file and its responsive variants if no other rows reference it
        if new_url != existing.url && self.repo.count_by_url(&existing.url).await? == 0 {
            let _ = self.storage.delete(&old_key).await;
            self.cleanup_formats_on_disk(&existing.formats).await;
        }

        if edited.width() > 500 {
            let _ = self
                .queue
                .enqueue(JobMessage::new(
                    "rapi:jobs:media",
                    serde_json::json!({ "fileId": updated.id }),
                ))
                .await;
        }

        Ok(updated)
    }

    pub async fn read_stored_bytes(&self, key: &str) -> ApiResult<(Vec<u8>, String)> {
        validate_storage_key(key)?;
        let bytes = self.storage.get(key).await?;
        let mime = mime_guess::from_path(key)
            .first_or_octet_stream()
            .essence_str()
            .to_string();
        Ok((bytes, mime))
    }

    pub async fn list_files(
        &self,
        filter: &MediaListFilter,
    ) -> ApiResult<(Vec<MediaFileDto>, PaginationMeta)> {
        self.repo.list_files(filter).await
    }

    pub async fn get_file(&self, id: i64) -> ApiResult<MediaFileDto> {
        self.repo
            .get_file(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Media file {id} not found")))
    }

    pub async fn update_file_metadata(
        &self,
        id: i64,
        input: UpdateMediaMetadataInput,
    ) -> ApiResult<MediaFileDto> {
        let folder_change = if input.move_to_root {
            Some(None)
        } else {
            input.folder_id.map(Some)
        };
        self.repo
            .update_file_metadata(
                id,
                input.name.as_deref(),
                input.alternative_text.as_deref().map(Some),
                input.caption.as_deref().map(Some),
                folder_change,
            )
            .await
    }

    pub async fn delete_file(&self, id: i64, force: bool) -> ApiResult<MediaFileDto> {
        let deleted = self.repo.delete_file(id, force).await?;

        // Reference-counted cleanup: only delete physical bytes when zero rows share this URL
        if self.repo.count_by_url(&deleted.url).await? == 0 {
            if let Some(key) = deleted.url.strip_prefix("/uploads/") {
                let _ = self.storage.delete(key).await;
            }
            self.cleanup_formats_on_disk(&deleted.formats).await;
        }

        Ok(deleted)
    }

    async fn cleanup_formats_on_disk(&self, formats: &serde_json::Value) {
        if let Some(obj) = formats.as_object() {
            for v in obj.values() {
                if let Some(u) = v.get("url").and_then(|x| x.as_str()) {
                    if let Some(k) = u.strip_prefix("/uploads/") {
                        let _ = self.storage.delete(k).await;
                    }
                }
            }
        }
    }

    // --- Folders ---

    pub async fn list_folders(&self, parent_id: Option<i64>) -> ApiResult<Vec<MediaFolderDto>> {
        self.repo.list_folders(parent_id).await
    }

    pub async fn create_folder(
        &self,
        name: &str,
        parent_id: Option<i64>,
    ) -> ApiResult<MediaFolderDto> {
        self.repo.create_folder(name, parent_id).await
    }

    pub async fn rename_folder(&self, id: i64, name: &str) -> ApiResult<MediaFolderDto> {
        self.repo.rename_folder(id, name).await
    }

    pub async fn delete_folder(&self, id: i64) -> ApiResult<()> {
        self.repo.delete_folder(id).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn generate_variant(
        &self,
        img: &DynamicImage,
        label: &str,
        max_w: u32,
        max_h: u32,
        short_hash: &str,
        stem: &str,
        ext: &str,
        mime: &str,
    ) -> ApiResult<MediaFormatVariant> {
        let resized = img.thumbnail(max_w, max_h);
        let fmt = image_format_for_ext(ext);
        let mut buf = Vec::new();
        resized
            .write_to(&mut Cursor::new(&mut buf), fmt)
            .map_err(|e| ApiError::Internal(format!("encode {label} variant failed: {e}")))?;

        let var_hash = format!("{label}_{short_hash}_{stem}");
        let var_key = format!("{var_hash}{ext}");
        let url = self.storage.put(&var_key, &buf).await?;

        Ok(MediaFormatVariant {
            name: format!("{label}_{stem}{ext}"),
            hash: var_hash,
            ext: ext.to_string(),
            mime: mime.to_string(),
            width: resized.width(),
            height: resized.height(),
            size: round_kb(buf.len()),
            url,
        })
    }
}

fn decode_safe_image(bytes: &[u8]) -> ApiResult<DynamicImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ApiError::BadRequest(format!("Failed to inspect image format: {e}")))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_IMAGE_DECODE_ALLOC);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| ApiError::BadRequest(format!("Failed to decode image safely: {e}")))
}

fn is_processable_raster(mime: &str, ext: &str) -> bool {
    matches!(
        mime,
        "image/png" | "image/jpeg" | "image/jpg" | "image/webp" | "image/gif"
    ) || matches!(
        ext.to_ascii_lowercase().as_str(),
        ".png" | ".jpg" | ".jpeg" | ".webp" | ".gif"
    )
}

fn image_format_for_ext(ext: &str) -> ImageFormat {
    match ext.to_ascii_lowercase().as_str() {
        ".jpg" | ".jpeg" => ImageFormat::Jpeg,
        ".webp" => ImageFormat::WebP,
        ".gif" => ImageFormat::Gif,
        _ => ImageFormat::Png,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn round_kb(byte_len: usize) -> f64 {
    ((byte_len as f64 / 1024.0) * 100.0).round() / 100.0
}
