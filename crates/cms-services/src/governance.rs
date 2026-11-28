use crate::content::ContentService;
use crate::schema::SchemaService;
use cms_core::{
    validate_locale_code, validate_preview_url_template, Actor, ApiError, ApiResult, AuditLogDto,
    AuditLogFilter, CacheBackend, ContentKind, DocumentLocaleStateDto, DocumentStatus, JobMessage,
    LocaleDto, PaginationMeta, PopulateSpec, PreviewConfigDto, QueueBackend, ReleaseDto,
    ResolvedPreviewDto, WorkflowEntryDto, WorkflowStageDto,
};
use cms_db::{GovernanceRepository, InsertAuditLogInput};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_LOCALE_CACHE_KEY: &str = "rapi:locales:default";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLocaleInput {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLocaleInput {
    pub name: Option<String>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertPreviewConfigInput {
    pub url_template: String,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default = "default_split")]
    pub open_mode: String,
}

fn default_true() -> bool {
    true
}

fn default_split() -> String {
    "split".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateReleaseInput {
    pub name: String,
    pub scheduled_at: Option<String>,
    #[serde(default = "default_utc")]
    pub timezone: String,
}

fn default_utc() -> String {
    "UTC".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReleaseInput {
    pub name: Option<String>,
    pub scheduled_at: Option<Option<String>>,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddReleaseEntryInput {
    pub content_type_uid: String,
    pub document_id: String,
    pub locale: Option<String>,
    #[serde(default = "default_publish_action")]
    pub action: String,
}

fn default_publish_action() -> String {
    "publish".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkflowStageInput {
    pub name: String,
    #[serde(default = "default_stage_color")]
    pub color: String,
    #[serde(default)]
    pub allowed_role_ids: Vec<String>,
    #[serde(default)]
    pub require_for_publish: bool,
}

fn default_stage_color() -> String {
    "#4f46e5".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkflowStageInput {
    pub name: Option<String>,
    pub color: Option<String>,
    pub position: Option<i64>,
    pub allowed_role_ids: Option<Vec<String>>,
    pub require_for_publish: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertWorkflowEntryInput {
    pub locale: Option<String>,
    pub stage_id: String,
    pub assignee_id: Option<String>,
}

#[derive(Clone)]
pub struct GovernanceService {
    repo: GovernanceRepository,
    schema_service: SchemaService,
    content_service: ContentService,
    cache: Arc<dyn CacheBackend>,
    queue: Arc<dyn QueueBackend>,
}

impl GovernanceService {
    pub fn new(
        repo: GovernanceRepository,
        schema_service: SchemaService,
        content_service: ContentService,
        cache: Arc<dyn CacheBackend>,
        queue: Arc<dyn QueueBackend>,
    ) -> Self {
        Self {
            repo,
            schema_service,
            content_service,
            cache,
            queue,
        }
    }

    // =========================================================================
    // Internationalization (`i18n`)
    // =========================================================================

    pub async fn list_locales(&self) -> ApiResult<Vec<LocaleDto>> {
        self.repo.list_locales().await
    }

    pub async fn default_locale(&self) -> ApiResult<String> {
        if let Ok(Some(cached)) = self.cache.get(DEFAULT_LOCALE_CACHE_KEY).await {
            return Ok(cached);
        }
        let code = self.repo.get_default_locale_code().await?;
        let _ = self
            .cache
            .set(
                DEFAULT_LOCALE_CACHE_KEY,
                &code,
                Some(Duration::from_secs(300)),
            )
            .await;
        Ok(code)
    }

    /// Resolves and validates the effective locale for a content-type read or write:
    /// - If `requested` is `None`, returns the configured default locale.
    /// - If `schema.i18n_enabled == false` and a non-default locale is requested on mutation, returns `400 BadRequest`.
    /// - Ensures the requested locale exists in `locales`.
    pub async fn resolve_and_validate_locale(
        &self,
        i18n_enabled: bool,
        requested: Option<&str>,
        is_mutation: bool,
    ) -> ApiResult<String> {
        let def = self.default_locale().await?;
        let Some(req) = requested.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(def);
        };

        if !i18n_enabled && is_mutation && req != def {
            return Err(ApiError::BadRequest(format!(
                "Internationalization (i18n) is disabled on this content type; only default locale '{def}' is writable"
            )));
        }

        if req != def && !self.repo.locale_exists(req).await? {
            return Err(ApiError::Validation {
                message: format!("Locale '{req}' is not configured in i18n settings"),
                details: serde_json::json!({ "locale": req }),
            });
        }

        Ok(req.to_string())
    }

    pub async fn create_locale(&self, input: CreateLocaleInput) -> ApiResult<LocaleDto> {
        let code = input.code.trim();
        validate_locale_code(code)?;
        let name = input.name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(ApiError::Validation {
                message: "Locale display name must be 1..80 characters".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }
        let created = self
            .repo
            .create_locale(code, name, input.is_default)
            .await?;
        let _ = self.cache.delete(DEFAULT_LOCALE_CACHE_KEY).await;
        Ok(created)
    }

    pub async fn update_locale(&self, id: i64, input: UpdateLocaleInput) -> ApiResult<LocaleDto> {
        let updated = self
            .repo
            .update_locale(id, input.name.as_deref(), input.is_default)
            .await?;
        let _ = self.cache.delete(DEFAULT_LOCALE_CACHE_KEY).await;
        Ok(updated)
    }

    pub async fn delete_locale(&self, id: i64) -> ApiResult<LocaleDto> {
        let deleted = self.repo.delete_locale(id).await?;
        let _ = self.cache.delete(DEFAULT_LOCALE_CACHE_KEY).await;
        Ok(deleted)
    }

    pub async fn list_document_locales(
        &self,
        uid: &str,
        document_id: &str,
    ) -> ApiResult<Vec<DocumentLocaleStateDto>> {
        self.repo.list_document_locales(uid, document_id).await
    }

    /// Copies localized scalar fields from `source_locale` into `target_locale` draft for `document_id`.
    pub async fn fill_from_locale(
        &self,
        uid: &str,
        document_id: &str,
        source_locale: &str,
        target_locale: &str,
        actor_id: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let schema = self.schema_service.get_schema(uid).await?;
        if !schema.i18n_enabled {
            return Err(ApiError::BadRequest(
                "i18n is not enabled on this content type".into(),
            ));
        }
        let target = self
            .resolve_and_validate_locale(true, Some(target_locale), true)
            .await?;

        let read_status = if schema.draft_and_publish {
            DocumentStatus::Draft
        } else {
            DocumentStatus::Published
        };
        let src_doc = self
            .content_service
            .find_one(
                &schema,
                document_id,
                source_locale,
                read_status,
                &PopulateSpec::None,
                &[],
                false,
            )
            .await?;

        let mut patch = serde_json::Map::new();
        if let Some(obj) = src_doc.as_object() {
            for f in schema
                .fields
                .iter()
                .filter(|f| f.field_type.has_physical_column() && !f.unique)
            {
                if let Some(v) = obj.get(&f.name) {
                    patch.insert(f.name.clone(), v.clone());
                }
            }
        }

        let write_status = if schema.draft_and_publish {
            DocumentStatus::Draft
        } else {
            DocumentStatus::Published
        };
        self.content_service
            .update_entry(
                &schema,
                document_id,
                &target,
                write_status,
                &patch,
                actor_id,
                false,
            )
            .await
    }

    // =========================================================================
    // Preview (`preview_configs` + template interpolation)
    // =========================================================================

    pub async fn list_preview_configs(&self) -> ApiResult<Vec<PreviewConfigDto>> {
        self.repo.list_preview_configs().await
    }

    pub async fn upsert_preview_config(
        &self,
        uid: &str,
        input: UpsertPreviewConfigInput,
    ) -> ApiResult<PreviewConfigDto> {
        let schema = self.schema_service.get_schema(uid).await?;
        if schema.kind == ContentKind::Component {
            return Err(ApiError::BadRequest(
                "Components cannot have standalone preview URLs".into(),
            ));
        }
        validate_preview_url_template(&input.url_template)?;
        self.repo
            .upsert_preview_config(
                uid,
                input.url_template.trim(),
                input.is_enabled,
                &input.open_mode,
            )
            .await
    }

    pub async fn delete_preview_config(&self, uid: &str) -> ApiResult<()> {
        self.repo.delete_preview_config(uid).await
    }

    pub async fn resolve_preview_url(
        &self,
        uid: &str,
        document_id: &str,
        locale: &str,
        status: &str,
    ) -> ApiResult<ResolvedPreviewDto> {
        let schema = self.schema_service.get_schema(uid).await?;
        if let Some(cfg) = self.repo.get_preview_config(uid).await? {
            if cfg.is_enabled {
                let url = cfg
                    .url_template
                    .replace("{uid}", &urlencoding_segment(&schema.uid))
                    .replace("{pluralName}", &urlencoding_segment(&schema.plural_name))
                    .replace(
                        "{singularName}",
                        &urlencoding_segment(&schema.singular_name),
                    )
                    .replace("{documentId}", &urlencoding_segment(document_id))
                    .replace("{locale}", &urlencoding_segment(locale))
                    .replace("{status}", &urlencoding_segment(status));
                return Ok(ResolvedPreviewDto {
                    url,
                    open_mode: cfg.open_mode,
                    is_builtin: false,
                });
            }
        }

        // Built-in zero-config preview renderer when no external frontend URL is configured
        let builtin_url = format!(
            "/admin/preview/render/{}/{}?locale={}&status={}",
            urlencoding_segment(uid),
            urlencoding_segment(document_id),
            urlencoding_segment(locale),
            urlencoding_segment(status)
        );
        Ok(ResolvedPreviewDto {
            url: builtin_url,
            open_mode: "split".to_string(),
            is_builtin: true,
        })
    }

    // =========================================================================
    // Releases & Scheduled Publishing
    // =========================================================================

    pub async fn list_releases(&self) -> ApiResult<Vec<ReleaseDto>> {
        self.repo.list_releases().await
    }

    pub async fn get_release(&self, id: &str) -> ApiResult<ReleaseDto> {
        self.repo
            .get_release(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Release '{id}' not found")))
    }

    pub async fn create_release(
        &self,
        input: CreateReleaseInput,
        actor_id: Option<&str>,
    ) -> ApiResult<ReleaseDto> {
        let name = input.name.trim();
        if name.is_empty() || name.len() > 120 {
            return Err(ApiError::Validation {
                message: "Release name must be 1..120 characters".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }
        let sched = normalize_optional_iso(input.scheduled_at.as_deref())?;
        let created = self
            .repo
            .create_release(name, sched.as_deref(), input.timezone.trim(), actor_id)
            .await?;

        if sched.is_some() {
            let _ = self
                .queue
                .enqueue(JobMessage::new(
                    "rapi:jobs:release",
                    serde_json::json!({ "releaseId": created.id }),
                ))
                .await;
        }
        Ok(created)
    }

    pub async fn update_release(
        &self,
        id: &str,
        input: UpdateReleaseInput,
    ) -> ApiResult<ReleaseDto> {
        let sched_change = match &input.scheduled_at {
            Some(Some(raw)) => Some(normalize_optional_iso(Some(raw))?),
            Some(None) => Some(None),
            None => None,
        };
        self.repo
            .update_release(
                id,
                input.name.as_deref(),
                sched_change.as_ref().map(|o| o.as_deref()),
                input.timezone.as_deref(),
            )
            .await
    }

    pub async fn delete_release(&self, id: &str) -> ApiResult<()> {
        self.repo.delete_release(id).await
    }

    pub async fn add_release_entry(
        &self,
        release_id: &str,
        input: AddReleaseEntryInput,
    ) -> ApiResult<ReleaseDto> {
        let locale = match input.locale.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(l) => l.to_string(),
            None => self.default_locale().await?,
        };
        if input.action != "publish" && input.action != "unpublish" {
            return Err(ApiError::Validation {
                message: "Release entry action must be 'publish' or 'unpublish'".into(),
                details: serde_json::json!({ "field": "action" }),
            });
        }
        self.repo
            .add_release_entry(
                release_id,
                input.content_type_uid.trim(),
                input.document_id.trim(),
                &locale,
                &input.action,
            )
            .await
    }

    pub async fn remove_release_entry(
        &self,
        release_id: &str,
        entry_id: i64,
    ) -> ApiResult<ReleaseDto> {
        self.repo.remove_release_entry(release_id, entry_id).await
    }

    /// Executes a release group (acquiring `rapi:lock:release:<id>` so concurrent workers never double-fire).
    pub async fn execute_release(
        &self,
        release_id: &str,
        actor_id: Option<&str>,
    ) -> ApiResult<ReleaseDto> {
        let lock_key = format!("rapi:lock:release:{release_id}");
        if self.cache.get(&lock_key).await?.is_some() {
            return Err(ApiError::Conflict(format!(
                "Release '{release_id}' is already being executed"
            )));
        }
        let _ = self
            .cache
            .set(&lock_key, "1", Some(Duration::from_secs(60)))
            .await;

        let result = self.execute_release_inner(release_id, actor_id).await;
        let _ = self.cache.delete(&lock_key).await;
        result
    }

    async fn execute_release_inner(
        &self,
        release_id: &str,
        actor_id: Option<&str>,
    ) -> ApiResult<ReleaseDto> {
        let release = self.get_release(release_id).await?;
        if release.status == "done" {
            return Ok(release);
        }
        if release.entries.is_empty() {
            return Err(ApiError::Validation {
                message: "Cannot publish an empty release — add at least one entry first".into(),
                details: serde_json::json!({ "releaseId": release_id }),
            });
        }

        // Pre-flight validation pass over ALL entries so a failure on entry N never leaves entries 1..N-1 partially published!
        for entry in &release.entries {
            let preflight: ApiResult<()> = async {
                let schema = self.schema_service.get_schema(&entry.content_type_uid).await?;
                if entry.action == "publish" {
                    self.enforce_publish_workflow_gate(&entry.document_id, &entry.locale)
                        .await?;
                    if schema.draft_and_publish {
                        let draft = self
                            .content_service
                            .find_one(
                                &schema,
                                &entry.document_id,
                                &entry.locale,
                                DocumentStatus::Draft,
                                &PopulateSpec::None,
                                &[],
                                false,
                            )
                            .await?;
                        if let Some(obj) = draft.as_object() {
                            for f in schema
                                .fields
                                .iter()
                                .filter(|f| f.field_type.has_physical_column() && f.required)
                            {
                                let missing = match obj.get(&f.name) {
                                    None | Some(serde_json::Value::Null) => true,
                                    Some(serde_json::Value::String(s)) => s.trim().is_empty(),
                                    _ => false,
                                };
                                if missing {
                                    return Err(ApiError::Validation {
                                        message: format!(
                                            "Required field '{}' is empty on draft",
                                            f.name
                                        ),
                                        details: serde_json::json!({ "field": f.name }),
                                    });
                                }
                            }
                        }
                    }
                }
                Ok(())
            }
            .await;

            if let Err(err) = preflight {
                let err_msg = format!(
                    "Pre-flight failed on {} ({}): {}",
                    entry.document_id, entry.locale, err
                );
                let _ = self
                    .repo
                    .mark_release_status(release_id, "failed", Some(&err_msg))
                    .await;
                return Err(err);
            }
        }

        for entry in &release.entries {
            let step_res = if entry.action == "unpublish" {
                self.content_service
                    .unpublish_entry(&entry.content_type_uid, &entry.document_id, &entry.locale)
                    .await
                    .map(|_| ())
            } else {
                self.content_service
                    .publish_entry(
                        &entry.content_type_uid,
                        &entry.document_id,
                        &entry.locale,
                        actor_id,
                    )
                    .await
                    .map(|_| ())
            };

            if let Err(err) = step_res {
                let err_msg = format!(
                    "Failed on {} ({}): {}",
                    entry.document_id, entry.locale, err
                );
                let _ = self
                    .repo
                    .mark_release_status(release_id, "failed", Some(&err_msg))
                    .await;
                return Err(err);
            }
        }

        self.repo.mark_release_status(release_id, "done", None).await
    }

    /// Background worker sweep executing any pending releases whose `scheduled_at <= now`.
    pub async fn process_due_releases(&self) -> ApiResult<usize> {
        let now_iso = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let due_ids = self.repo.list_due_release_ids(&now_iso).await?;
        let mut completed = 0;
        for id in due_ids {
            if self.execute_release(&id, Some("system:release-worker")).await.is_ok() {
                completed += 1;
            }
        }
        Ok(completed)
    }

    // =========================================================================
    // Review Workflows & Publish Gates
    // =========================================================================

    pub async fn list_workflow_stages(&self) -> ApiResult<Vec<WorkflowStageDto>> {
        self.repo.list_workflow_stages().await
    }

    pub async fn create_workflow_stage(
        &self,
        input: CreateWorkflowStageInput,
    ) -> ApiResult<WorkflowStageDto> {
        let name = input.name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(ApiError::Validation {
                message: "Stage name must be 1..80 characters".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }
        let color = sanitize_hex_color(&input.color);
        self.repo
            .create_workflow_stage(
                name,
                &color,
                &input.allowed_role_ids,
                input.require_for_publish,
            )
            .await
    }

    pub async fn update_workflow_stage(
        &self,
        id: &str,
        input: UpdateWorkflowStageInput,
    ) -> ApiResult<WorkflowStageDto> {
        let color = input.color.as_deref().map(sanitize_hex_color);
        self.repo
            .update_workflow_stage(
                id,
                input.name.as_deref(),
                color.as_deref(),
                input.position,
                input.allowed_role_ids.as_deref(),
                input.require_for_publish,
            )
            .await
    }

    pub async fn delete_workflow_stage(&self, id: &str) -> ApiResult<()> {
        self.repo.delete_workflow_stage(id).await
    }

    pub async fn get_workflow_entry(
        &self,
        document_id: &str,
        locale: Option<&str>,
    ) -> ApiResult<WorkflowEntryDto> {
        let loc = match locale.map(str::trim).filter(|s| !s.is_empty()) {
            Some(l) => l.to_string(),
            None => self.default_locale().await?,
        };
        if let Some(entry) = self.repo.get_workflow_entry(document_id, &loc).await? {
            return Ok(entry);
        }

        // Default to the first configured workflow stage ("To do") when not yet explicitly transitioned
        let stages = self.repo.list_workflow_stages().await?;
        let first = stages.first().ok_or_else(|| {
            ApiError::Internal("No workflow stages configured".into())
        })?;
        Ok(WorkflowEntryDto {
            document_id: document_id.to_string(),
            locale: loc,
            stage_id: first.id.clone(),
            stage_name: first.name.clone(),
            stage_color: first.color.clone(),
            assignee_id: None,
            assignee_email: None,
            updated_at: String::new(),
        })
    }

    pub async fn update_workflow_entry(
        &self,
        actor: &Actor,
        document_id: &str,
        input: UpsertWorkflowEntryInput,
    ) -> ApiResult<WorkflowEntryDto> {
        let loc = match input.locale.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(l) => l.to_string(),
            None => self.default_locale().await?,
        };

        let target_stage = self
            .repo
            .get_workflow_stage(&input.stage_id)
            .await?
            .ok_or_else(|| {
                ApiError::NotFound(format!("Workflow stage '{}' not found", input.stage_id))
            })?;

        // Enforce role-level stage transition gate when `allowed_role_ids` is non-empty!
        if !target_stage.allowed_role_ids.is_empty() {
            let allowed = match actor {
                Actor::AdminUser {
                    is_super_admin: true,
                    ..
                } => true,
                Actor::AdminUser { role_ids, .. } => role_ids
                    .iter()
                    .any(|rid| target_stage.allowed_role_ids.contains(rid)),
                Actor::AdminToken { token_type, .. } => {
                    *token_type == cms_core::TokenType::FullAccess
                }
                _ => false,
            };
            if !allowed {
                return Err(ApiError::Forbidden(format!(
                    "Your role is not permitted to move entries into workflow stage '{}'",
                    target_stage.name
                )));
            }
        }

        let clean_assignee = input
            .assignee_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        self.repo
            .upsert_workflow_entry(document_id, &loc, &target_stage.id, clean_assignee)
            .await
    }

    /// Blocks direct creation as `status=published` whenever a review workflow publish gate is active.
    pub async fn enforce_new_entry_publish_gate(&self) -> ApiResult<()> {
        let stages = self.repo.list_workflow_stages().await?;
        let required_stages: Vec<&str> = stages
            .iter()
            .filter(|s| s.require_for_publish)
            .map(|s| s.name.as_str())
            .collect();
        if required_stages.is_empty() {
            return Ok(());
        }
        Err(ApiError::Validation {
            message: format!(
                "Review workflow gate active: create as draft first and advance to [{}] before publishing",
                required_stages.join(" / ")
            ),
            details: serde_json::json!({ "requiredStages": required_stages }),
        })
    }

    /// Checks whether any workflow stage has `require_for_publish == true`. If so, verifies that
    /// `(document_id, locale)` is currently in one of the publish-approved stages!
    pub async fn enforce_publish_workflow_gate(
        &self,
        document_id: &str,
        locale: &str,
    ) -> ApiResult<()> {
        let stages = self.repo.list_workflow_stages().await?;
        let required_stages: Vec<&WorkflowStageDto> =
            stages.iter().filter(|s| s.require_for_publish).collect();
        if required_stages.is_empty() {
            return Ok(());
        }

        let current = self.get_workflow_entry(document_id, Some(locale)).await?;
        if required_stages.iter().any(|s| s.id == current.stage_id) {
            return Ok(());
        }

        let names: Vec<&str> = required_stages.iter().map(|s| s.name.as_str()).collect();
        Err(ApiError::Validation {
            message: format!(
                "Review workflow gate: entry is in stage '{}' and must reach [{}] before publishing",
                current.stage_name,
                names.join(" / ")
            ),
            details: serde_json::json!({
                "currentStage": current.stage_name,
                "requiredStages": names,
            }),
        })
    }

    // =========================================================================
    // Audit Logs
    // =========================================================================

    pub async fn record_audit(&self, input: InsertAuditLogInput) -> ApiResult<()> {
        self.repo.insert_audit_log(input).await
    }

    pub async fn list_audit_logs(
        &self,
        filter: &AuditLogFilter,
    ) -> ApiResult<(Vec<AuditLogDto>, PaginationMeta)> {
        self.repo.list_audit_logs(filter).await
    }
}

fn sanitize_hex_color(raw: &str) -> String {
    let t = raw.trim();
    if t.len() == 7 && t.starts_with('#') && t[1..].chars().all(|c| c.is_ascii_hexdigit()) {
        t.to_ascii_lowercase()
    } else {
        "#4f46e5".to_string()
    }
}

fn normalize_optional_iso(raw: Option<&str>) -> ApiResult<Option<String>> {
    let Some(s) = raw.map(str::trim).filter(|x| !x.is_empty()) else {
        return Ok(None);
    };
    let parsed = chrono::DateTime::parse_from_rfc3339(s).map_err(|_| ApiError::Validation {
        message: "scheduledAt must be a valid ISO-8601 / RFC-3339 timestamp".into(),
        details: serde_json::json!({ "field": "scheduledAt" }),
    })?;
    Ok(Some(
        parsed
            .with_timezone(&chrono::Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    ))
}

fn urlencoding_segment(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
