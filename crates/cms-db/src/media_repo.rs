use crate::pool::DbPools;
use cms_core::{
    ApiError, ApiResult, DocumentId, MediaFileDto, MediaFolderDto, MediaListFilter, PaginationMeta,
};
use sqlx::Row;

#[derive(Debug, Clone)]
pub struct InsertMediaInput {
    pub name: String,
    pub alternative_text: Option<String>,
    pub caption: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub formats_json: String,
    pub hash: String,
    pub ext: String,
    pub mime: String,
    pub size_kb: f64,
    pub url: String,
    pub provider: String,
    pub folder_id: Option<i64>,
    pub created_by: Option<String>,
}

#[derive(Clone)]
pub struct MediaRepository {
    pools: DbPools,
}

impl MediaRepository {
    pub fn new(pools: DbPools) -> Self {
        Self { pools }
    }

    // --- Folders ---

    pub async fn list_folders(&self, parent_id: Option<i64>) -> ApiResult<Vec<MediaFolderDto>> {
        let rows = if let Some(pid) = parent_id {
            sqlx::query(
                r#"
                SELECT f.id, f.name, f.path_id, f.path, f.parent_id, f.created_at, f.updated_at,
                       (SELECT COUNT(*) FROM upload_files uf WHERE uf.folder_id = f.id) AS file_count,
                       (SELECT COUNT(*) FROM upload_folders ch WHERE ch.parent_id = f.id) AS child_count
                FROM upload_folders f
                WHERE f.parent_id = ?
                ORDER BY f.name ASC
                "#,
            )
            .bind(pid)
            .fetch_all(&self.pools.reader)
            .await
        } else {
            sqlx::query(
                r#"
                SELECT f.id, f.name, f.path_id, f.path, f.parent_id, f.created_at, f.updated_at,
                       (SELECT COUNT(*) FROM upload_files uf WHERE uf.folder_id = f.id) AS file_count,
                       (SELECT COUNT(*) FROM upload_folders ch WHERE ch.parent_id = f.id) AS child_count
                FROM upload_folders f
                ORDER BY f.path ASC, f.name ASC
                "#,
            )
            .fetch_all(&self.pools.reader)
            .await
        }
        .map_err(|e| ApiError::Internal(format!("list_folders failed: {e}")))?;

        Ok(rows.into_iter().map(map_folder_row).collect())
    }

    pub async fn get_folder(&self, id: i64) -> ApiResult<Option<MediaFolderDto>> {
        let row = sqlx::query(
            r#"
            SELECT f.id, f.name, f.path_id, f.path, f.parent_id, f.created_at, f.updated_at,
                   (SELECT COUNT(*) FROM upload_files uf WHERE uf.folder_id = f.id) AS file_count,
                   (SELECT COUNT(*) FROM upload_folders ch WHERE ch.parent_id = f.id) AS child_count
            FROM upload_folders f
            WHERE f.id = ?
            LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get_folder failed: {e}")))?;

        Ok(row.map(map_folder_row))
    }

    pub async fn create_folder(
        &self,
        name: &str,
        parent_id: Option<i64>,
    ) -> ApiResult<MediaFolderDto> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ApiError::Validation {
                message: "Folder name is required".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }

        let parent_path = if let Some(pid) = parent_id {
            let parent = self
                .get_folder(pid)
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Parent folder {pid} not found")))?;
            parent.path
        } else {
            String::new()
        };

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin create_folder tx failed: {e}")))?;

        let next_path_id: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(path_id), 0) + 1 FROM upload_folders")
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("next path_id failed: {e}")))?;

        let path = if parent_path.is_empty() {
            format!("/{next_path_id}")
        } else {
            format!("{parent_path}/{next_path_id}")
        };

        let res = sqlx::query(
            "INSERT INTO upload_folders (name, path_id, path, parent_id) VALUES (?, ?, ?, ?)",
        )
        .bind(trimmed)
        .bind(next_path_id)
        .bind(&path)
        .bind(parent_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("insert folder failed: {e}")))?;

        let id = res.last_insert_rowid();
        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit create_folder failed: {e}")))?;

        self.get_folder(id)
            .await?
            .ok_or_else(|| ApiError::Internal("Created folder not found".into()))
    }

    pub async fn rename_folder(&self, id: i64, name: &str) -> ApiResult<MediaFolderDto> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ApiError::Validation {
                message: "Folder name is required".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }
        let res = sqlx::query(
            "UPDATE upload_folders SET name = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(trimmed)
        .bind(id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("rename_folder failed: {e}")))?;

        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("Folder {id} not found")));
        }

        self.get_folder(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Folder {id} not found")))
    }

    pub async fn delete_folder(&self, id: i64) -> ApiResult<()> {
        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin delete_folder tx failed: {e}")))?;

        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM upload_folders WHERE id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("check folder exists failed: {e}")))?;
        if exists == 0 {
            return Err(ApiError::NotFound(format!("Folder {id} not found")));
        }

        let file_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM upload_files WHERE folder_id = ?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("count folder files failed: {e}")))?;
        let child_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM upload_folders WHERE parent_id = ?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("count child folders failed: {e}")))?;

        if file_count > 0 || child_count > 0 {
            return Err(ApiError::Conflict(
                "Folder is not empty — move or delete its files and subfolders first".into(),
            ));
        }

        sqlx::query("DELETE FROM upload_folders WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete_folder failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit delete_folder failed: {e}")))?;
        Ok(())
    }

    // --- Files ---

    pub async fn find_by_hash(&self, hash: &str) -> ApiResult<Option<MediaFileDto>> {
        let row = sqlx::query(
            r#"
            SELECT uf.*,
                   (SELECT COUNT(DISTINCT from_document_id) FROM upload_file_relations ufr WHERE ufr.file_id = uf.id) AS usage_count
            FROM upload_files uf
            WHERE uf.hash = ?
            LIMIT 1
            "#,
        )
        .bind(hash)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("find_by_hash failed: {e}")))?;

        Ok(row.map(map_file_row))
    }

    pub async fn count_by_url(&self, url: &str) -> ApiResult<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM upload_files WHERE url = ?")
            .bind(url)
            .fetch_one(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("count_by_url failed: {e}")))
    }

    pub async fn insert_file(&self, input: InsertMediaInput) -> ApiResult<MediaFileDto> {
        let folder_path = if let Some(fid) = input.folder_id {
            self.get_folder(fid)
                .await?
                .ok_or_else(|| ApiError::BadRequest(format!("Folder {fid} does not exist")))?
                .path
        } else {
            "/".to_string()
        };

        let doc_id = DocumentId::generate().to_string();
        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin insert_file tx failed: {e}")))?;

        let res = sqlx::query(
            r#"
            INSERT INTO upload_files (
                document_id, name, alternative_text, caption, width, height,
                formats_json, hash, ext, mime, size_kb, url, provider,
                folder_id, folder_path, created_by
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&doc_id)
        .bind(&input.name)
        .bind(&input.alternative_text)
        .bind(&input.caption)
        .bind(input.width)
        .bind(input.height)
        .bind(&input.formats_json)
        .bind(&input.hash)
        .bind(&input.ext)
        .bind(&input.mime)
        .bind(input.size_kb)
        .bind(&input.url)
        .bind(&input.provider)
        .bind(input.folder_id)
        .bind(&folder_path)
        .bind(&input.created_by)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("insert upload_file failed: {e}")))?;

        let id = res.last_insert_rowid();

        sqlx::query(
            "INSERT INTO fts_upload_files (file_id, name, alternative_text, caption, mime) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&input.name)
        .bind(input.alternative_text.as_deref().unwrap_or(""))
        .bind(input.caption.as_deref().unwrap_or(""))
        .bind(&input.mime)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("insert fts_upload_files failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit insert_file failed: {e}")))?;

        self.get_file(id)
            .await?
            .ok_or_else(|| ApiError::Internal("Inserted file not found".into()))
    }

    pub async fn get_file(&self, id: i64) -> ApiResult<Option<MediaFileDto>> {
        let row = sqlx::query(
            r#"
            SELECT uf.*,
                   (SELECT COUNT(DISTINCT from_document_id) FROM upload_file_relations ufr WHERE ufr.file_id = uf.id) AS usage_count
            FROM upload_files uf
            WHERE uf.id = ?
            LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get_file failed: {e}")))?;

        Ok(row.map(map_file_row))
    }

    pub async fn list_files(
        &self,
        filter: &MediaListFilter,
    ) -> ApiResult<(Vec<MediaFileDto>, PaginationMeta)> {
        let mut clauses = vec!["1 = 1".to_string()];
        // Keep bind order explicit via enum
        enum BindVal {
            Str(String),
            Int(i64),
        }
        let mut binds: Vec<BindVal> = Vec::new();

        if let Some(folder_opt) = filter.folder_id {
            match folder_opt {
                Some(fid) => {
                    clauses.push("uf.folder_id = ?".into());
                    binds.push(BindVal::Int(fid));
                }
                None => {
                    clauses.push("uf.folder_id IS NULL".into());
                }
            }
        }

        if let Some(prefix) = filter.mime_prefix.as_deref().filter(|s| !s.is_empty()) {
            clauses.push("uf.mime LIKE ?".into());
            binds.push(BindVal::Str(format!("{prefix}%")));
        }

        if let Some(q) = filter.search_query.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let tokens: Vec<String> = q
                .split_whitespace()
                .map(|t| {
                    t.chars()
                        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                        .collect::<String>()
                })
                .filter(|t| !t.is_empty())
                .map(|t| format!("\"{t}\"*"))
                .collect();
            if !tokens.is_empty() {
                clauses.push(
                    "uf.id IN (SELECT CAST(file_id AS INTEGER) FROM fts_upload_files WHERE fts_upload_files MATCH ?)"
                        .into(),
                );
                binds.push(BindVal::Str(tokens.join(" AND ")));
            } else {
                clauses.push("uf.name LIKE ?".into());
                binds.push(BindVal::Str(format!("%{q}%")));
            }
        }

        let where_sql = clauses.join(" AND ");

        let count_sql = format!("SELECT COUNT(*) FROM upload_files uf WHERE {where_sql}");
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
        for b in &binds {
            count_q = match b {
                BindVal::Str(s) => count_q.bind(s.clone()),
                BindVal::Int(i) => count_q.bind(*i),
            };
        }
        let total = count_q
            .fetch_one(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("count upload_files failed: {e}")))?
            as u64;

        let page = filter.page.max(1);
        let page_size = if filter.page_size == 0 {
            24
        } else {
            filter.page_size.clamp(1, 200)
        };
        let offset = (page - 1) * page_size;

        let select_sql = format!(
            r#"
            SELECT uf.*,
                   (SELECT COUNT(DISTINCT from_document_id) FROM upload_file_relations ufr WHERE ufr.file_id = uf.id) AS usage_count
            FROM upload_files uf
            WHERE {where_sql}
            ORDER BY uf.created_at DESC, uf.id DESC
            LIMIT ? OFFSET ?
            "#
        );
        let mut sel_q = sqlx::query(&select_sql);
        for b in &binds {
            sel_q = match b {
                BindVal::Str(s) => sel_q.bind(s.clone()),
                BindVal::Int(i) => sel_q.bind(*i),
            };
        }
        sel_q = sel_q.bind(page_size as i64).bind(offset as i64);

        let rows = sel_q
            .fetch_all(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("list upload_files failed: {e}")))?;

        let files = rows.into_iter().map(map_file_row).collect();
        Ok((files, PaginationMeta::compute(page, page_size, total)))
    }

    pub async fn update_file_metadata(
        &self,
        id: i64,
        name: Option<&str>,
        alternative_text: Option<Option<&str>>,
        caption: Option<Option<&str>>,
        folder_id: Option<Option<i64>>,
    ) -> ApiResult<MediaFileDto> {
        let existing = self
            .get_file(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Media file {id} not found")))?;

        let next_name = name.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&existing.name);
        let next_alt: Option<String> = match alternative_text {
            Some(v) => v.map(String::from),
            None => existing.alternative_text.clone(),
        };
        let next_cap: Option<String> = match caption {
            Some(v) => v.map(String::from),
            None => existing.caption.clone(),
        };
        let next_folder_id: Option<i64> = match folder_id {
            Some(v) => v,
            None => existing.folder_id,
        };
        let next_folder_path = if let Some(fid) = next_folder_id {
            self.get_folder(fid)
                .await?
                .ok_or_else(|| ApiError::BadRequest(format!("Folder {fid} does not exist")))?
                .path
        } else {
            "/".to_string()
        };

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin update_file tx failed: {e}")))?;

        sqlx::query(
            r#"
            UPDATE upload_files
            SET name = ?, alternative_text = ?, caption = ?, folder_id = ?, folder_path = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?
            "#,
        )
        .bind(next_name)
        .bind(&next_alt)
        .bind(&next_cap)
        .bind(next_folder_id)
        .bind(&next_folder_path)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("update upload_files failed: {e}")))?;

        sqlx::query("DELETE FROM fts_upload_files WHERE file_id = ?")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete fts_upload_files failed: {e}")))?;

        sqlx::query(
            "INSERT INTO fts_upload_files (file_id, name, alternative_text, caption, mime) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(next_name)
        .bind(next_alt.as_deref().unwrap_or(""))
        .bind(next_cap.as_deref().unwrap_or(""))
        .bind(&existing.mime)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("insert fts_upload_files failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit update_file failed: {e}")))?;

        self.get_file(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Media file {id} not found")))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_file_binary_details(
        &self,
        id: i64,
        width: i64,
        height: i64,
        size_kb: f64,
        hash: &str,
        url: &str,
        formats_json: &str,
    ) -> ApiResult<MediaFileDto> {
        sqlx::query(
            r#"
            UPDATE upload_files
            SET width = ?, height = ?, size_kb = ?, hash = ?, url = ?, formats_json = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?
            "#,
        )
        .bind(width)
        .bind(height)
        .bind(size_kb)
        .bind(hash)
        .bind(url)
        .bind(formats_json)
        .bind(id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("update_file_binary_details failed: {e}")))?;

        self.get_file(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Media file {id} not found")))
    }

    pub async fn update_formats_json(&self, id: i64, formats_json: &str) -> ApiResult<()> {
        sqlx::query(
            "UPDATE upload_files SET formats_json = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(formats_json)
        .bind(id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("update_formats_json failed: {e}")))?;
        Ok(())
    }

    /// Deletes a media file row (blocking if referenced by documents unless `force == true`)
    /// and returns the deleted `MediaFileDto` so the service can clean up unreferenced disk objects.
    pub async fn delete_file(&self, id: i64, force: bool) -> ApiResult<MediaFileDto> {
        let existing = self
            .get_file(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Media file {id} not found")))?;

        if existing.usage_count > 0 && !force {
            return Err(ApiError::Conflict(format!(
                "Media asset '{}' is referenced by {} document(s). Pass ?force=true to unlink and delete.",
                existing.name, existing.usage_count
            )));
        }

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin delete_file tx failed: {e}")))?;

        sqlx::query("DELETE FROM fts_upload_files WHERE file_id = ?")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete fts_upload_files failed: {e}")))?;

        sqlx::query("DELETE FROM upload_files WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete upload_files failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit delete_file failed: {e}")))?;

        Ok(existing)
    }
}

fn map_folder_row(r: sqlx::sqlite::SqliteRow) -> MediaFolderDto {
    MediaFolderDto {
        id: r.get("id"),
        name: r.get("name"),
        path_id: r.get("path_id"),
        path: r.get("path"),
        parent_id: r.get("parent_id"),
        file_count: r.get("file_count"),
        child_count: r.get("child_count"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn map_file_row(r: sqlx::sqlite::SqliteRow) -> MediaFileDto {
    let formats_raw: String = r.get("formats_json");
    let formats: serde_json::Value =
        serde_json::from_str(&formats_raw).unwrap_or_else(|_| serde_json::json!({}));

    MediaFileDto {
        id: r.get("id"),
        document_id: r.get("document_id"),
        name: r.get("name"),
        alternative_text: r.get("alternative_text"),
        caption: r.get("caption"),
        width: r.get("width"),
        height: r.get("height"),
        formats,
        hash: r.get("hash"),
        ext: r.get("ext"),
        mime: r.get("mime"),
        size: r.get("size_kb"),
        url: r.get("url"),
        preview_url: r.get("preview_url"),
        provider: r.get("provider"),
        folder_id: r.get("folder_id"),
        folder_path: r.get("folder_path"),
        usage_count: r.get("usage_count"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}
