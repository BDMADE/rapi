use crate::pool::DbPools;
use cms_core::{
    fts_table_name_for_uid, table_name_for_uid, validate_field_identifier, validate_schema_uid,
    ApiError, ApiResult, ComponentConfig, ContentKind, ContentTypeSchema, FieldDefinition,
    FieldType, FieldValidation, PhysicalColumnInfo, RelationConfig, SchemaInspectionDto,
    SchemaMigrationRecord,
};
use sea_query::{Alias, ColumnDef, Index, SqliteQueryBuilder, Table};
use sqlx::{Row, Sqlite, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct SchemaEngine {
    pools: DbPools,
}

impl SchemaEngine {
    pub fn new(pools: DbPools) -> Self {
        Self { pools }
    }

    pub async fn list_schemas(&self) -> ApiResult<Vec<ContentTypeSchema>> {
        let rows = sqlx::query(
            r#"
            SELECT uid, kind, category, singular_name, plural_name, display_name, description,
                   icon, draft_and_publish, i18n_enabled, plugin_options_json, options_json,
                   created_at, updated_at
            FROM content_types
            ORDER BY kind ASC, display_name ASC
            "#,
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list content_types failed: {e}")))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let uid: String = row.get("uid");
            let fields = self.load_fields(&uid).await?;
            out.push(map_schema_row(row, fields));
        }
        Ok(out)
    }

    pub async fn get_schema(&self, uid: &str) -> ApiResult<Option<ContentTypeSchema>> {
        let row = sqlx::query(
            r#"
            SELECT uid, kind, category, singular_name, plural_name, display_name, description,
                   icon, draft_and_publish, i18n_enabled, plugin_options_json, options_json,
                   created_at, updated_at
            FROM content_types
            WHERE uid = ?
            LIMIT 1
            "#,
        )
        .bind(uid)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get content_type failed: {e}")))?;

        let Some(row) = row else {
            return Ok(None);
        };
        let fields = self.load_fields(uid).await?;
        Ok(Some(map_schema_row(row, fields)))
    }

    async fn load_fields(&self, content_type_uid: &str) -> ApiResult<Vec<FieldDefinition>> {
        let rows = sqlx::query(
            r#"
            SELECT name, field_type, required, unique_field, localized, private_field,
                   default_value_json, validation_json, relation_json, component_json, custom_field_uid
            FROM field_definitions
            WHERE content_type_uid = ?
            ORDER BY position ASC
            "#,
        )
        .bind(content_type_uid)
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("load field_definitions failed: {e}")))?;

        let mut fields = Vec::with_capacity(rows.len());
        for r in rows {
            let ft_str: String = r.get("field_type");
            let field_type = FieldType::parse_str(&ft_str).unwrap_or(FieldType::String);

            let default_value = r
                .get::<Option<String>, _>("default_value_json")
                .and_then(|s| serde_json::from_str(&s).ok());

            let validation: FieldValidation = r
                .get::<String, _>("validation_json")
                .parse::<serde_json::Value>()
                .ok()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();

            let relation: Option<RelationConfig> = r
                .get::<Option<String>, _>("relation_json")
                .and_then(|s| serde_json::from_str(&s).ok());

            let component: Option<ComponentConfig> = r
                .get::<Option<String>, _>("component_json")
                .and_then(|s| serde_json::from_str(&s).ok());

            fields.push(FieldDefinition {
                name: r.get("name"),
                field_type,
                required: r.get::<i64, _>("required") != 0,
                unique: r.get::<i64, _>("unique_field") != 0,
                localized: r.get::<i64, _>("localized") != 0,
                private: r.get::<i64, _>("private_field") != 0,
                default_value,
                validation,
                relation,
                component,
                custom_field_uid: r.get("custom_field_uid"),
            });
        }
        Ok(fields)
    }

    pub async fn create_schema(&self, schema: &ContentTypeSchema) -> ApiResult<ContentTypeSchema> {
        validate_schema_uid(&schema.uid, schema.kind)?;
        for f in &schema.fields {
            validate_field_identifier(&f.name)?;
        }

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin schema create tx failed: {e}")))?;

        let existing: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM content_types WHERE uid = ?")
                .bind(&schema.uid)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("check existing uid failed: {e}")))?;
        if existing > 0 {
            return Err(ApiError::Conflict(format!(
                "Schema '{}' already exists",
                schema.uid
            )));
        }

        insert_metadata(&mut tx, schema).await?;

        if schema.kind != ContentKind::Component {
            let table_name = table_name_for_uid(&schema.uid);
            let fts_name = fts_table_name_for_uid(&schema.uid);
            let ddl_statements = build_create_table_and_indexes_ddl(&table_name, &fts_name, &schema.fields);
            execute_and_log_ddl(&mut tx, &schema.uid, "create_table", &ddl_statements).await?;
        }

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit schema create failed: {e}")))?;

        self.get_schema(&schema.uid)
            .await?
            .ok_or_else(|| ApiError::Internal("Created schema not found".into()))
    }

    pub async fn update_schema(&self, schema: &ContentTypeSchema) -> ApiResult<ContentTypeSchema> {
        validate_schema_uid(&schema.uid, schema.kind)?;
        for f in &schema.fields {
            validate_field_identifier(&f.name)?;
        }

        let Some(old_schema) = self.get_schema(&schema.uid).await? else {
            return Err(ApiError::NotFound(format!(
                "Schema '{}' not found",
                schema.uid
            )));
        };

        if old_schema.kind != schema.kind {
            return Err(ApiError::BadRequest(
                "Changing schema kind (collectionType / singleType / component) is not permitted"
                    .into(),
            ));
        }

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin schema update tx failed: {e}")))?;

        update_metadata(&mut tx, schema).await?;

        if schema.kind != ContentKind::Component {
            evolve_physical_table(&mut tx, &old_schema, schema).await?;
            cleanup_removed_virtual_fields(&mut tx, &old_schema, schema).await?;
        }

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit schema update failed: {e}")))?;

        self.get_schema(&schema.uid)
            .await?
            .ok_or_else(|| ApiError::Internal("Updated schema not found".into()))
    }

    pub async fn delete_schema(&self, uid: &str) -> ApiResult<()> {
        let Some(existing) = self.get_schema(uid).await? else {
            return Err(ApiError::NotFound(format!("Schema '{uid}' not found")));
        };

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin schema delete tx failed: {e}")))?;

        if existing.kind != ContentKind::Component {
            let table_name = table_name_for_uid(uid);
            let fts_name = fts_table_name_for_uid(uid);
            let drop_table = Table::drop()
                .table(Alias::new(&table_name))
                .if_exists()
                .to_string(SqliteQueryBuilder);
            let drop_fts = format!("DROP TABLE IF EXISTS \"{fts_name}\"");
            execute_and_log_ddl(&mut tx, uid, "drop_table", &[drop_table, drop_fts]).await?;

            // Clean up status-isolated relations, media links, and version snapshots for this type
            sqlx::query(
                "DELETE FROM content_relations WHERE from_content_type_uid = ? OR to_content_type_uid = ?",
            )
            .bind(uid)
            .bind(uid)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("cleanup content_relations failed: {e}")))?;

            sqlx::query(
                "DELETE FROM upload_file_relations WHERE from_document_id IN (SELECT document_id FROM documents WHERE content_type_uid = ?)",
            )
            .bind(uid)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("cleanup upload_file_relations failed: {e}")))?;

            sqlx::query("DELETE FROM content_versions WHERE content_type_uid = ?")
                .bind(uid)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("cleanup content_versions failed: {e}")))?;
        }

        sqlx::query("DELETE FROM content_types WHERE uid = ?")
            .bind(uid)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete content_type failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit schema delete failed: {e}")))?;

        Ok(())
    }

    pub async fn inspect_schema(&self, uid: &str) -> ApiResult<SchemaInspectionDto> {
        let Some(schema) = self.get_schema(uid).await? else {
            return Err(ApiError::NotFound(format!("Schema '{uid}' not found")));
        };

        let (table_name, fts_table_name, columns) = if schema.kind == ContentKind::Component {
            (None, None, vec![])
        } else {
            let tname = table_name_for_uid(uid);
            let fname = fts_table_name_for_uid(uid);
            let pragma_sql = format!("PRAGMA table_info(\"{tname}\")");
            let rows = sqlx::query(&pragma_sql)
                .fetch_all(&self.pools.reader)
                .await
                .map_err(|e| ApiError::Internal(format!("PRAGMA table_info failed: {e}")))?;
            let cols = rows
                .into_iter()
                .map(|r| PhysicalColumnInfo {
                    cid: r.get("cid"),
                    name: r.get("name"),
                    data_type: r.get("type"),
                    not_null: r.get::<i64, _>("notnull") != 0,
                    primary_key: r.get::<i64, _>("pk") != 0,
                })
                .collect();
            (Some(tname), Some(fname), cols)
        };

        let mig_rows = sqlx::query(
            r#"
            SELECT id, content_type_uid, operation, ddl_sql, applied_at
            FROM schema_migrations
            WHERE content_type_uid = ?
            ORDER BY id DESC
            LIMIT 50
            "#,
        )
        .bind(uid)
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("fetch schema_migrations failed: {e}")))?;

        let migrations = mig_rows
            .into_iter()
            .map(|r| SchemaMigrationRecord {
                id: r.get("id"),
                content_type_uid: r.get("content_type_uid"),
                operation: r.get("operation"),
                ddl_sql: r.get("ddl_sql"),
                applied_at: r.get("applied_at"),
            })
            .collect();

        Ok(SchemaInspectionDto {
            uid: uid.to_string(),
            table_name,
            fts_table_name,
            columns,
            migrations,
        })
    }
}

fn kind_to_str(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::CollectionType => "collectionType",
        ContentKind::SingleType => "singleType",
        ContentKind::Component => "component",
    }
}

fn str_to_kind(s: &str) -> ContentKind {
    match s {
        "singleType" => ContentKind::SingleType,
        "component" => ContentKind::Component,
        _ => ContentKind::CollectionType,
    }
}

fn map_schema_row(row: sqlx::sqlite::SqliteRow, fields: Vec<FieldDefinition>) -> ContentTypeSchema {
    let kind_str: String = row.get("kind");
    let plugin_options: serde_json::Value = serde_json::from_str(
        &row.get::<String, _>("plugin_options_json"),
    )
    .unwrap_or_else(|_| serde_json::json!({}));
    let options: serde_json::Value =
        serde_json::from_str(&row.get::<String, _>("options_json"))
            .unwrap_or_else(|_| serde_json::json!({}));

    ContentTypeSchema {
        uid: row.get("uid"),
        kind: str_to_kind(&kind_str),
        category: row.get("category"),
        singular_name: row.get("singular_name"),
        plural_name: row.get("plural_name"),
        display_name: row.get("display_name"),
        description: row.get("description"),
        icon: row.get("icon"),
        draft_and_publish: row.get::<i64, _>("draft_and_publish") != 0,
        i18n_enabled: row.get::<i64, _>("i18n_enabled") != 0,
        plugin_options,
        options,
        fields,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

async fn insert_metadata(
    tx: &mut Transaction<'_, Sqlite>,
    schema: &ContentTypeSchema,
) -> ApiResult<()> {
    sqlx::query(
        r#"
        INSERT INTO content_types (
            uid, kind, category, singular_name, plural_name, display_name, description,
            icon, draft_and_publish, i18n_enabled, plugin_options_json, options_json
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&schema.uid)
    .bind(kind_to_str(schema.kind))
    .bind(&schema.category)
    .bind(&schema.singular_name)
    .bind(&schema.plural_name)
    .bind(&schema.display_name)
    .bind(&schema.description)
    .bind(&schema.icon)
    .bind(if schema.draft_and_publish { 1 } else { 0 })
    .bind(if schema.i18n_enabled { 1 } else { 0 })
    .bind(schema.plugin_options.to_string())
    .bind(schema.options.to_string())
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("insert content_type failed: {e}")))?;

    insert_field_definitions(tx, &schema.uid, &schema.fields).await
}

async fn update_metadata(
    tx: &mut Transaction<'_, Sqlite>,
    schema: &ContentTypeSchema,
) -> ApiResult<()> {
    sqlx::query(
        r#"
        UPDATE content_types
        SET category = ?, singular_name = ?, plural_name = ?, display_name = ?,
            description = ?, icon = ?, draft_and_publish = ?, i18n_enabled = ?,
            plugin_options_json = ?, options_json = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE uid = ?
        "#,
    )
    .bind(&schema.category)
    .bind(&schema.singular_name)
    .bind(&schema.plural_name)
    .bind(&schema.display_name)
    .bind(&schema.description)
    .bind(&schema.icon)
    .bind(if schema.draft_and_publish { 1 } else { 0 })
    .bind(if schema.i18n_enabled { 1 } else { 0 })
    .bind(schema.plugin_options.to_string())
    .bind(schema.options.to_string())
    .bind(&schema.uid)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("update content_type failed: {e}")))?;

    sqlx::query("DELETE FROM field_definitions WHERE content_type_uid = ?")
        .bind(&schema.uid)
        .execute(&mut **tx)
        .await
        .map_err(|e| ApiError::Internal(format!("clear field_definitions failed: {e}")))?;

    insert_field_definitions(tx, &schema.uid, &schema.fields).await
}

async fn insert_field_definitions(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    fields: &[FieldDefinition],
) -> ApiResult<()> {
    for (idx, f) in fields.iter().enumerate() {
        let fid = Uuid::now_v7().to_string();
        let default_str = f.default_value.as_ref().map(|v| v.to_string());
        let val_str = serde_json::to_string(&f.validation).unwrap_or_else(|_| "{}".to_string());
        let rel_str = f
            .relation
            .as_ref()
            .and_then(|r| serde_json::to_string(r).ok());
        let comp_str = f
            .component
            .as_ref()
            .and_then(|c| serde_json::to_string(c).ok());

        sqlx::query(
            r#"
            INSERT INTO field_definitions (
                id, content_type_uid, name, field_type, required, unique_field,
                localized, private_field, position, default_value_json,
                validation_json, relation_json, component_json, custom_field_uid
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(fid)
        .bind(uid)
        .bind(&f.name)
        .bind(f.field_type.as_str())
        .bind(if f.required { 1 } else { 0 })
        .bind(if f.unique { 1 } else { 0 })
        .bind(if f.localized { 1 } else { 0 })
        .bind(if f.private { 1 } else { 0 })
        .bind(idx as i64)
        .bind(default_str)
        .bind(val_str)
        .bind(rel_str)
        .bind(comp_str)
        .bind(&f.custom_field_uid)
        .execute(&mut **tx)
        .await
        .map_err(|e| ApiError::Internal(format!("insert field '{}' failed: {e}", f.name)))?;
    }
    Ok(())
}

/// Builds `CREATE TABLE` + composite/field indexes via `sea-query` + FTS5 virtual table DDL.
fn build_create_table_and_indexes_ddl(
    table_name: &str,
    fts_table_name: &str,
    fields: &[FieldDefinition],
) -> Vec<String> {
    let mut create = Table::create();
    create
        .table(Alias::new(table_name))
        .if_not_exists()
        .col(
            ColumnDef::new(Alias::new("id"))
                .integer()
                .not_null()
                .auto_increment()
                .primary_key(),
        )
        .col(ColumnDef::new(Alias::new("document_id")).text().not_null())
        .col(
            ColumnDef::new(Alias::new("locale"))
                .text()
                .not_null()
                .default("en"),
        )
        .col(
            ColumnDef::new(Alias::new("status"))
                .text()
                .not_null()
                .default("draft"),
        )
        .col(
            ColumnDef::new(Alias::new("version"))
                .integer()
                .not_null()
                .default(1),
        )
        .col(ColumnDef::new(Alias::new("published_at")).text())
        .col(ColumnDef::new(Alias::new("created_at")).text().not_null())
        .col(ColumnDef::new(Alias::new("updated_at")).text().not_null())
        .col(ColumnDef::new(Alias::new("created_by")).text())
        .col(ColumnDef::new(Alias::new("updated_by")).text());

    for f in fields.iter().filter(|f| f.field_type.has_physical_column()) {
        create.col(build_column_def(f));
    }

    let mut statements = vec![create.to_string(SqliteQueryBuilder)];
    statements.extend(build_index_statements(table_name, fields));

    // Provision FTS5 virtual table for full-text search across entries
    statements.push(format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS \"{fts_table_name}\" USING fts5(document_id UNINDEXED, locale UNINDEXED, status UNINDEXED, content_text)"
    ));

    statements
}

fn build_index_statements(table_name: &str, fields: &[FieldDefinition]) -> Vec<String> {
    let mut stmts = Vec::new();

    let uniq_doc = Index::create()
        .if_not_exists()
        .unique()
        .name(format!("idx_{table_name}_doc_loc_status"))
        .table(Alias::new(table_name))
        .col(Alias::new("document_id"))
        .col(Alias::new("locale"))
        .col(Alias::new("status"))
        .to_string(SqliteQueryBuilder);
    stmts.push(uniq_doc);

    let pub_idx = Index::create()
        .if_not_exists()
        .name(format!("idx_{table_name}_status_pub"))
        .table(Alias::new(table_name))
        .col(Alias::new("status"))
        .col(Alias::new("published_at"))
        .to_string(SqliteQueryBuilder);
    stmts.push(pub_idx);

    for f in fields
        .iter()
        .filter(|f| f.field_type.has_physical_column() && f.unique)
    {
        // Scope unique constraint per (field, locale, status) so draft & published rows of the same documentId can coexist!
        let idx = Index::create()
            .if_not_exists()
            .unique()
            .name(format!("idx_{table_name}_uniq_{}", f.name))
            .table(Alias::new(table_name))
            .col(Alias::new(&f.name))
            .col(Alias::new("locale"))
            .col(Alias::new("status"))
            .to_string(SqliteQueryBuilder);
        stmts.push(idx);
    }

    stmts
}

fn build_column_def(field: &FieldDefinition) -> ColumnDef {
    let mut col = ColumnDef::new(Alias::new(&field.name));
    match field.field_type.sqlite_affinity() {
        "INTEGER" => {
            col.integer();
        }
        "REAL" => {
            col.double();
        }
        _ => {
            col.text();
        }
    }
    // Keep user-defined columns nullable at the raw SQLite DDL level with service-layer required
    // enforcement so draft autosaves and `ALTER TABLE ADD COLUMN` on populated tables never fail.
    col
}

/// Evolves `ct_<uid>` safely:
/// - Pure field additions use `ALTER TABLE ADD COLUMN` + new unique indexes.
/// - Removals, type changes, or constraint changes run a transactional 12-step SQLite table rebuild.
async fn evolve_physical_table(
    tx: &mut Transaction<'_, Sqlite>,
    old_schema: &ContentTypeSchema,
    new_schema: &ContentTypeSchema,
) -> ApiResult<()> {
    let table_name = table_name_for_uid(&new_schema.uid);
    let fts_name = fts_table_name_for_uid(&new_schema.uid);

    let old_phys: Vec<&FieldDefinition> = old_schema
        .fields
        .iter()
        .filter(|f| f.field_type.has_physical_column())
        .collect();
    let new_phys: Vec<&FieldDefinition> = new_schema
        .fields
        .iter()
        .filter(|f| f.field_type.has_physical_column())
        .collect();

    let needs_rebuild = old_phys.iter().any(|old_f| {
        match new_phys.iter().find(|nf| nf.name == old_f.name) {
            None => true, // column removed
            Some(new_f) => {
                old_f.field_type.sqlite_affinity() != new_f.field_type.sqlite_affinity()
                    || (old_f.unique && !new_f.unique)
            }
        }
    });

    if !needs_rebuild {
        let mut stmts = Vec::new();
        let mut added_unique = false;

        for nf in &new_phys {
            let old_match = old_phys.iter().find(|of| of.name == nf.name);
            if old_match.is_none() {
                let alter = Table::alter()
                    .table(Alias::new(&table_name))
                    .add_column(build_column_def(nf))
                    .to_string(SqliteQueryBuilder);
                stmts.push(alter);
                if nf.unique {
                    added_unique = true;
                }
            } else if let Some(of) = old_match {
                if !of.unique && nf.unique {
                    added_unique = true;
                }
            }
        }

        if !stmts.is_empty() || added_unique {
            stmts.extend(build_index_statements(&table_name, &new_schema.fields));
            execute_and_log_ddl(tx, &new_schema.uid, "alter_add_columns", &stmts).await?;
        }
        return Ok(());
    }

    // Transactional 12-step SQLite table rebuild preserving all shared columns
    let temp_table = format!("{table_name}__rebuild");
    let mut create_temp = Table::create();
    create_temp
        .table(Alias::new(&temp_table))
        .col(
            ColumnDef::new(Alias::new("id"))
                .integer()
                .not_null()
                .auto_increment()
                .primary_key(),
        )
        .col(ColumnDef::new(Alias::new("document_id")).text().not_null())
        .col(
            ColumnDef::new(Alias::new("locale"))
                .text()
                .not_null()
                .default("en"),
        )
        .col(
            ColumnDef::new(Alias::new("status"))
                .text()
                .not_null()
                .default("draft"),
        )
        .col(
            ColumnDef::new(Alias::new("version"))
                .integer()
                .not_null()
                .default(1),
        )
        .col(ColumnDef::new(Alias::new("published_at")).text())
        .col(ColumnDef::new(Alias::new("created_at")).text().not_null())
        .col(ColumnDef::new(Alias::new("updated_at")).text().not_null())
        .col(ColumnDef::new(Alias::new("created_by")).text())
        .col(ColumnDef::new(Alias::new("updated_by")).text());

    for nf in &new_phys {
        create_temp.col(build_column_def(nf));
    }

    let mut shared_cols: Vec<String> = vec![
        "id",
        "document_id",
        "locale",
        "status",
        "version",
        "published_at",
        "created_at",
        "updated_at",
        "created_by",
        "updated_by",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    for nf in &new_phys {
        if old_phys.iter().any(|of| of.name == nf.name) {
            shared_cols.push(nf.name.clone());
        }
    }

    let quoted_cols = shared_cols
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let copy_sql =
        format!("INSERT INTO \"{temp_table}\" ({quoted_cols}) SELECT {quoted_cols} FROM \"{table_name}\"");
    let drop_old = Table::drop()
        .table(Alias::new(&table_name))
        .to_string(SqliteQueryBuilder);
    let rename_sql = Table::rename()
        .table(Alias::new(&temp_table), Alias::new(&table_name))
        .to_string(SqliteQueryBuilder);

    let mut rebuild_stmts = vec![
        create_temp.to_string(SqliteQueryBuilder),
        copy_sql,
        drop_old,
        rename_sql,
    ];
    rebuild_stmts.extend(build_index_statements(&table_name, &new_schema.fields));
    rebuild_stmts.push(format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS \"{fts_name}\" USING fts5(document_id UNINDEXED, locale UNINDEXED, status UNINDEXED, content_text)"
    ));

    execute_and_log_ddl(tx, &new_schema.uid, "rebuild_table", &rebuild_stmts).await
}

async fn cleanup_removed_virtual_fields(
    tx: &mut Transaction<'_, Sqlite>,
    old_schema: &ContentTypeSchema,
    new_schema: &ContentTypeSchema,
) -> ApiResult<()> {
    for old_f in &old_schema.fields {
        let still_same_virtual = new_schema
            .fields
            .iter()
            .any(|nf| nf.name == old_f.name && nf.field_type == old_f.field_type);
        if still_same_virtual {
            continue;
        }
        if old_f.field_type == FieldType::Relation {
            sqlx::query(
                "DELETE FROM content_relations WHERE from_content_type_uid = ? AND field_name = ?",
            )
            .bind(&new_schema.uid)
            .bind(&old_f.name)
            .execute(&mut **tx)
            .await
            .map_err(|e| ApiError::Internal(format!("cleanup removed relation field failed: {e}")))?;
        } else if old_f.field_type == FieldType::Media {
            sqlx::query(
                "DELETE FROM upload_file_relations WHERE field_name = ? AND from_document_id IN (SELECT document_id FROM documents WHERE content_type_uid = ?)",
            )
            .bind(&old_f.name)
            .bind(&new_schema.uid)
            .execute(&mut **tx)
            .await
            .map_err(|e| ApiError::Internal(format!("cleanup removed media field failed: {e}")))?;
        }
    }
    Ok(())
}

async fn execute_and_log_ddl(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    operation: &str,
    statements: &[String],
) -> ApiResult<()> {
    for stmt in statements {
        if let Err(e) = sqlx::query(stmt).execute(&mut **tx).await {
            let msg = e.to_string();
            if msg.contains("UNIQUE constraint failed") {
                return Err(ApiError::Conflict(
                    "Cannot apply unique constraint: duplicate values already exist for this field"
                        .to_string(),
                ));
            }
            return Err(ApiError::Internal(format!(
                "DDL execution failed ({stmt}): {e}"
            )));
        }
    }

    let combined = statements.join(";\n");
    sqlx::query(
        "INSERT INTO schema_migrations (content_type_uid, operation, ddl_sql) VALUES (?, ?, ?)",
    )
    .bind(uid)
    .bind(operation)
    .bind(combined)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::Internal(format!("log schema_migration failed: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cms_core::FieldValidation;

    fn sample_article_schema(fields: Vec<FieldDefinition>) -> ContentTypeSchema {
        ContentTypeSchema {
            uid: "api::article.article".into(),
            kind: ContentKind::CollectionType,
            category: None,
            singular_name: "article".into(),
            plural_name: "articles".into(),
            display_name: "Article".into(),
            description: "Blog articles".into(),
            icon: "file-text".into(),
            draft_and_publish: true,
            i18n_enabled: true,
            plugin_options: serde_json::json!({}),
            options: serde_json::json!({}),
            fields,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn text_field(name: &str, field_type: FieldType, unique: bool) -> FieldDefinition {
        FieldDefinition {
            name: name.into(),
            field_type,
            required: true,
            unique,
            localized: true,
            private: false,
            default_value: None,
            validation: FieldValidation::default(),
            relation: None,
            component: None,
            custom_field_uid: None,
        }
    }

    #[tokio::test]
    async fn creates_alters_and_rebuilds_sqlite_table_preserving_rows() {
        let pools = DbPools::connect("sqlite::memory:").await.unwrap();
        pools.run_migrations().await.unwrap();
        let engine = SchemaEngine::new(pools.clone());

        // 1. Create Article collection type
        let initial = sample_article_schema(vec![
            text_field("title", FieldType::String, false),
            text_field("slug", FieldType::Uid, true),
        ]);
        engine.create_schema(&initial).await.unwrap();

        // Insert a live row into ct_api_article_article
        sqlx::query(
            r#"
            INSERT INTO ct_api_article_article (document_id, locale, status, version, created_at, updated_at, title, slug)
            VALUES ('doc123456789012345678901', 'en', 'draft', 1, '2026-11-27T00:00:00Z', '2026-11-27T00:00:00Z', 'Hello Rust', 'hello-rust')
            "#,
        )
        .execute(&pools.writer)
        .await
        .unwrap();

        // 2. Add `views` integer column (uses ALTER TABLE ADD COLUMN)
        let with_views = sample_article_schema(vec![
            text_field("title", FieldType::String, false),
            text_field("slug", FieldType::Uid, true),
            text_field("views", FieldType::Integer, false),
        ]);
        engine.update_schema(&with_views).await.unwrap();

        let inspection = engine.inspect_schema("api::article.article").await.unwrap();
        assert!(inspection.columns.iter().any(|c| c.name == "views" && c.data_type == "INTEGER"));

        // 3. Remove `slug` column (triggers transactional 12-step rebuild while preserving `title`!)
        let rebuilt = sample_article_schema(vec![
            text_field("title", FieldType::String, false),
            text_field("views", FieldType::Integer, false),
        ]);
        engine.update_schema(&rebuilt).await.unwrap();

        let after_inspect = engine.inspect_schema("api::article.article").await.unwrap();
        assert!(!after_inspect.columns.iter().any(|c| c.name == "slug"));
        assert_eq!(after_inspect.migrations.len(), 3);

        let preserved_title: String =
            sqlx::query_scalar("SELECT title FROM ct_api_article_article WHERE document_id = 'doc123456789012345678901'")
                .fetch_one(&pools.reader)
                .await
                .unwrap();
        assert_eq!(preserved_title, "Hello Rust");
    }
}
