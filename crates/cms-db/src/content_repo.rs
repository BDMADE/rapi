use crate::pool::DbPools;
use crate::schema_engine::SchemaEngine;
use chrono::Utc;
use cms_core::{
    fts_table_name_for_uid, table_name_for_uid, ApiError, ApiResult, ContentKind,
    ContentTypeSchema, ContentVersionDto, DocumentId, DocumentQuery, DocumentStatus,
    FieldDefinition, FieldType, PaginationMeta, PopulateSpec, RelationKind, RelationMutation,
    RelationPosition, SortSpec,
};
use sqlx::{Column, Row, Sqlite, Transaction};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

const MAX_POPULATE_DEPTH: usize = 3;

#[derive(Clone)]
pub struct DynamicRepository {
    pools: DbPools,
    schema_engine: SchemaEngine,
}

impl DynamicRepository {
    pub fn new(pools: DbPools, schema_engine: SchemaEngine) -> Self {
        Self {
            pools,
            schema_engine,
        }
    }

    /// Queries documents with Strapi v5 filters, FTS5 search, sorting, field projection, pagination, and batch populate.
    pub async fn find_many(
        &self,
        schema: &ContentTypeSchema,
        query: &DocumentQuery,
        strip_private: bool,
    ) -> ApiResult<(Vec<serde_json::Value>, PaginationMeta)> {
        let table_name = table_name_for_uid(&schema.uid);
        let fts_name = fts_table_name_for_uid(&schema.uid);
        let status = query.status.unwrap_or(DocumentStatus::Published);
        let locale = query.locale.as_deref().unwrap_or("en");

        let mut where_clauses = vec![
            "\"status\" = ?".to_string(),
            "\"locale\" = ?".to_string(),
        ];
        let mut binds: Vec<SqlBind> = vec![
            SqlBind::Text(status.as_str().to_string()),
            SqlBind::Text(locale.to_string()),
        ];

        // Full-text search via FTS5 virtual table
        if let Some(q_raw) = query.search_query.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(fts_expr) = build_fts_match_query(q_raw) {
                where_clauses.push(format!(
                    "\"document_id\" IN (SELECT document_id FROM \"{fts_name}\" WHERE status = ? AND locale = ? AND \"{fts_name}\" MATCH ?)"
                ));
                binds.push(SqlBind::Text(status.as_str().to_string()));
                binds.push(SqlBind::Text(locale.to_string()));
                binds.push(SqlBind::Text(fts_expr));
            }
        }

        // Dynamic Strapi v5 `filters` AST
        if let Some(filter_val) = &query.filters {
            let mut filter_sql = String::new();
            compile_filter_node(schema, filter_val, strip_private, &mut filter_sql, &mut binds)?;
            if !filter_sql.is_empty() {
                where_clauses.push(format!("({filter_sql})"));
            }
        }

        let where_sql = where_clauses.join(" AND ");

        // Count total matching rows
        let count_sql = format!("SELECT COUNT(*) FROM \"{table_name}\" WHERE {where_sql}");
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
        for b in &binds {
            count_q = b.bind_scalar(count_q);
        }
        let total = count_q
            .fetch_one(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("count documents failed: {e}")))?
            as u64;

        let (page, page_size, offset) = if query.start.is_some() || query.limit.is_some() {
            let lim = query.limit.unwrap_or(query.page_size).clamp(1, 200);
            let st = query.start.unwrap_or(0);
            let computed_page = (st / lim) + 1;
            (computed_page, lim, st)
        } else {
            let p = query.page.max(1);
            let ps = query.page_size.clamp(1, 200);
            (p, ps, (p - 1) * ps)
        };

        let order_sql = build_order_by(schema, &query.sort, strip_private)?;
        let select_sql = format!(
            "SELECT * FROM \"{table_name}\" WHERE {where_sql} ORDER BY {order_sql} LIMIT ? OFFSET ?"
        );
        let mut rows_q = sqlx::query(&select_sql);
        for b in &binds {
            rows_q = b.bind_query(rows_q);
        }
        rows_q = rows_q.bind(page_size as i64).bind(offset as i64);

        let rows = rows_q
            .fetch_all(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("select documents failed: {e}")))?;

        let mut docs: Vec<serde_json::Map<String, serde_json::Value>> = rows
            .into_iter()
            .map(|r| decode_document_row(schema, &r, &query.fields, strip_private))
            .collect();

        if !docs.is_empty() && query.populate != PopulateSpec::None {
            self.populate_documents(schema, &mut docs, &query.populate, status, locale, strip_private, 0)
                .await?;
        }

        // In Admin Content Manager (strip_private == false), attach `publishedStatus` summary
        if !strip_private && !docs.is_empty() {
            self.attach_publication_badges(&schema.uid, locale, &mut docs)
                .await?;
        }

        let values = docs.into_iter().map(serde_json::Value::Object).collect();
        let meta = PaginationMeta::compute(page, page_size, total);
        Ok((values, meta))
    }

    /// Lists distinct logical entries for Admin Content Manager (preferring Draft row when Draft & Publish is enabled,
    /// while annotating whether a live Published row also exists).
    pub async fn find_admin_entries(
        &self,
        schema: &ContentTypeSchema,
        query: &DocumentQuery,
    ) -> ApiResult<(Vec<serde_json::Value>, PaginationMeta)> {
        if query.status.is_some() || !schema.draft_and_publish {
            let mut adjusted = query.clone();
            if !schema.draft_and_publish {
                adjusted.status = Some(DocumentStatus::Published);
            }
            return self.find_many(schema, &adjusted, false).await;
        }

        // Default Admin list on draftAndPublish types: read `draft` rows (every entry has a draft row!)
        let mut draft_query = query.clone();
        draft_query.status = Some(DocumentStatus::Draft);
        self.find_many(schema, &draft_query, false).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn find_one(
        &self,
        schema: &ContentTypeSchema,
        document_id: &str,
        locale: &str,
        status: DocumentStatus,
        populate: &PopulateSpec,
        fields: &[String],
        strip_private: bool,
    ) -> ApiResult<Option<serde_json::Value>> {
        let table_name = table_name_for_uid(&schema.uid);
        let effective_status = if schema.draft_and_publish {
            status
        } else {
            DocumentStatus::Published
        };

        let sql = format!(
            "SELECT * FROM \"{table_name}\" WHERE \"document_id\" = ? AND \"locale\" = ? AND \"status\" = ? LIMIT 1"
        );
        let row = sqlx::query(&sql)
            .bind(document_id)
            .bind(locale)
            .bind(effective_status.as_str())
            .fetch_optional(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("find_one failed: {e}")))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let mut docs = vec![decode_document_row(schema, &row, fields, strip_private)];
        if *populate != PopulateSpec::None {
            self.populate_documents(
                schema,
                &mut docs,
                populate,
                effective_status,
                locale,
                strip_private,
                0,
            )
            .await?;
        }
        if !strip_private {
            self.attach_publication_badges(&schema.uid, locale, &mut docs)
                .await?;
        }

        Ok(docs.into_iter().next().map(serde_json::Value::Object))
    }

    pub async fn find_first_single(
        &self,
        schema: &ContentTypeSchema,
        locale: &str,
        status: DocumentStatus,
        populate: &PopulateSpec,
        fields: &[String],
        strip_private: bool,
    ) -> ApiResult<Option<serde_json::Value>> {
        let table_name = table_name_for_uid(&schema.uid);
        let effective_status = if schema.draft_and_publish {
            status
        } else {
            DocumentStatus::Published
        };
        let doc_id: Option<String> = sqlx::query_scalar(&format!(
            "SELECT \"document_id\" FROM \"{table_name}\" WHERE \"locale\" = ? AND \"status\" = ? ORDER BY id ASC LIMIT 1"
        ))
        .bind(locale)
        .bind(effective_status.as_str())
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("find_first_single failed: {e}")))?;

        match doc_id {
            Some(id) => {
                self.find_one(
                    schema,
                    &id,
                    locale,
                    effective_status,
                    populate,
                    fields,
                    strip_private,
                )
                .await
            }
            None => Ok(None),
        }
    }

    /// Creates a new logical document (`documentId`) — always maintaining a `draft` row on `draft_and_publish` types,
    /// and optionally publishing immediately when `target_status == DocumentStatus::Published`.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_document(
        &self,
        schema: &ContentTypeSchema,
        locale: &str,
        target_status: DocumentStatus,
        scalar_data: &BTreeMap<String, serde_json::Value>,
        relations: &BTreeMap<String, RelationMutation>,
        media_links: &BTreeMap<String, Vec<i64>>,
        actor_id: Option<&str>,
        strip_private: bool,
    ) -> ApiResult<serde_json::Value> {
        let doc_id = if schema.kind == ContentKind::SingleType {
            // SingleType reuses existing document_id across locales/statuses if any row already exists
            let existing_id: Option<String> = sqlx::query_scalar(
                "SELECT document_id FROM documents WHERE content_type_uid = ? LIMIT 1",
            )
            .bind(&schema.uid)
            .fetch_optional(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("check singleType doc_id failed: {e}")))?;
            existing_id.unwrap_or_else(|| DocumentId::generate().to_string())
        } else {
            DocumentId::generate().to_string()
        };

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin create_document tx failed: {e}")))?;

        let primary_status = if schema.draft_and_publish {
            DocumentStatus::Draft
        } else {
            DocumentStatus::Published
        };

        upsert_row_in_tx(
            &mut tx,
            schema,
            &doc_id,
            locale,
            primary_status,
            scalar_data,
            actor_id,
        )
        .await?;

        apply_relation_mutations_in_tx(&mut tx, schema, &doc_id, locale, primary_status, relations)
            .await?;
        apply_media_links_in_tx(&mut tx, &doc_id, locale, primary_status, media_links).await?;
        sync_fts_in_tx(&mut tx, schema, &doc_id, locale, primary_status, scalar_data).await?;
        record_version_snapshot_in_tx(
            &mut tx,
            schema,
            &doc_id,
            locale,
            primary_status,
            scalar_data,
            actor_id,
        )
        .await?;

        let final_status = if schema.draft_and_publish && target_status == DocumentStatus::Published {
            promote_draft_to_published_in_tx(&mut tx, schema, &doc_id, locale, actor_id).await?;
            DocumentStatus::Published
        } else {
            primary_status
        };

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit create_document failed: {e}")))?;

        self.find_one(
            schema,
            &doc_id,
            locale,
            final_status,
            &PopulateSpec::All,
            &[],
            strip_private,
        )
        .await?
        .ok_or_else(|| ApiError::Internal("Created document could not be loaded".into()))
    }

    /// Updates an existing document (merging provided scalar fields & relation/media mutations),
    /// and promotes to Published if `target_status == DocumentStatus::Published`.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_document(
        &self,
        schema: &ContentTypeSchema,
        document_id: &str,
        locale: &str,
        target_status: DocumentStatus,
        scalar_patch: &BTreeMap<String, serde_json::Value>,
        relations: &BTreeMap<String, RelationMutation>,
        media_links: &BTreeMap<String, Vec<i64>>,
        actor_id: Option<&str>,
        strip_private: bool,
    ) -> ApiResult<serde_json::Value> {
        let edit_status = if schema.draft_and_publish {
            DocumentStatus::Draft
        } else {
            DocumentStatus::Published
        };

        let existing = self
            .find_one(
                schema,
                document_id,
                locale,
                edit_status,
                &PopulateSpec::None,
                &[],
                false,
            )
            .await?;

        // Merge scalar_patch onto existing scalar values (or start fresh when creating a new locale variant!)
        let mut merged_scalars = BTreeMap::new();
        if let Some(serde_json::Value::Object(existing_map)) = existing {
            for f in schema.fields.iter().filter(|f| f.field_type.has_physical_column()) {
                if let Some(val) = existing_map.get(&f.name) {
                    merged_scalars.insert(f.name.clone(), val.clone());
                }
            }
        } else {
            // Verify the document_id exists in at least one locale/status (unless SingleType)
            // and pre-populate non-localized (`localized == false`) fields from any existing sibling locale row!
            if schema.kind != ContentKind::SingleType {
                let any_exists: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM documents WHERE content_type_uid = ? AND document_id = ?",
                )
                .bind(&schema.uid)
                .bind(document_id)
                .fetch_one(&self.pools.reader)
                .await
                .map_err(|e| ApiError::Internal(format!("check document existence failed: {e}")))?;
                if any_exists == 0 {
                    return Err(ApiError::NotFound(format!(
                        "Document '{document_id}' not found"
                    )));
                }
            }
            if schema.i18n_enabled {
                let sibling_locale: Option<String> = sqlx::query_scalar(
                    "SELECT locale FROM documents WHERE content_type_uid = ? AND document_id = ? ORDER BY CASE status WHEN 'draft' THEN 0 ELSE 1 END LIMIT 1",
                )
                .bind(&schema.uid)
                .bind(document_id)
                .fetch_optional(&self.pools.reader)
                .await
                .map_err(|e| ApiError::Internal(format!("find sibling locale failed: {e}")))?;

                if let Some(sib_loc) = sibling_locale {
                    if let Some(serde_json::Value::Object(sib_map)) = self
                        .find_one(
                            schema,
                            document_id,
                            &sib_loc,
                            edit_status,
                            &PopulateSpec::None,
                            &[],
                            false,
                        )
                        .await?
                    {
                        for f in schema
                            .fields
                            .iter()
                            .filter(|f| f.field_type.has_physical_column() && !f.localized)
                        {
                            if let Some(val) = sib_map.get(&f.name) {
                                merged_scalars.insert(f.name.clone(), val.clone());
                            }
                        }
                    }
                }
            }
        }

        for (k, v) in scalar_patch {
            merged_scalars.insert(k.clone(), v.clone());
        }

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin update_document tx failed: {e}")))?;

        upsert_row_in_tx(
            &mut tx,
            schema,
            document_id,
            locale,
            edit_status,
            &merged_scalars,
            actor_id,
        )
        .await?;

        // Propagate modified non-localized (`localized == false`) fields across sibling locale rows of the same documentId
        if schema.i18n_enabled {
            sync_non_localized_scalars_in_tx(
                &mut tx,
                schema,
                document_id,
                locale,
                edit_status,
                scalar_patch,
            )
            .await?;
        }

        apply_relation_mutations_in_tx(&mut tx, schema, document_id, locale, edit_status, relations)
            .await?;
        apply_media_links_in_tx(&mut tx, document_id, locale, edit_status, media_links).await?;
        sync_fts_in_tx(&mut tx, schema, document_id, locale, edit_status, &merged_scalars).await?;
        record_version_snapshot_in_tx(
            &mut tx,
            schema,
            document_id,
            locale,
            edit_status,
            &merged_scalars,
            actor_id,
        )
        .await?;

        let final_status = if schema.draft_and_publish && target_status == DocumentStatus::Published {
            promote_draft_to_published_in_tx(&mut tx, schema, document_id, locale, actor_id)
                .await?;
            DocumentStatus::Published
        } else {
            edit_status
        };

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit update_document failed: {e}")))?;

        self.find_one(
            schema,
            document_id,
            locale,
            final_status,
            &PopulateSpec::All,
            &[],
            strip_private,
        )
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Document '{document_id}' not found")))
    }

    pub async fn publish_document(
        &self,
        schema: &ContentTypeSchema,
        document_id: &str,
        locale: &str,
        actor_id: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        if !schema.draft_and_publish {
            return self
                .find_one(
                    schema,
                    document_id,
                    locale,
                    DocumentStatus::Published,
                    &PopulateSpec::All,
                    &[],
                    false,
                )
                .await?
                .ok_or_else(|| ApiError::NotFound(format!("Document '{document_id}' not found")));
        }

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin publish tx failed: {e}")))?;

        promote_draft_to_published_in_tx(&mut tx, schema, document_id, locale, actor_id).await?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit publish failed: {e}")))?;

        self.find_one(
            schema,
            document_id,
            locale,
            DocumentStatus::Published,
            &PopulateSpec::All,
            &[],
            false,
        )
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Document '{document_id}' not found")))
    }

    pub async fn unpublish_document(
        &self,
        schema: &ContentTypeSchema,
        document_id: &str,
        locale: &str,
    ) -> ApiResult<serde_json::Value> {
        if !schema.draft_and_publish {
            return Err(ApiError::BadRequest(
                "Draft & Publish is disabled on this content type".into(),
            ));
        }
        let table_name = table_name_for_uid(&schema.uid);
        let fts_name = fts_table_name_for_uid(&schema.uid);

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin unpublish tx failed: {e}")))?;

        sqlx::query(&format!(
            "DELETE FROM \"{table_name}\" WHERE document_id = ? AND locale = ? AND status = 'published'"
        ))
        .bind(document_id)
        .bind(locale)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("delete published ct row failed: {e}")))?;

        sqlx::query(
            "DELETE FROM documents WHERE content_type_uid = ? AND document_id = ? AND locale = ? AND status = 'published'",
        )
        .bind(&schema.uid)
        .bind(document_id)
        .bind(locale)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("delete published index row failed: {e}")))?;

        sqlx::query(
            "DELETE FROM content_relations WHERE from_document_id = ? AND from_locale = ? AND from_status = 'published'",
        )
        .bind(document_id)
        .bind(locale)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("delete published relations failed: {e}")))?;

        sqlx::query(
            "DELETE FROM upload_file_relations WHERE from_document_id = ? AND from_locale = ? AND from_status = 'published'",
        )
        .bind(document_id)
        .bind(locale)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("delete published media relations failed: {e}")))?;

        sqlx::query(&format!(
            "DELETE FROM \"{fts_name}\" WHERE document_id = ? AND locale = ? AND status = 'published'"
        ))
        .bind(document_id)
        .bind(locale)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("delete published fts row failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit unpublish failed: {e}")))?;

        self.find_one(
            schema,
            document_id,
            locale,
            DocumentStatus::Draft,
            &PopulateSpec::All,
            &[],
            false,
        )
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Document '{document_id}' not found")))
    }

    pub async fn delete_document(
        &self,
        schema: &ContentTypeSchema,
        document_id: &str,
        locale: Option<&str>,
    ) -> ApiResult<()> {
        let table_name = table_name_for_uid(&schema.uid);
        let fts_name = fts_table_name_for_uid(&schema.uid);

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin delete_document tx failed: {e}")))?;

        let deleted_rows = if let Some(loc) = locale {
            let res = sqlx::query(&format!(
                "DELETE FROM \"{table_name}\" WHERE document_id = ? AND locale = ?"
            ))
            .bind(document_id)
            .bind(loc)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete ct locale rows failed: {e}")))?;

            sqlx::query(
                "DELETE FROM documents WHERE content_type_uid = ? AND document_id = ? AND locale = ?",
            )
            .bind(&schema.uid)
            .bind(document_id)
            .bind(loc)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete documents index failed: {e}")))?;

            sqlx::query(
                "DELETE FROM content_relations WHERE from_document_id = ? AND from_locale = ?",
            )
            .bind(document_id)
            .bind(loc)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete from relations failed: {e}")))?;

            sqlx::query(
                "DELETE FROM upload_file_relations WHERE from_document_id = ? AND from_locale = ?",
            )
            .bind(document_id)
            .bind(loc)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete media relations failed: {e}")))?;

            sqlx::query(&format!(
                "DELETE FROM \"{fts_name}\" WHERE document_id = ? AND locale = ?"
            ))
            .bind(document_id)
            .bind(loc)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete fts rows failed: {e}")))?;

            sqlx::query("DELETE FROM workflow_entries WHERE document_id = ? AND locale = ?")
                .bind(document_id)
                .bind(loc)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("delete workflow_entries failed: {e}")))?;

            sqlx::query("DELETE FROM release_entries WHERE document_id = ? AND locale = ?")
                .bind(document_id)
                .bind(loc)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("delete release_entries failed: {e}")))?;

            res.rows_affected()
        } else {
            let res = sqlx::query(&format!(
                "DELETE FROM \"{table_name}\" WHERE document_id = ?"
            ))
            .bind(document_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete ct rows failed: {e}")))?;

            sqlx::query("DELETE FROM documents WHERE content_type_uid = ? AND document_id = ?")
                .bind(&schema.uid)
                .bind(document_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("delete documents index failed: {e}")))?;

            sqlx::query(
                "DELETE FROM content_relations WHERE from_document_id = ? OR to_document_id = ?",
            )
            .bind(document_id)
            .bind(document_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete relations failed: {e}")))?;

            sqlx::query("DELETE FROM upload_file_relations WHERE from_document_id = ?")
                .bind(document_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("delete media relations failed: {e}")))?;

            sqlx::query("DELETE FROM content_versions WHERE document_id = ?")
                .bind(document_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("delete versions failed: {e}")))?;

            sqlx::query(&format!(
                "DELETE FROM \"{fts_name}\" WHERE document_id = ?"
            ))
            .bind(document_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete fts rows failed: {e}")))?;

            sqlx::query("DELETE FROM workflow_entries WHERE document_id = ?")
                .bind(document_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("delete workflow_entries failed: {e}")))?;

            sqlx::query("DELETE FROM release_entries WHERE document_id = ?")
                .bind(document_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("delete release_entries failed: {e}")))?;

            res.rows_affected()
        };

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit delete_document failed: {e}")))?;

        if deleted_rows == 0 {
            return Err(ApiError::NotFound(format!(
                "Document '{document_id}' not found"
            )));
        }
        Ok(())
    }

    pub async fn list_versions(
        &self,
        uid: &str,
        document_id: &str,
        locale: &str,
    ) -> ApiResult<Vec<ContentVersionDto>> {
        let rows = sqlx::query(
            r#"
            SELECT id, document_id, content_type_uid, version, locale, status, data_json, created_by, created_at
            FROM content_versions
            WHERE content_type_uid = ? AND document_id = ? AND locale = ?
            ORDER BY created_at DESC, version DESC
            LIMIT 50
            "#,
        )
        .bind(uid)
        .bind(document_id)
        .bind(locale)
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_versions failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let data_str: String = r.get("data_json");
                let data =
                    serde_json::from_str(&data_str).unwrap_or_else(|_| serde_json::json!({}));
                ContentVersionDto {
                    id: r.get("id"),
                    document_id: r.get("document_id"),
                    content_type_uid: r.get("content_type_uid"),
                    version: r.get("version"),
                    locale: r.get("locale"),
                    status: r.get("status"),
                    data,
                    created_by: r.get("created_by"),
                    created_at: r.get("created_at"),
                }
            })
            .collect())
    }

    pub async fn restore_version(
        &self,
        schema: &ContentTypeSchema,
        document_id: &str,
        version_id: &str,
        actor_id: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let row = sqlx::query(
            "SELECT locale, data_json FROM content_versions WHERE id = ? AND document_id = ? AND content_type_uid = ? LIMIT 1",
        )
        .bind(version_id)
        .bind(document_id)
        .bind(&schema.uid)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("find version failed: {e}")))?;

        let Some(row) = row else {
            return Err(ApiError::NotFound(format!(
                "Version '{version_id}' not found"
            )));
        };

        let locale: String = row.get("locale");
        let data_str: String = row.get("data_json");
        let parsed: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&data_str)
            .map_err(|e| ApiError::Internal(format!("decode version json failed: {e}")))?;

        let mut scalars = BTreeMap::new();
        for f in schema.fields.iter().filter(|f| f.field_type.has_physical_column()) {
            if let Some(val) = parsed.get(&f.name) {
                scalars.insert(f.name.clone(), val.clone());
            }
        }

        let target_status = if schema.draft_and_publish {
            DocumentStatus::Draft
        } else {
            DocumentStatus::Published
        };

        self.update_document(
            schema,
            document_id,
            &locale,
            target_status,
            &scalars,
            &BTreeMap::new(),
            &BTreeMap::new(),
            actor_id,
            false,
        )
        .await
    }

    // --- Batch Relation & Media Population ---

    #[allow(clippy::too_many_arguments)]
    fn populate_documents<'a>(
        &'a self,
        schema: &'a ContentTypeSchema,
        docs: &'a mut [serde_json::Map<String, serde_json::Value>],
        populate: &'a PopulateSpec,
        status: DocumentStatus,
        locale: &'a str,
        strip_private: bool,
        depth: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ApiResult<()>> + Send + 'a>> {
        Box::pin(async move {
            if depth >= MAX_POPULATE_DEPTH || docs.is_empty() {
                return Ok(());
            }

            let doc_ids: Vec<String> = docs
                .iter()
                .filter_map(|d| d.get("documentId").and_then(|v| v.as_str()).map(String::from))
                .collect();
            if doc_ids.is_empty() {
                return Ok(());
            }

            for field in &schema.fields {
                let field_opts = match populate {
                    PopulateSpec::None => continue,
                    PopulateSpec::All => Some((vec![], false)),
                    PopulateSpec::Fields(map) => map
                        .get(&field.name)
                        .map(|o| (o.fields.clone(), o.populate_nested)),
                };
                let Some((sub_fields, nest)) = field_opts else {
                    continue;
                };

                if field.field_type == FieldType::Relation {
                    let Some(rel_cfg) = &field.relation else {
                        continue;
                    };
                    let Some(target_uid) = &rel_cfg.target_uid else {
                        continue;
                    };
                    let Some(target_schema) = self.schema_engine.get_schema(target_uid).await? else {
                        continue;
                    };

                    let placeholders = vec!["?"; doc_ids.len()].join(", ");
                    let rel_sql = format!(
                        r#"
                        SELECT from_document_id, to_document_id, position
                        FROM content_relations
                        WHERE from_content_type_uid = ?
                          AND from_status = ?
                          AND from_locale = ?
                          AND field_name = ?
                          AND from_document_id IN ({placeholders})
                        ORDER BY position ASC, id ASC
                        "#
                    );
                    let mut q = sqlx::query(&rel_sql)
                        .bind(&schema.uid)
                        .bind(status.as_str())
                        .bind(locale)
                        .bind(&field.name);
                    for id in &doc_ids {
                        q = q.bind(id);
                    }
                    let link_rows = q
                        .fetch_all(&self.pools.reader)
                        .await
                        .map_err(|e| ApiError::Internal(format!("populate relations query failed: {e}")))?;

                    let mut by_source: HashMap<String, Vec<String>> = HashMap::new();
                    let mut target_ids_set: HashSet<String> = HashSet::new();
                    for r in link_rows {
                        let from_id: String = r.get("from_document_id");
                        let to_id: String = r.get("to_document_id");
                        target_ids_set.insert(to_id.clone());
                        by_source.entry(from_id).or_default().push(to_id);
                    }

                    let mut target_map: HashMap<String, serde_json::Value> = HashMap::new();
                    if !target_ids_set.is_empty() {
                        let target_table = table_name_for_uid(&target_schema.uid);
                        let target_status = if target_schema.draft_and_publish {
                            status
                        } else {
                            DocumentStatus::Published
                        };
                        let t_placeholders = vec!["?"; target_ids_set.len()].join(", ");
                        let t_sql = format!(
                            "SELECT * FROM \"{target_table}\" WHERE status = ? AND locale = ? AND document_id IN ({t_placeholders})"
                        );
                        let mut tq = sqlx::query(&t_sql)
                            .bind(target_status.as_str())
                            .bind(locale);
                        for tid in &target_ids_set {
                            tq = tq.bind(tid);
                        }
                        let t_rows = tq
                            .fetch_all(&self.pools.reader)
                            .await
                            .map_err(|e| ApiError::Internal(format!("batch fetch target docs failed: {e}")))?;

                        let mut decoded_targets: Vec<serde_json::Map<String, serde_json::Value>> = t_rows
                            .into_iter()
                            .map(|r| decode_document_row(&target_schema, &r, &sub_fields, strip_private))
                            .collect();

                        if nest {
                            self.populate_documents(
                                &target_schema,
                                &mut decoded_targets,
                                &PopulateSpec::All,
                                target_status,
                                locale,
                                strip_private,
                                depth + 1,
                            )
                            .await?;
                        }

                        for d in decoded_targets {
                            if let Some(did) = d.get("documentId").and_then(|v| v.as_str()) {
                                target_map.insert(did.to_string(), serde_json::Value::Object(d));
                            }
                        }
                    }

                    let is_single_rel = matches!(
                        rel_cfg.relation_type,
                        RelationKind::OneToOne | RelationKind::ManyToOne | RelationKind::MorphOne
                    );

                    for doc in docs.iter_mut() {
                        let Some(src_id) = doc.get("documentId").and_then(|v| v.as_str()) else {
                            continue;
                        };
                        let linked_ids = by_source.get(src_id).cloned().unwrap_or_default();
                        if is_single_rel {
                            let val = linked_ids
                                .first()
                                .and_then(|tid| target_map.get(tid))
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);
                            doc.insert(field.name.clone(), val);
                        } else {
                            let arr: Vec<serde_json::Value> = linked_ids
                                .iter()
                                .filter_map(|tid| target_map.get(tid).cloned())
                                .collect();
                            doc.insert(field.name.clone(), serde_json::Value::Array(arr));
                        }
                    }
                } else if field.field_type == FieldType::Media {
                    let placeholders = vec!["?"; doc_ids.len()].join(", ");
                    let media_sql = format!(
                        r#"
                        SELECT ufr.from_document_id, uf.id, uf.document_id, uf.name, uf.alternative_text,
                               uf.caption, uf.width, uf.height, uf.mime, uf.size_kb, uf.url
                        FROM upload_file_relations ufr
                        INNER JOIN upload_files uf ON uf.id = ufr.file_id
                        WHERE ufr.from_status = ?
                          AND ufr.from_locale = ?
                          AND ufr.field_name = ?
                          AND ufr.from_document_id IN ({placeholders})
                        ORDER BY ufr.position ASC, ufr.id ASC
                        "#
                    );
                    let mut mq = sqlx::query(&media_sql)
                        .bind(status.as_str())
                        .bind(locale)
                        .bind(&field.name);
                    for id in &doc_ids {
                        mq = mq.bind(id);
                    }
                    let m_rows = mq
                        .fetch_all(&self.pools.reader)
                        .await
                        .map_err(|e| ApiError::Internal(format!("populate media query failed: {e}")))?;

                    let mut by_source: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
                    for r in m_rows {
                        let from_id: String = r.get("from_document_id");
                        let file_json = serde_json::json!({
                            "id": r.get::<i64, _>("id"),
                            "documentId": r.get::<String, _>("document_id"),
                            "name": r.get::<String, _>("name"),
                            "alternativeText": r.get::<Option<String>, _>("alternative_text"),
                            "caption": r.get::<Option<String>, _>("caption"),
                            "width": r.get::<Option<i64>, _>("width"),
                            "height": r.get::<Option<i64>, _>("height"),
                            "mime": r.get::<String, _>("mime"),
                            "size": r.get::<f64, _>("size_kb"),
                            "url": r.get::<String, _>("url"),
                        });
                        by_source.entry(from_id).or_default().push(file_json);
                    }

                    let is_multiple = field.validation.multiple.unwrap_or(false);
                    for doc in docs.iter_mut() {
                        let Some(src_id) = doc.get("documentId").and_then(|v| v.as_str()) else {
                            continue;
                        };
                        let items = by_source.remove(src_id).unwrap_or_default();
                        if is_multiple {
                            doc.insert(field.name.clone(), serde_json::Value::Array(items));
                        } else {
                            doc.insert(
                                field.name.clone(),
                                items.into_iter().next().unwrap_or(serde_json::Value::Null),
                            );
                        }
                    }
                }
            }

            Ok(())
        })
    }

    async fn attach_publication_badges(
        &self,
        uid: &str,
        locale: &str,
        docs: &mut [serde_json::Map<String, serde_json::Value>],
    ) -> ApiResult<()> {
        let doc_ids: Vec<String> = docs
            .iter()
            .filter_map(|d| d.get("documentId").and_then(|v| v.as_str()).map(String::from))
            .collect();
        if doc_ids.is_empty() {
            return Ok(());
        }
        let placeholders = vec!["?"; doc_ids.len()].join(", ");
        let sql = format!(
            "SELECT document_id, published_at, updated_at FROM documents WHERE content_type_uid = ? AND locale = ? AND status = 'published' AND document_id IN ({placeholders})"
        );
        let mut q = sqlx::query(&sql).bind(uid).bind(locale);
        for id in &doc_ids {
            q = q.bind(id);
        }
        let rows = q
            .fetch_all(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("check published badges failed: {e}")))?;

        let mut pub_map: HashMap<String, (Option<String>, String)> = HashMap::new();
        for r in rows {
            pub_map.insert(
                r.get("document_id"),
                (r.get("published_at"), r.get("updated_at")),
            );
        }

        for doc in docs.iter_mut() {
            let Some(did) = doc.get("documentId").and_then(|v| v.as_str()) else {
                continue;
            };
            if let Some((pub_at, pub_updated)) = pub_map.get(did) {
                if doc.get("publishedAt").is_none_or(|v| v.is_null()) {
                    doc.insert(
                        "publishedAt".into(),
                        pub_at
                            .clone()
                            .map(serde_json::Value::String)
                            .unwrap_or(serde_json::Value::Null),
                    );
                }
                let draft_updated = doc
                    .get("updatedAt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let state = if draft_updated > pub_updated.as_str() {
                    "modified"
                } else {
                    "published"
                };
                doc.insert(
                    "publicationState".into(),
                    serde_json::Value::String(state.into()),
                );
            } else {
                doc.insert(
                    "publicationState".into(),
                    serde_json::Value::String("draft".into()),
                );
            }
        }
        Ok(())
    }
}

// --- Internal SQLite Transaction Helpers ---

async fn sync_non_localized_scalars_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    schema: &ContentTypeSchema,
    document_id: &str,
    active_locale: &str,
    status: DocumentStatus,
    scalar_patch: &BTreeMap<String, serde_json::Value>,
) -> ApiResult<()> {
    let non_localized: Vec<&FieldDefinition> = schema
        .fields
        .iter()
        .filter(|f| {
            f.field_type.has_physical_column()
                && !f.localized
                && !f.unique
                && scalar_patch.contains_key(&f.name)
        })
        .collect();

    if non_localized.is_empty() {
        return Ok(());
    }

    let table_name = table_name_for_uid(&schema.uid);
    let mut set_parts = Vec::with_capacity(non_localized.len() + 1);
    for f in &non_localized {
        set_parts.push(format!("\"{}\" = ?", f.name));
    }
    set_parts.push("\"updated_at\" = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')".to_string());

    let sql = format!(
        "UPDATE \"{table_name}\" SET {} WHERE document_id = ? AND locale != ? AND status = ?",
        set_parts.join(", ")
    );
    let mut q = sqlx::query(&sql);
    for f in &non_localized {
        q = bind_json_for_field(q, f, scalar_patch.get(&f.name));
    }
    q = q
        .bind(document_id)
        .bind(active_locale)
        .bind(status.as_str());

    if let Err(e) = q.execute(&mut **tx).await {
        return Err(map_sqlite_write_error(e));
    }
    Ok(())
}

async fn upsert_row_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    schema: &ContentTypeSchema,
    document_id: &str,
    locale: &str,
    status: DocumentStatus,
    scalar_data: &BTreeMap<String, serde_json::Value>,
    actor_id: Option<&str>,
) -> ApiResult<i64> {
    let table_name = table_name_for_uid(&schema.uid);
    let now = Utc::now().to_rfc3339();
    let published_at = if status == DocumentStatus::Published {
        Some(now.clone())
    } else {
        None
    };

    // Check if row already exists for (document_id, locale, status)
    let existing_ver: Option<i64> = sqlx::query_scalar(&format!(
        "SELECT version FROM \"{table_name}\" WHERE document_id = ? AND locale = ? AND status = ? LIMIT 1"
    ))
    .bind(document_id)
    .bind(locale)
    .bind(status.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("check existing row version failed: {e}")))?;

    let phys_fields: Vec<&FieldDefinition> = schema
        .fields
        .iter()
        .filter(|f| f.field_type.has_physical_column())
        .collect();

    let next_version = existing_ver.map(|v| v + 1).unwrap_or(1);

    if existing_ver.is_some() {
        let mut set_parts = vec![
            "\"version\" = ?".to_string(),
            "\"updated_at\" = ?".to_string(),
            "\"updated_by\" = ?".to_string(),
        ];
        if status == DocumentStatus::Published {
            set_parts.push("\"published_at\" = COALESCE(\"published_at\", ?)".to_string());
        }
        for f in &phys_fields {
            set_parts.push(format!("\"{}\" = ?", f.name));
        }

        let sql = format!(
            "UPDATE \"{table_name}\" SET {} WHERE document_id = ? AND locale = ? AND status = ?",
            set_parts.join(", ")
        );
        let mut q = sqlx::query(&sql)
            .bind(next_version)
            .bind(&now)
            .bind(actor_id);
        if status == DocumentStatus::Published {
            q = q.bind(&now);
        }
        for f in &phys_fields {
            q = bind_json_for_field(q, f, scalar_data.get(&f.name));
        }
        q = q.bind(document_id).bind(locale).bind(status.as_str());

        if let Err(e) = q.execute(&mut **tx).await {
            return Err(map_sqlite_write_error(e));
        }
    } else {
        let mut col_names = vec![
            "\"document_id\"".to_string(),
            "\"locale\"".to_string(),
            "\"status\"".to_string(),
            "\"version\"".to_string(),
            "\"published_at\"".to_string(),
            "\"created_at\"".to_string(),
            "\"updated_at\"".to_string(),
            "\"created_by\"".to_string(),
            "\"updated_by\"".to_string(),
        ];
        for f in &phys_fields {
            col_names.push(format!("\"{}\"", f.name));
        }
        let placeholders = vec!["?"; col_names.len()].join(", ");
        let sql = format!(
            "INSERT INTO \"{table_name}\" ({}) VALUES ({placeholders})",
            col_names.join(", ")
        );
        let mut q = sqlx::query(&sql)
            .bind(document_id)
            .bind(locale)
            .bind(status.as_str())
            .bind(next_version)
            .bind(&published_at)
            .bind(&now)
            .bind(&now)
            .bind(actor_id)
            .bind(actor_id);

        for f in &phys_fields {
            q = bind_json_for_field(q, f, scalar_data.get(&f.name));
        }

        if let Err(e) = q.execute(&mut **tx).await {
            return Err(map_sqlite_write_error(e));
        }
    }

    // Keep cross-type `documents` index synchronized in the same transaction
    sqlx::query(
        r#"
        INSERT INTO documents (document_id, content_type_uid, locale, status, version, published_at, created_by, updated_by, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(document_id, locale, status) DO UPDATE SET
            version = excluded.version,
            published_at = COALESCE(documents.published_at, excluded.published_at),
            updated_by = excluded.updated_by,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(document_id)
    .bind(&schema.uid)
    .bind(locale)
    .bind(status.as_str())
    .bind(next_version)
    .bind(&published_at)
    .bind(actor_id)
    .bind(actor_id)
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("upsert documents index failed: {e}")))?;

    Ok(next_version)
}

async fn promote_draft_to_published_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    schema: &ContentTypeSchema,
    document_id: &str,
    locale: &str,
    actor_id: Option<&str>,
) -> ApiResult<()> {
    let table_name = table_name_for_uid(&schema.uid);
    let draft_row = sqlx::query(&format!(
        "SELECT * FROM \"{table_name}\" WHERE document_id = ? AND locale = ? AND status = 'draft' LIMIT 1"
    ))
    .bind(document_id)
    .bind(locale)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("fetch draft for publish failed: {e}")))?;

    let Some(draft_row) = draft_row else {
        return Err(ApiError::NotFound(format!(
            "Draft for document '{document_id}' ({locale}) not found"
        )));
    };

    let decoded = decode_document_row(schema, &draft_row, &[], false);
    let mut scalars = BTreeMap::new();
    for f in schema.fields.iter().filter(|f| f.field_type.has_physical_column()) {
        if let Some(val) = decoded.get(&f.name) {
            scalars.insert(f.name.clone(), val.clone());
        }
    }

    upsert_row_in_tx(
        tx,
        schema,
        document_id,
        locale,
        DocumentStatus::Published,
        &scalars,
        actor_id,
    )
    .await?;

    // Copy status-isolated relations from draft -> published
    sqlx::query(
        "DELETE FROM content_relations WHERE from_document_id = ? AND from_locale = ? AND from_status = 'published'",
    )
    .bind(document_id)
    .bind(locale)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("clear published relations failed: {e}")))?;

    sqlx::query(
        r#"
        INSERT INTO content_relations (
            from_content_type_uid, from_document_id, from_status, from_locale,
            field_name, component_path, to_content_type_uid, to_document_id, position
        )
        SELECT
            from_content_type_uid, from_document_id, 'published', from_locale,
            field_name, component_path, to_content_type_uid, to_document_id, position
        FROM content_relations
        WHERE from_document_id = ? AND from_locale = ? AND from_status = 'draft'
        "#,
    )
    .bind(document_id)
    .bind(locale)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("copy draft relations to published failed: {e}")))?;

    // Copy status-isolated media relations from draft -> published
    sqlx::query(
        "DELETE FROM upload_file_relations WHERE from_document_id = ? AND from_locale = ? AND from_status = 'published'",
    )
    .bind(document_id)
    .bind(locale)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("clear published media relations failed: {e}")))?;

    sqlx::query(
        r#"
        INSERT INTO upload_file_relations (
            from_document_id, from_status, from_locale, field_name, component_path, file_id, position
        )
        SELECT
            from_document_id, 'published', from_locale, field_name, component_path, file_id, position
        FROM upload_file_relations
        WHERE from_document_id = ? AND from_locale = ? AND from_status = 'draft'
        "#,
    )
    .bind(document_id)
    .bind(locale)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("copy draft media relations failed: {e}")))?;

    sync_fts_in_tx(
        tx,
        schema,
        document_id,
        locale,
        DocumentStatus::Published,
        &scalars,
    )
    .await?;

    record_version_snapshot_in_tx(
        tx,
        schema,
        document_id,
        locale,
        DocumentStatus::Published,
        &scalars,
        actor_id,
    )
    .await?;

    Ok(())
}

async fn apply_relation_mutations_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    schema: &ContentTypeSchema,
    document_id: &str,
    locale: &str,
    status: DocumentStatus,
    mutations: &BTreeMap<String, RelationMutation>,
) -> ApiResult<()> {
    for (field_name, mutation) in mutations {
        let Some(field_def) = schema
            .fields
            .iter()
            .find(|f| &f.name == field_name && f.field_type == FieldType::Relation)
        else {
            continue;
        };
        let Some(rel_cfg) = &field_def.relation else {
            continue;
        };
        let target_uid = rel_cfg.target_uid.as_deref().unwrap_or("");
        let is_single = matches!(
            rel_cfg.relation_type,
            RelationKind::OneToOne | RelationKind::ManyToOne | RelationKind::MorphOne
        );

        // Load current ordered target documentIds
        let mut current: Vec<String> = if let Some(set_list) = &mutation.set {
            set_list.clone()
        } else {
            let rows = sqlx::query(
                r#"
                SELECT to_document_id
                FROM content_relations
                WHERE from_content_type_uid = ? AND from_document_id = ? AND from_status = ? AND from_locale = ? AND field_name = ?
                ORDER BY position ASC, id ASC
                "#,
            )
            .bind(&schema.uid)
            .bind(document_id)
            .bind(status.as_str())
            .bind(locale)
            .bind(field_name)
            .fetch_all(&mut **tx)
            .await
            .map_err(|e| ApiError::Internal(format!("load existing relations failed: {e}")))?;
            rows.into_iter().map(|r| r.get("to_document_id")).collect()
        };

        if !mutation.disconnect.is_empty() {
            current.retain(|id| !mutation.disconnect.contains(id));
        }

        for item in &mutation.connect {
            current.retain(|id| id != &item.document_id);
            match &item.position {
                RelationPosition::Start => current.insert(0, item.document_id.clone()),
                RelationPosition::End => current.push(item.document_id.clone()),
                RelationPosition::Before(anchor) => {
                    if let Some(pos) = current.iter().position(|x| x == anchor) {
                        current.insert(pos, item.document_id.clone());
                    } else {
                        current.push(item.document_id.clone());
                    }
                }
                RelationPosition::After(anchor) => {
                    if let Some(pos) = current.iter().position(|x| x == anchor) {
                        current.insert(pos + 1, item.document_id.clone());
                    } else {
                        current.push(item.document_id.clone());
                    }
                }
            }
        }

        if is_single && current.len() > 1 {
            current = vec![current.pop().unwrap()];
        }

        // Verify all target documentIds exist in `documents`
        for tid in &current {
            let found: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM documents WHERE content_type_uid = ? AND document_id = ?",
            )
            .bind(target_uid)
            .bind(tid)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| ApiError::Internal(format!("verify relation target failed: {e}")))?;
            if found == 0 {
                return Err(ApiError::BadRequest(format!(
                    "Related document '{tid}' does not exist in '{target_uid}'"
                )));
            }
        }

        sqlx::query(
            "DELETE FROM content_relations WHERE from_content_type_uid = ? AND from_document_id = ? AND from_status = ? AND from_locale = ? AND field_name = ?",
        )
        .bind(&schema.uid)
        .bind(document_id)
        .bind(status.as_str())
        .bind(locale)
        .bind(field_name)
        .execute(&mut **tx)
        .await
        .map_err(|e| ApiError::Internal(format!("clear field relations failed: {e}")))?;

        for (idx, tid) in current.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO content_relations (
                    from_content_type_uid, from_document_id, from_status, from_locale,
                    field_name, to_content_type_uid, to_document_id, position
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&schema.uid)
            .bind(document_id)
            .bind(status.as_str())
            .bind(locale)
            .bind(field_name)
            .bind(target_uid)
            .bind(tid)
            .bind(idx as i64)
            .execute(&mut **tx)
            .await
            .map_err(|e| ApiError::Internal(format!("insert relation row failed: {e}")))?;
        }
    }
    Ok(())
}

async fn apply_media_links_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    document_id: &str,
    locale: &str,
    status: DocumentStatus,
    media_links: &BTreeMap<String, Vec<i64>>,
) -> ApiResult<()> {
    for (field_name, file_ids) in media_links {
        sqlx::query(
            "DELETE FROM upload_file_relations WHERE from_document_id = ? AND from_status = ? AND from_locale = ? AND field_name = ?",
        )
        .bind(document_id)
        .bind(status.as_str())
        .bind(locale)
        .bind(field_name)
        .execute(&mut **tx)
        .await
        .map_err(|e| ApiError::Internal(format!("clear media relations failed: {e}")))?;

        for (idx, fid) in file_ids.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO upload_file_relations (from_document_id, from_status, from_locale, field_name, file_id, position)
                VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(document_id)
            .bind(status.as_str())
            .bind(locale)
            .bind(field_name)
            .bind(fid)
            .bind(idx as i64)
            .execute(&mut **tx)
            .await
            .map_err(|e| ApiError::BadRequest(format!("Invalid media file ID {fid}: {e}")))?;
        }
    }
    Ok(())
}

async fn sync_fts_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    schema: &ContentTypeSchema,
    document_id: &str,
    locale: &str,
    status: DocumentStatus,
    scalar_data: &BTreeMap<String, serde_json::Value>,
) -> ApiResult<()> {
    let fts_name = fts_table_name_for_uid(&schema.uid);
    sqlx::query(&format!(
        "DELETE FROM \"{fts_name}\" WHERE document_id = ? AND locale = ? AND status = ?"
    ))
    .bind(document_id)
    .bind(locale)
    .bind(status.as_str())
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("delete fts entry failed: {e}")))?;

    let mut chunks = Vec::new();
    for f in &schema.fields {
        if f.field_type.is_searchable_text() {
            if let Some(s) = scalar_data.get(&f.name).and_then(|v| v.as_str()) {
                if !s.trim().is_empty() {
                    chunks.push(s.to_string());
                }
            }
        } else if f.field_type == FieldType::Blocks {
            if let Some(val) = scalar_data.get(&f.name) {
                let mut extracted = String::new();
                extract_blocks_plain_text(val, &mut extracted);
                if !extracted.trim().is_empty() {
                    chunks.push(extracted);
                }
            }
        }
    }

    if !chunks.is_empty() {
        sqlx::query(&format!(
            "INSERT INTO \"{fts_name}\" (document_id, locale, status, content_text) VALUES (?, ?, ?, ?)"
        ))
        .bind(document_id)
        .bind(locale)
        .bind(status.as_str())
        .bind(chunks.join(" "))
        .execute(&mut **tx)
        .await
        .map_err(|e| ApiError::Internal(format!("insert fts entry failed: {e}")))?;
    }
    Ok(())
}

fn extract_blocks_plain_text(val: &serde_json::Value, out: &mut String) {
    match val {
        serde_json::Value::Array(arr) => {
            for item in arr {
                extract_blocks_plain_text(item, out);
            }
        }
        serde_json::Value::Object(obj) => {
            if let Some(t) = obj.get("text").and_then(|v| v.as_str()) {
                out.push_str(t);
                out.push(' ');
            }
            if let Some(children) = obj.get("children") {
                extract_blocks_plain_text(children, out);
            }
        }
        _ => {}
    }
}

async fn record_version_snapshot_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    schema: &ContentTypeSchema,
    document_id: &str,
    locale: &str,
    status: DocumentStatus,
    scalar_data: &BTreeMap<String, serde_json::Value>,
    actor_id: Option<&str>,
) -> ApiResult<()> {
    let vid = Uuid::now_v7().to_string();
    let count: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0) FROM content_versions WHERE document_id = ? AND locale = ?",
    )
    .bind(document_id)
    .bind(locale)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("count versions failed: {e}")))?;

    let mut clean_data = scalar_data.clone();
    for f in schema.fields.iter().filter(|f| f.field_type == FieldType::Password) {
        clean_data.remove(&f.name);
    }
    let data_json = serde_json::to_string(&clean_data).unwrap_or_else(|_| "{}".into());

    sqlx::query(
        r#"
        INSERT INTO content_versions (id, document_id, content_type_uid, version, locale, status, data_json, created_by)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(vid)
    .bind(document_id)
    .bind(&schema.uid)
    .bind(count + 1)
    .bind(locale)
    .bind(status.as_str())
    .bind(data_json)
    .bind(actor_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("insert content_version failed: {e}")))?;
    Ok(())
}

// --- Row Decoding & Parameterized Filter Compiler ---

fn decode_document_row(
    schema: &ContentTypeSchema,
    row: &sqlx::sqlite::SqliteRow,
    selected_fields: &[String],
    strip_private: bool,
) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    map.insert("id".into(), serde_json::json!(row.get::<i64, _>("id")));
    map.insert(
        "documentId".into(),
        serde_json::json!(row.get::<String, _>("document_id")),
    );

    let col_names: HashSet<&str> = row.columns().iter().map(|c| c.name()).collect();

    for f in schema.fields.iter().filter(|f| f.field_type.has_physical_column()) {
        if strip_private && (f.private || f.field_type == FieldType::Password) {
            continue;
        }
        if !selected_fields.is_empty() && !selected_fields.iter().any(|sf| sf == &f.name) {
            continue;
        }
        if !col_names.contains(f.name.as_str()) {
            map.insert(f.name.clone(), serde_json::Value::Null);
            continue;
        }

        let val = match f.field_type {
            FieldType::Boolean => row
                .get::<Option<i64>, _>(f.name.as_str())
                .map(|v| serde_json::Value::Bool(v != 0))
                .unwrap_or(serde_json::Value::Null),
            FieldType::Integer | FieldType::BigInteger => row
                .get::<Option<i64>, _>(f.name.as_str())
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),
            FieldType::Float | FieldType::Decimal => row
                .get::<Option<f64>, _>(f.name.as_str())
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),
            FieldType::Blocks
            | FieldType::Json
            | FieldType::Component
            | FieldType::DynamicZone
            | FieldType::Custom => row
                .get::<Option<String>, _>(f.name.as_str())
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Null),
            _ => row
                .get::<Option<String>, _>(f.name.as_str())
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        };
        map.insert(f.name.clone(), val);
    }

    map.insert(
        "locale".into(),
        serde_json::json!(row.get::<String, _>("locale")),
    );
    map.insert(
        "publishedAt".into(),
        row.get::<Option<String>, _>("published_at")
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    );
    map.insert(
        "createdAt".into(),
        serde_json::json!(row.get::<String, _>("created_at")),
    );
    map.insert(
        "updatedAt".into(),
        serde_json::json!(row.get::<String, _>("updated_at")),
    );

    map
}

fn bind_json_for_field<'q>(
    query: sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    field: &FieldDefinition,
    val: Option<&serde_json::Value>,
) -> sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    let Some(val) = val.filter(|v| !v.is_null()) else {
        return match field.field_type.sqlite_affinity() {
            "INTEGER" => query.bind(None::<i64>),
            "REAL" => query.bind(None::<f64>),
            _ => query.bind(None::<String>),
        };
    };

    match field.field_type {
        FieldType::Boolean => {
            let b = val
                .as_bool()
                .unwrap_or_else(|| val.as_i64().unwrap_or(0) != 0);
            query.bind(if b { 1_i64 } else { 0_i64 })
        }
        FieldType::Integer | FieldType::BigInteger => {
            let n = val
                .as_i64()
                .or_else(|| val.as_str().and_then(|s| s.parse::<i64>().ok()));
            query.bind(n)
        }
        FieldType::Float | FieldType::Decimal => {
            let n = val
                .as_f64()
                .or_else(|| val.as_str().and_then(|s| s.parse::<f64>().ok()));
            query.bind(n)
        }
        FieldType::Blocks
        | FieldType::Json
        | FieldType::Component
        | FieldType::DynamicZone
        | FieldType::Custom => query.bind(val.to_string()),
        _ => {
            if let Some(s) = val.as_str() {
                query.bind(s.to_string())
            } else {
                query.bind(val.to_string())
            }
        }
    }
}

fn map_sqlite_write_error(err: sqlx::Error) -> ApiError {
    let msg = err.to_string();
    if msg.contains("UNIQUE constraint failed") {
        ApiError::Conflict("Unique constraint violation on one or more fields".into())
    } else {
        ApiError::Internal(format!("Database write error: {err}"))
    }
}

#[derive(Debug, Clone)]
enum SqlBind {
    Text(String),
    Int(i64),
    Float(f64),
}

impl SqlBind {
    fn from_json_for_type(field_type: Option<FieldType>, val: &serde_json::Value) -> ApiResult<Self> {
        match field_type {
            Some(FieldType::Boolean) => {
                let b = val
                    .as_bool()
                    .or_else(|| {
                        val.as_str().and_then(|s| match s.to_ascii_lowercase().as_str() {
                            "true" | "1" => Some(true),
                            "false" | "0" => Some(false),
                            _ => None,
                        })
                    })
                    .or_else(|| val.as_i64().map(|n| n != 0))
                    .ok_or_else(|| ApiError::BadRequest("Boolean filter expects true/false".into()))?;
                return Ok(Self::Int(if b { 1 } else { 0 }));
            }
            Some(FieldType::Integer | FieldType::BigInteger) => {
                if let Some(i) = val
                    .as_i64()
                    .or_else(|| val.as_str().and_then(|s| s.parse::<i64>().ok()))
                {
                    return Ok(Self::Int(i));
                }
            }
            Some(FieldType::Float | FieldType::Decimal) => {
                if let Some(f) = val
                    .as_f64()
                    .or_else(|| val.as_str().and_then(|s| s.parse::<f64>().ok()))
                {
                    return Ok(Self::Float(f));
                }
            }
            _ => {}
        }

        match val {
            serde_json::Value::Bool(b) => Ok(Self::Int(if *b { 1 } else { 0 })),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Self::Int(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Self::Float(f))
                } else {
                    Err(ApiError::BadRequest("Invalid numeric filter value".into()))
                }
            }
            serde_json::Value::String(s) => Ok(Self::Text(s.clone())),
            _ => Err(ApiError::BadRequest(
                "Filter scalar value must be string, number, or boolean".into(),
            )),
        }
    }

    fn bind_query<'q>(
        &self,
        q: sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    ) -> sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
        match self {
            Self::Text(s) => q.bind(s.clone()),
            Self::Int(i) => q.bind(*i),
            Self::Float(f) => q.bind(*f),
        }
    }

    fn bind_scalar<'q, O>(
        &self,
        q: sqlx::query::QueryScalar<'q, Sqlite, O, sqlx::sqlite::SqliteArguments<'q>>,
    ) -> sqlx::query::QueryScalar<'q, Sqlite, O, sqlx::sqlite::SqliteArguments<'q>> {
        match self {
            Self::Text(s) => q.bind(s.clone()),
            Self::Int(i) => q.bind(*i),
            Self::Float(f) => q.bind(*f),
        }
    }
}

fn resolve_filterable_column(
    schema: &ContentTypeSchema,
    key: &str,
    strip_private: bool,
) -> ApiResult<(String, Option<FieldType>)> {
    match key {
        "id" => Ok(("id".into(), Some(FieldType::Integer))),
        "documentId" | "document_id" => Ok(("document_id".into(), Some(FieldType::String))),
        "createdAt" | "created_at" => Ok(("created_at".into(), Some(FieldType::DateTime))),
        "updatedAt" | "updated_at" => Ok(("updated_at".into(), Some(FieldType::DateTime))),
        "publishedAt" | "published_at" => Ok(("published_at".into(), Some(FieldType::DateTime))),
        "locale" => Ok(("locale".into(), Some(FieldType::String))),
        other => {
            let found = schema.fields.iter().find(|f| {
                f.name == other
                    && f.field_type.has_physical_column()
                    && f.field_type != FieldType::Password
                    && (!strip_private || !f.private)
            });
            if let Some(f) = found {
                Ok((f.name.clone(), Some(f.field_type)))
            } else {
                Err(ApiError::BadRequest(format!(
                    "Unknown or non-filterable field '{other}'"
                )))
            }
        }
    }
}

fn compile_filter_node(
    schema: &ContentTypeSchema,
    node: &serde_json::Value,
    strip_private: bool,
    out: &mut String,
    binds: &mut Vec<SqlBind>,
) -> ApiResult<()> {
    let Some(obj) = node.as_object() else {
        return Err(ApiError::BadRequest("filters must be an object".into()));
    };

    let mut clauses = Vec::new();
    for (key, val) in obj {
        if key == "$and" || key == "$or" {
            let arr = extract_filter_list(val)?;
            let mut sub_parts = Vec::new();
            for item in arr {
                let mut sub_sql = String::new();
                compile_filter_node(schema, item, strip_private, &mut sub_sql, binds)?;
                if !sub_sql.is_empty() {
                    sub_parts.push(format!("({sub_sql})"));
                }
            }
            if !sub_parts.is_empty() {
                let joiner = if key == "$and" { " AND " } else { " OR " };
                clauses.push(format!("({})", sub_parts.join(joiner)));
            }
        } else if key == "$not" {
            let mut sub_sql = String::new();
            compile_filter_node(schema, val, strip_private, &mut sub_sql, binds)?;
            if !sub_sql.is_empty() {
                clauses.push(format!("NOT ({sub_sql})"));
            }
        } else {
            let (col, ftype) = resolve_filterable_column(schema, key, strip_private)?;
            let mut field_sql = String::new();
            compile_field_ops(&col, ftype, val, &mut field_sql, binds)?;
            if !field_sql.is_empty() {
                clauses.push(field_sql);
            }
        }
    }

    out.push_str(&clauses.join(" AND "));
    Ok(())
}

fn extract_filter_list(val: &serde_json::Value) -> ApiResult<Vec<&serde_json::Value>> {
    if let Some(arr) = val.as_array() {
        return Ok(arr.iter().collect());
    }
    // serde_qs may deserialize indexed bracket arrays `[0]`, `[1]` as an Object with numeric keys!
    if let Some(obj) = val.as_object() {
        return Ok(obj.values().collect());
    }
    Err(ApiError::BadRequest(
        "Logical operator ($and/$or) expects a list of filter objects".into(),
    ))
}

fn compile_field_ops(
    col: &str,
    ftype: Option<FieldType>,
    val: &serde_json::Value,
    out: &mut String,
    binds: &mut Vec<SqlBind>,
) -> ApiResult<()> {
    // Direct equality shorthand: `filters[slug]=hello`
    if !val.is_object() {
        if val.is_null() {
            out.push_str(&format!("\"{col}\" IS NULL"));
        } else {
            binds.push(SqlBind::from_json_for_type(ftype, val)?);
            out.push_str(&format!("\"{col}\" = ?"));
        }
        return Ok(());
    }

    let ops = val.as_object().unwrap();
    let mut parts = Vec::new();

    for (op, operand) in ops {
        match op.as_str() {
            "$eq" => {
                if operand.is_null() {
                    parts.push(format!("\"{col}\" IS NULL"));
                } else {
                    binds.push(SqlBind::from_json_for_type(ftype, operand)?);
                    parts.push(format!("\"{col}\" = ?"));
                }
            }
            "$eqi" => {
                let s = operand
                    .as_str()
                    .ok_or_else(|| ApiError::BadRequest("$eqi expects a string".into()))?;
                binds.push(SqlBind::Text(s.to_lowercase()));
                parts.push(format!("LOWER(\"{col}\") = ?"));
            }
            "$ne" => {
                if operand.is_null() {
                    parts.push(format!("\"{col}\" IS NOT NULL"));
                } else {
                    binds.push(SqlBind::from_json_for_type(ftype, operand)?);
                    parts.push(format!("(\"{col}\" IS NULL OR \"{col}\" != ?)"));
                }
            }
            "$nei" => {
                let s = operand
                    .as_str()
                    .ok_or_else(|| ApiError::BadRequest("$nei expects a string".into()))?;
                binds.push(SqlBind::Text(s.to_lowercase()));
                parts.push(format!("(\"{col}\" IS NULL OR LOWER(\"{col}\") != ?)"));
            }
            "$lt" => {
                binds.push(SqlBind::from_json_for_type(ftype, operand)?);
                parts.push(format!("\"{col}\" < ?"));
            }
            "$lte" => {
                binds.push(SqlBind::from_json_for_type(ftype, operand)?);
                parts.push(format!("\"{col}\" <= ?"));
            }
            "$gt" => {
                binds.push(SqlBind::from_json_for_type(ftype, operand)?);
                parts.push(format!("\"{col}\" > ?"));
            }
            "$gte" => {
                binds.push(SqlBind::from_json_for_type(ftype, operand)?);
                parts.push(format!("\"{col}\" >= ?"));
            }
            "$contains" => {
                let s = operand
                    .as_str()
                    .ok_or_else(|| ApiError::BadRequest("$contains expects a string".into()))?;
                binds.push(SqlBind::Text(format!("%{}%", escape_like(s))));
                parts.push(format!("\"{col}\" LIKE ? ESCAPE '\\'"));
            }
            "$notContains" => {
                let s = operand
                    .as_str()
                    .ok_or_else(|| ApiError::BadRequest("$notContains expects a string".into()))?;
                binds.push(SqlBind::Text(format!("%{}%", escape_like(s))));
                parts.push(format!("(\"{col}\" IS NULL OR \"{col}\" NOT LIKE ? ESCAPE '\\')"));
            }
            "$containsi" => {
                let s = operand
                    .as_str()
                    .ok_or_else(|| ApiError::BadRequest("$containsi expects a string".into()))?;
                binds.push(SqlBind::Text(format!("%{}%", escape_like(&s.to_lowercase()))));
                parts.push(format!("LOWER(\"{col}\") LIKE ? ESCAPE '\\'"));
            }
            "$notContainsi" => {
                let s = operand
                    .as_str()
                    .ok_or_else(|| ApiError::BadRequest("$notContainsi expects a string".into()))?;
                binds.push(SqlBind::Text(format!("%{}%", escape_like(&s.to_lowercase()))));
                parts.push(format!(
                    "(\"{col}\" IS NULL OR LOWER(\"{col}\") NOT LIKE ? ESCAPE '\\')"
                ));
            }
            "$startsWith" => {
                let s = operand
                    .as_str()
                    .ok_or_else(|| ApiError::BadRequest("$startsWith expects a string".into()))?;
                binds.push(SqlBind::Text(format!("{}%", escape_like(s))));
                parts.push(format!("\"{col}\" LIKE ? ESCAPE '\\'"));
            }
            "$endsWith" => {
                let s = operand
                    .as_str()
                    .ok_or_else(|| ApiError::BadRequest("$endsWith expects a string".into()))?;
                binds.push(SqlBind::Text(format!("%{}", escape_like(s))));
                parts.push(format!("\"{col}\" LIKE ? ESCAPE '\\'"));
            }
            "$in" | "$notIn" => {
                let items = extract_filter_list(operand)?;
                if items.is_empty() {
                    parts.push(if op == "$in" {
                        "1 = 0".into()
                    } else {
                        "1 = 1".into()
                    });
                } else {
                    for it in &items {
                        binds.push(SqlBind::from_json_for_type(ftype, it)?);
                    }
                    let ph = vec!["?"; items.len()].join(", ");
                    let kw = if op == "$in" { "IN" } else { "NOT IN" };
                    parts.push(format!("\"{col}\" {kw} ({ph})"));
                }
            }
            "$null" => {
                let want_null = operand
                    .as_bool()
                    .or_else(|| operand.as_str().map(|s| s == "true"))
                    .unwrap_or(true);
                parts.push(if want_null {
                    format!("\"{col}\" IS NULL")
                } else {
                    format!("\"{col}\" IS NOT NULL")
                });
            }
            "$notNull" => {
                let want_not_null = operand
                    .as_bool()
                    .or_else(|| operand.as_str().map(|s| s == "true"))
                    .unwrap_or(true);
                parts.push(if want_not_null {
                    format!("\"{col}\" IS NOT NULL")
                } else {
                    format!("\"{col}\" IS NULL")
                });
            }
            "$between" => {
                let items = extract_filter_list(operand)?;
                if items.len() != 2 {
                    return Err(ApiError::BadRequest(
                        "$between expects exactly 2 values".into(),
                    ));
                }
                binds.push(SqlBind::from_json_for_type(ftype, items[0])?);
                binds.push(SqlBind::from_json_for_type(ftype, items[1])?);
                parts.push(format!("\"{col}\" BETWEEN ? AND ?"));
            }
            other => {
                return Err(ApiError::BadRequest(format!(
                    "Unsupported filter operator '{other}'"
                )));
            }
        }
    }

    out.push_str(&parts.join(" AND "));
    Ok(())
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn build_order_by(
    schema: &ContentTypeSchema,
    sorts: &[SortSpec],
    strip_private: bool,
) -> ApiResult<String> {
    if sorts.is_empty() {
        return Ok("\"created_at\" DESC, \"id\" DESC".into());
    }
    let mut parts = Vec::with_capacity(sorts.len());
    for s in sorts {
        let (col, _) = resolve_filterable_column(schema, &s.field, strip_private)?;
        let dir = if s.desc { "DESC" } else { "ASC" };
        parts.push(format!("\"{col}\" {dir}"));
    }
    Ok(parts.join(", "))
}

fn build_fts_match_query(raw: &str) -> Option<String> {
    let tokens: Vec<String> = raw
        .split_whitespace()
        .map(|tok| {
            tok.chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect::<String>()
        })
        .filter(|tok| !tok.is_empty())
        .map(|tok| format!("\"{tok}\"*"))
        .collect();

    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" AND "))
    }
}
