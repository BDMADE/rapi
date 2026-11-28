use crate::pool::DbPools;
use cms_core::{
    ApiError, ApiResult, AuditLogDto, AuditLogFilter, DocumentLocaleStateDto, LocaleDto,
    PaginationMeta, PreviewConfigDto, ReleaseDto, ReleaseEntryDto, WorkflowEntryDto,
    WorkflowStageDto,
};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct InsertAuditLogInput {
    pub action: String,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub target_uid: Option<String>,
    pub target_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub payload_json: String,
}

#[derive(Clone)]
pub struct GovernanceRepository {
    pools: DbPools,
}

impl GovernanceRepository {
    pub fn new(pools: DbPools) -> Self {
        Self { pools }
    }

    // =========================================================================
    // Internationalization (`locales` + per-document locale states)
    // =========================================================================

    pub async fn list_locales(&self) -> ApiResult<Vec<LocaleDto>> {
        let rows = sqlx::query(
            "SELECT id, code, name, is_default, created_at, updated_at FROM locales ORDER BY is_default DESC, code ASC",
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_locales failed: {e}")))?;

        Ok(rows.into_iter().map(map_locale_row).collect())
    }

    pub async fn get_default_locale_code(&self) -> ApiResult<String> {
        let code: Option<String> =
            sqlx::query_scalar("SELECT code FROM locales WHERE is_default = 1 LIMIT 1")
                .fetch_optional(&self.pools.reader)
                .await
                .map_err(|e| ApiError::Internal(format!("get_default_locale_code failed: {e}")))?;
        Ok(code.unwrap_or_else(|| "en".to_string()))
    }

    pub async fn locale_exists(&self, code: &str) -> ApiResult<bool> {
        let cnt: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM locales WHERE code = ?")
            .bind(code)
            .fetch_one(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("locale_exists failed: {e}")))?;
        Ok(cnt > 0)
    }

    pub async fn create_locale(
        &self,
        code: &str,
        name: &str,
        is_default: bool,
    ) -> ApiResult<LocaleDto> {
        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin create_locale tx failed: {e}")))?;

        let dup: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM locales WHERE code = ?")
            .bind(code)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("check duplicate locale failed: {e}")))?;
        if dup > 0 {
            return Err(ApiError::Conflict(format!(
                "Locale '{code}' already exists"
            )));
        }

        if is_default {
            sqlx::query(
                "UPDATE locales SET is_default = 0, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE is_default = 1",
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("unset previous default locale failed: {e}")))?;
        }

        let res = sqlx::query("INSERT INTO locales (code, name, is_default) VALUES (?, ?, ?)")
            .bind(code)
            .bind(name)
            .bind(if is_default { 1 } else { 0 })
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("insert locale failed: {e}")))?;

        let id = res.last_insert_rowid();
        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit create_locale failed: {e}")))?;

        self.get_locale_by_id(id)
            .await?
            .ok_or_else(|| ApiError::Internal("Created locale not found".into()))
    }

    pub async fn update_locale(
        &self,
        id: i64,
        name: Option<&str>,
        is_default: Option<bool>,
    ) -> ApiResult<LocaleDto> {
        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin update_locale tx failed: {e}")))?;

        let existing = sqlx::query(
            "SELECT id, code, name, is_default, created_at, updated_at FROM locales WHERE id = ? LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("fetch locale failed: {e}")))?
        .map(map_locale_row)
        .ok_or_else(|| ApiError::NotFound(format!("Locale {id} not found")))?;

        if is_default == Some(false) && existing.is_default {
            return Err(ApiError::BadRequest(
                "Cannot unset the default locale directly — promote another locale to default instead"
                    .into(),
            ));
        }

        let next_name = name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.name);
        let next_default = is_default.unwrap_or(existing.is_default);

        if next_default && !existing.is_default {
            sqlx::query(
                "UPDATE locales SET is_default = 0, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE is_default = 1",
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("unset previous default locale failed: {e}")))?;
        }

        sqlx::query(
            "UPDATE locales SET name = ?, is_default = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(next_name)
        .bind(if next_default { 1 } else { 0 })
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("update locale failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit update_locale failed: {e}")))?;

        self.get_locale_by_id(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Locale {id} not found")))
    }

    pub async fn delete_locale(&self, id: i64) -> ApiResult<LocaleDto> {
        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin delete_locale tx failed: {e}")))?;

        let existing = sqlx::query(
            "SELECT id, code, name, is_default, created_at, updated_at FROM locales WHERE id = ? LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("fetch locale failed: {e}")))?
        .map(map_locale_row)
        .ok_or_else(|| ApiError::NotFound(format!("Locale {id} not found")))?;

        if existing.is_default {
            return Err(ApiError::BadRequest(
                "Cannot delete the default locale".into(),
            ));
        }

        let doc_count: i64 =
            sqlx::query_scalar("SELECT COUNT(DISTINCT document_id) FROM documents WHERE locale = ?")
                .bind(&existing.code)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("count locale documents failed: {e}")))?;
        if doc_count > 0 {
            return Err(ApiError::Conflict(format!(
                "Cannot delete locale '{}': {} document(s) still have variants in this locale",
                existing.code, doc_count
            )));
        }

        sqlx::query("DELETE FROM locales WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete locale failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit delete_locale failed: {e}")))?;

        Ok(existing)
    }

    pub async fn get_locale_by_id(&self, id: i64) -> ApiResult<Option<LocaleDto>> {
        let row = sqlx::query(
            "SELECT id, code, name, is_default, created_at, updated_at FROM locales WHERE id = ? LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get_locale_by_id failed: {e}")))?;
        Ok(row.map(map_locale_row))
    }

    pub async fn list_document_locales(
        &self,
        content_type_uid: &str,
        document_id: &str,
    ) -> ApiResult<Vec<DocumentLocaleStateDto>> {
        let rows = sqlx::query(
            r#"
            SELECT locale,
                   MAX(CASE WHEN status = 'draft' THEN 1 ELSE 0 END) AS has_draft,
                   MAX(CASE WHEN status = 'published' THEN 1 ELSE 0 END) AS has_published,
                   MAX(updated_at) AS updated_at
            FROM documents
            WHERE content_type_uid = ? AND document_id = ?
            GROUP BY locale
            ORDER BY locale ASC
            "#,
        )
        .bind(content_type_uid)
        .bind(document_id)
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_document_locales failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| DocumentLocaleStateDto {
                locale: r.get("locale"),
                has_draft: r.get::<i64, _>("has_draft") != 0,
                has_published: r.get::<i64, _>("has_published") != 0,
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    // =========================================================================
    // Preview Configurations (`preview_configs`)
    // =========================================================================

    pub async fn list_preview_configs(&self) -> ApiResult<Vec<PreviewConfigDto>> {
        let rows = sqlx::query(
            "SELECT content_type_uid, url_template, is_enabled, open_mode, updated_at FROM preview_configs ORDER BY content_type_uid ASC",
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_preview_configs failed: {e}")))?;

        Ok(rows.into_iter().map(map_preview_row).collect())
    }

    pub async fn get_preview_config(
        &self,
        content_type_uid: &str,
    ) -> ApiResult<Option<PreviewConfigDto>> {
        let row = sqlx::query(
            "SELECT content_type_uid, url_template, is_enabled, open_mode, updated_at FROM preview_configs WHERE content_type_uid = ? LIMIT 1",
        )
        .bind(content_type_uid)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get_preview_config failed: {e}")))?;
        Ok(row.map(map_preview_row))
    }

    pub async fn upsert_preview_config(
        &self,
        content_type_uid: &str,
        url_template: &str,
        is_enabled: bool,
        open_mode: &str,
    ) -> ApiResult<PreviewConfigDto> {
        let mode = if open_mode == "tab" { "tab" } else { "split" };
        sqlx::query(
            r#"
            INSERT INTO preview_configs (content_type_uid, url_template, is_enabled, open_mode, updated_at)
            VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            ON CONFLICT(content_type_uid) DO UPDATE SET
                url_template = excluded.url_template,
                is_enabled = excluded.is_enabled,
                open_mode = excluded.open_mode,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(content_type_uid)
        .bind(url_template)
        .bind(if is_enabled { 1 } else { 0 })
        .bind(mode)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("FOREIGN KEY") {
                ApiError::NotFound(format!("Content type '{content_type_uid}' not found"))
            } else {
                ApiError::Internal(format!("upsert_preview_config failed: {e}"))
            }
        })?;

        self.get_preview_config(content_type_uid)
            .await?
            .ok_or_else(|| ApiError::Internal("Saved preview config not found".into()))
    }

    pub async fn delete_preview_config(&self, content_type_uid: &str) -> ApiResult<()> {
        sqlx::query("DELETE FROM preview_configs WHERE content_type_uid = ?")
            .bind(content_type_uid)
            .execute(&self.pools.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("delete_preview_config failed: {e}")))?;
        Ok(())
    }

    // =========================================================================
    // Releases (`releases` + `release_entries`)
    // =========================================================================

    pub async fn list_releases(&self) -> ApiResult<Vec<ReleaseDto>> {
        let rows = sqlx::query(
            r#"
            SELECT r.id, r.name, r.scheduled_at, r.timezone, r.status, r.released_at,
                   r.error_message, r.created_by, r.created_at
            FROM releases r
            ORDER BY CASE r.status WHEN 'pending' THEN 0 WHEN 'failed' THEN 1 ELSE 2 END,
                     COALESCE(r.scheduled_at, r.created_at) ASC
            "#,
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_releases failed: {e}")))?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let id: String = r.get("id");
            let entries = self.list_release_entries(&id).await?;
            out.push(map_release_row(r, entries));
        }
        Ok(out)
    }

    pub async fn get_release(&self, id: &str) -> ApiResult<Option<ReleaseDto>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, scheduled_at, timezone, status, released_at,
                   error_message, created_by, created_at
            FROM releases
            WHERE id = ?
            LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get_release failed: {e}")))?;

        let Some(r) = row else {
            return Ok(None);
        };
        let entries = self.list_release_entries(id).await?;
        Ok(Some(map_release_row(r, entries)))
    }

    pub async fn create_release(
        &self,
        name: &str,
        scheduled_at: Option<&str>,
        timezone: &str,
        created_by: Option<&str>,
    ) -> ApiResult<ReleaseDto> {
        let id = format!("rel_{}", Uuid::now_v7().simple());
        sqlx::query(
            "INSERT INTO releases (id, name, scheduled_at, timezone, status, created_by) VALUES (?, ?, ?, ?, 'pending', ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(scheduled_at)
        .bind(timezone)
        .bind(created_by)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("create_release failed: {e}")))?;

        self.get_release(&id)
            .await?
            .ok_or_else(|| ApiError::Internal("Created release not found".into()))
    }

    pub async fn update_release(
        &self,
        id: &str,
        name: Option<&str>,
        scheduled_at: Option<Option<&str>>,
        timezone: Option<&str>,
    ) -> ApiResult<ReleaseDto> {
        let existing = self
            .get_release(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Release '{id}' not found")))?;

        if existing.status == "done" {
            return Err(ApiError::BadRequest(
                "Completed releases are immutable".into(),
            ));
        }

        let next_name = name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.name);
        let next_sched: Option<String> = match scheduled_at {
            Some(v) => v.map(String::from),
            None => existing.scheduled_at.clone(),
        };
        let next_tz = timezone
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.timezone);

        sqlx::query(
            "UPDATE releases SET name = ?, scheduled_at = ?, timezone = ?, status = 'pending', error_message = NULL WHERE id = ?",
        )
        .bind(next_name)
        .bind(&next_sched)
        .bind(next_tz)
        .bind(id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("update_release failed: {e}")))?;

        self.get_release(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Release '{id}' not found")))
    }

    pub async fn delete_release(&self, id: &str) -> ApiResult<()> {
        let res = sqlx::query("DELETE FROM releases WHERE id = ?")
            .bind(id)
            .execute(&self.pools.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("delete_release failed: {e}")))?;
        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("Release '{id}' not found")));
        }
        Ok(())
    }

    pub async fn add_release_entry(
        &self,
        release_id: &str,
        content_type_uid: &str,
        document_id: &str,
        locale: &str,
        action: &str,
    ) -> ApiResult<ReleaseDto> {
        let rel = self
            .get_release(release_id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Release '{release_id}' not found")))?;

        if rel.status == "done" {
            return Err(ApiError::BadRequest(
                "Cannot modify entries of a completed release".into(),
            ));
        }

        let act = if action == "unpublish" {
            "unpublish"
        } else {
            "publish"
        };

        // Verify target document exists
        let doc_exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM documents WHERE content_type_uid = ? AND document_id = ? AND locale = ?",
        )
        .bind(content_type_uid)
        .bind(document_id)
        .bind(locale)
        .fetch_one(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("verify release document failed: {e}")))?;
        if doc_exists == 0 {
            return Err(ApiError::NotFound(format!(
                "Document '{document_id}' ({locale}) not found in '{content_type_uid}'"
            )));
        }

        sqlx::query(
            r#"
            INSERT INTO release_entries (release_id, content_type_uid, document_id, locale, action)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(release_id, document_id, locale) DO UPDATE SET
                content_type_uid = excluded.content_type_uid,
                action = excluded.action
            "#,
        )
        .bind(release_id)
        .bind(content_type_uid)
        .bind(document_id)
        .bind(locale)
        .bind(act)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("add_release_entry failed: {e}")))?;

        self.get_release(release_id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Release '{release_id}' not found")))
    }

    pub async fn remove_release_entry(
        &self,
        release_id: &str,
        entry_id: i64,
    ) -> ApiResult<ReleaseDto> {
        let rel = self
            .get_release(release_id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Release '{release_id}' not found")))?;
        if rel.status == "done" {
            return Err(ApiError::BadRequest(
                "Cannot modify entries of a completed release".into(),
            ));
        }

        sqlx::query("DELETE FROM release_entries WHERE release_id = ? AND id = ?")
            .bind(release_id)
            .bind(entry_id)
            .execute(&self.pools.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("remove_release_entry failed: {e}")))?;

        self.get_release(release_id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Release '{release_id}' not found")))
    }

    pub async fn mark_release_status(
        &self,
        id: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> ApiResult<ReleaseDto> {
        if status == "done" {
            sqlx::query(
                "UPDATE releases SET status = 'done', released_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), error_message = NULL WHERE id = ?",
            )
            .bind(id)
            .execute(&self.pools.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("mark release done failed: {e}")))?;
        } else {
            sqlx::query("UPDATE releases SET status = ?, error_message = ? WHERE id = ?")
                .bind(status)
                .bind(error_message)
                .bind(id)
                .execute(&self.pools.writer)
                .await
                .map_err(|e| ApiError::Internal(format!("mark release status failed: {e}")))?;
        }

        self.get_release(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Release '{id}' not found")))
    }

    pub async fn list_due_release_ids(&self, now_iso: &str) -> ApiResult<Vec<String>> {
        sqlx::query_scalar(
            "SELECT id FROM releases WHERE status = 'pending' AND scheduled_at IS NOT NULL AND scheduled_at <= ? ORDER BY scheduled_at ASC LIMIT 25",
        )
        .bind(now_iso)
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_due_release_ids failed: {e}")))
    }

    async fn list_release_entries(&self, release_id: &str) -> ApiResult<Vec<ReleaseEntryDto>> {
        let rows = sqlx::query(
            "SELECT id, release_id, content_type_uid, document_id, locale, action FROM release_entries WHERE release_id = ? ORDER BY id ASC",
        )
        .bind(release_id)
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_release_entries failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| ReleaseEntryDto {
                id: r.get("id"),
                release_id: r.get("release_id"),
                content_type_uid: r.get("content_type_uid"),
                document_id: r.get("document_id"),
                locale: r.get("locale"),
                action: r.get("action"),
            })
            .collect())
    }

    // =========================================================================
    // Review Workflows (`workflow_stages` + `workflow_entries`)
    // =========================================================================

    pub async fn list_workflow_stages(&self) -> ApiResult<Vec<WorkflowStageDto>> {
        let rows = sqlx::query(
            "SELECT id, name, color, position, allowed_role_ids_json, require_for_publish FROM workflow_stages ORDER BY position ASC, id ASC",
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_workflow_stages failed: {e}")))?;

        Ok(rows.into_iter().map(map_stage_row).collect())
    }

    pub async fn get_workflow_stage(&self, id: &str) -> ApiResult<Option<WorkflowStageDto>> {
        let row = sqlx::query(
            "SELECT id, name, color, position, allowed_role_ids_json, require_for_publish FROM workflow_stages WHERE id = ? LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get_workflow_stage failed: {e}")))?;
        Ok(row.map(map_stage_row))
    }

    pub async fn create_workflow_stage(
        &self,
        name: &str,
        color: &str,
        allowed_role_ids: &[String],
        require_for_publish: bool,
    ) -> ApiResult<WorkflowStageDto> {
        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin create_workflow_stage tx: {e}")))?;

        let next_pos: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(position), -1) + 1 FROM workflow_stages")
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("max stage pos failed: {e}")))?;

        let id = format!("stage_{}", Uuid::now_v7().simple());
        let roles_json =
            serde_json::to_string(allowed_role_ids).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            "INSERT INTO workflow_stages (id, name, color, position, allowed_role_ids_json, require_for_publish) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(color)
        .bind(next_pos)
        .bind(&roles_json)
        .bind(if require_for_publish { 1 } else { 0 })
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("insert workflow_stage failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit workflow_stage failed: {e}")))?;

        self.get_workflow_stage(&id)
            .await?
            .ok_or_else(|| ApiError::Internal("Created workflow stage not found".into()))
    }

    pub async fn update_workflow_stage(
        &self,
        id: &str,
        name: Option<&str>,
        color: Option<&str>,
        position: Option<i64>,
        allowed_role_ids: Option<&[String]>,
        require_for_publish: Option<bool>,
    ) -> ApiResult<WorkflowStageDto> {
        let existing = self
            .get_workflow_stage(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Workflow stage '{id}' not found")))?;

        let next_name = name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.name);
        let next_color = color
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.color);
        let next_pos = position.unwrap_or(existing.position);
        let next_roles = allowed_role_ids
            .map(|r| r.to_vec())
            .unwrap_or(existing.allowed_role_ids);
        let next_req = require_for_publish.unwrap_or(existing.require_for_publish);
        let roles_json = serde_json::to_string(&next_roles).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            "UPDATE workflow_stages SET name = ?, color = ?, position = ?, allowed_role_ids_json = ?, require_for_publish = ? WHERE id = ?",
        )
        .bind(next_name)
        .bind(next_color)
        .bind(next_pos)
        .bind(&roles_json)
        .bind(if next_req { 1 } else { 0 })
        .bind(id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("update_workflow_stage failed: {e}")))?;

        self.get_workflow_stage(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Workflow stage '{id}' not found")))
    }

    pub async fn delete_workflow_stage(&self, id: &str) -> ApiResult<()> {
        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin delete_workflow_stage tx: {e}")))?;

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workflow_stages")
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("count stages failed: {e}")))?;
        if total <= 1 {
            return Err(ApiError::BadRequest(
                "At least one workflow stage must remain configured".into(),
            ));
        }

        let res = sqlx::query("DELETE FROM workflow_stages WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete_workflow_stage failed: {e}")))?;
        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("Workflow stage '{id}' not found")));
        }

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit delete_workflow_stage failed: {e}")))?;
        Ok(())
    }

    pub async fn get_workflow_entry(
        &self,
        document_id: &str,
        locale: &str,
    ) -> ApiResult<Option<WorkflowEntryDto>> {
        let row = sqlx::query(
            r#"
            SELECT we.document_id, we.locale, we.stage_id, ws.name AS stage_name, ws.color AS stage_color,
                   we.assignee_id, au.email AS assignee_email, we.updated_at
            FROM workflow_entries we
            JOIN workflow_stages ws ON ws.id = we.stage_id
            LEFT JOIN admin_users au ON au.id = we.assignee_id
            WHERE we.document_id = ? AND we.locale = ?
            LIMIT 1
            "#,
        )
        .bind(document_id)
        .bind(locale)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get_workflow_entry failed: {e}")))?;

        Ok(row.map(|r| WorkflowEntryDto {
            document_id: r.get("document_id"),
            locale: r.get("locale"),
            stage_id: r.get("stage_id"),
            stage_name: r.get("stage_name"),
            stage_color: r.get("stage_color"),
            assignee_id: r.get("assignee_id"),
            assignee_email: r.get("assignee_email"),
            updated_at: r.get("updated_at"),
        }))
    }

    pub async fn upsert_workflow_entry(
        &self,
        document_id: &str,
        locale: &str,
        stage_id: &str,
        assignee_id: Option<&str>,
    ) -> ApiResult<WorkflowEntryDto> {
        sqlx::query(
            r#"
            INSERT INTO workflow_entries (document_id, locale, stage_id, assignee_id, updated_at)
            VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            ON CONFLICT(document_id, locale) DO UPDATE SET
                stage_id = excluded.stage_id,
                assignee_id = excluded.assignee_id,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(document_id)
        .bind(locale)
        .bind(stage_id)
        .bind(assignee_id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("FOREIGN KEY") {
                ApiError::BadRequest("Invalid workflow stageId or assigneeId".into())
            } else {
                ApiError::Internal(format!("upsert_workflow_entry failed: {e}"))
            }
        })?;

        self.get_workflow_entry(document_id, locale)
            .await?
            .ok_or_else(|| ApiError::Internal("Saved workflow entry not found".into()))
    }

    // =========================================================================
    // Audit Logs (`audit_logs`)
    // =========================================================================

    pub async fn insert_audit_log(&self, input: InsertAuditLogInput) -> ApiResult<()> {
        let id = format!("aud_{}", Uuid::now_v7().simple());
        sqlx::query(
            r#"
            INSERT INTO audit_logs (
                id, action, actor_type, actor_id, target_uid, target_id,
                ip_address, user_agent, payload_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&input.action)
        .bind(&input.actor_type)
        .bind(&input.actor_id)
        .bind(&input.target_uid)
        .bind(&input.target_id)
        .bind(&input.ip_address)
        .bind(&input.user_agent)
        .bind(&input.payload_json)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("insert_audit_log failed: {e}")))?;
        Ok(())
    }

    pub async fn list_audit_logs(
        &self,
        filter: &AuditLogFilter,
    ) -> ApiResult<(Vec<AuditLogDto>, PaginationMeta)> {
        let mut clauses = vec!["1 = 1".to_string()];
        let mut binds: Vec<String> = Vec::new();

        if let Some(act) = filter.action.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            clauses.push("action LIKE ?".into());
            binds.push(format!("%{act}%"));
        }
        if let Some(aid) = filter.actor_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            clauses.push("actor_id = ?".into());
            binds.push(aid.to_string());
        }
        if let Some(tuid) = filter.target_uid.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            clauses.push("target_uid = ?".into());
            binds.push(tuid.to_string());
        }
        if let Some(df) = filter.date_from.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            clauses.push("created_at >= ?".into());
            binds.push(df.to_string());
        }
        if let Some(dt) = filter.date_to.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            clauses.push("created_at <= ?".into());
            binds.push(dt.to_string());
        }

        let where_sql = clauses.join(" AND ");
        let count_sql = format!("SELECT COUNT(*) FROM audit_logs WHERE {where_sql}");
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql);
        for b in &binds {
            cq = cq.bind(b.clone());
        }
        let total = cq
            .fetch_one(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("count audit_logs failed: {e}")))?
            as u64;

        let page = filter.page.max(1);
        let page_size = if filter.page_size == 0 {
            30
        } else {
            filter.page_size.clamp(1, 200)
        };
        let offset = (page - 1) * page_size;

        let sel_sql = format!(
            r#"
            SELECT id, action, actor_type, actor_id, target_uid, target_id,
                   ip_address, user_agent, payload_json, created_at
            FROM audit_logs
            WHERE {where_sql}
            ORDER BY created_at DESC, id DESC
            LIMIT ? OFFSET ?
            "#
        );
        let mut sq = sqlx::query(&sel_sql);
        for b in &binds {
            sq = sq.bind(b.clone());
        }
        sq = sq.bind(page_size as i64).bind(offset as i64);

        let rows = sq
            .fetch_all(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("list audit_logs failed: {e}")))?;

        let items = rows
            .into_iter()
            .map(|r| {
                let raw_p: String = r.get("payload_json");
                let payload =
                    serde_json::from_str(&raw_p).unwrap_or_else(|_| serde_json::json!({}));
                AuditLogDto {
                    id: r.get("id"),
                    action: r.get("action"),
                    actor_type: r.get("actor_type"),
                    actor_id: r.get("actor_id"),
                    target_uid: r.get("target_uid"),
                    target_id: r.get("target_id"),
                    ip_address: r.get("ip_address"),
                    user_agent: r.get("user_agent"),
                    payload,
                    created_at: r.get("created_at"),
                }
            })
            .collect();

        Ok((items, PaginationMeta::compute(page, page_size, total)))
    }
}

fn map_locale_row(r: sqlx::sqlite::SqliteRow) -> LocaleDto {
    LocaleDto {
        id: r.get("id"),
        code: r.get("code"),
        name: r.get("name"),
        is_default: r.get::<i64, _>("is_default") != 0,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn map_preview_row(r: sqlx::sqlite::SqliteRow) -> PreviewConfigDto {
    PreviewConfigDto {
        content_type_uid: r.get("content_type_uid"),
        url_template: r.get("url_template"),
        is_enabled: r.get::<i64, _>("is_enabled") != 0,
        open_mode: r.get("open_mode"),
        updated_at: r.get("updated_at"),
    }
}

fn map_release_row(r: sqlx::sqlite::SqliteRow, entries: Vec<ReleaseEntryDto>) -> ReleaseDto {
    let entry_count = entries.len() as i64;
    ReleaseDto {
        id: r.get("id"),
        name: r.get("name"),
        scheduled_at: r.get("scheduled_at"),
        timezone: r.get("timezone"),
        status: r.get("status"),
        released_at: r.get("released_at"),
        error_message: r.get("error_message"),
        entry_count,
        entries,
        created_by: r.get("created_by"),
        created_at: r.get("created_at"),
    }
}

fn map_stage_row(r: sqlx::sqlite::SqliteRow) -> WorkflowStageDto {
    let raw_roles: String = r.get("allowed_role_ids_json");
    let allowed_role_ids: Vec<String> = serde_json::from_str(&raw_roles).unwrap_or_default();
    WorkflowStageDto {
        id: r.get("id"),
        name: r.get("name"),
        color: r.get("color"),
        position: r.get("position"),
        allowed_role_ids,
        require_for_publish: r.get::<i64, _>("require_for_publish") != 0,
    }
}
