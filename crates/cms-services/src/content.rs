use crate::schema::SchemaService;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHasher, SaltString};
use argon2::Argon2;
use cms_core::{
    ApiError, ApiResult, ContentKind, ContentTypeSchema, ContentVersionDto, DocumentQuery,
    DocumentStatus, FieldDefinition, FieldType, InvalidationEvent, PaginationMeta, PopulateSpec,
    PubSubBackend, RelationMutation,
};
use cms_db::DynamicRepository;
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

pub struct ParsedWritePayload {
    pub scalars: BTreeMap<String, serde_json::Value>,
    pub relations: BTreeMap<String, RelationMutation>,
    pub media_links: BTreeMap<String, Vec<i64>>,
}

#[derive(Clone)]
pub struct ContentService {
    repo: DynamicRepository,
    schema_service: SchemaService,
    pubsub: Arc<dyn PubSubBackend>,
}

impl ContentService {
    pub fn new(
        repo: DynamicRepository,
        schema_service: SchemaService,
        pubsub: Arc<dyn PubSubBackend>,
    ) -> Self {
        Self {
            repo,
            schema_service,
            pubsub,
        }
    }

    pub async fn resolve_api_slug(&self, slug: &str) -> ApiResult<ContentTypeSchema> {
        let all = self.schema_service.list_schemas().await?;
        for s in all {
            match s.kind {
                ContentKind::CollectionType if s.plural_name == slug => return Ok(s),
                ContentKind::SingleType if s.singular_name == slug || s.plural_name == slug => {
                    return Ok(s);
                }
                _ => {}
            }
        }
        Err(ApiError::NotFound(format!(
            "Content API route '/api/{slug}' not found"
        )))
    }

    pub async fn find_many_public(
        &self,
        schema: &ContentTypeSchema,
        query: &DocumentQuery,
    ) -> ApiResult<(Vec<serde_json::Value>, PaginationMeta)> {
        self.repo.find_many(schema, query, true).await
    }

    pub async fn find_admin_entries(
        &self,
        uid: &str,
        query: &DocumentQuery,
    ) -> ApiResult<(Vec<serde_json::Value>, PaginationMeta)> {
        let schema = self.schema_service.get_schema(uid).await?;
        if schema.kind == ContentKind::Component {
            return Err(ApiError::BadRequest(
                "Components cannot be queried as standalone documents".into(),
            ));
        }
        self.repo.find_admin_entries(&schema, query).await
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
    ) -> ApiResult<serde_json::Value> {
        self.repo
            .find_one(
                schema,
                document_id,
                locale,
                status,
                populate,
                fields,
                strip_private,
            )
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Document '{document_id}' not found")))
    }

    pub async fn find_single(
        &self,
        schema: &ContentTypeSchema,
        locale: &str,
        status: DocumentStatus,
        populate: &PopulateSpec,
        fields: &[String],
        strip_private: bool,
    ) -> ApiResult<Option<serde_json::Value>> {
        self.repo
            .find_first_single(schema, locale, status, populate, fields, strip_private)
            .await
    }

    pub async fn create_entry(
        &self,
        schema: &ContentTypeSchema,
        locale: &str,
        target_status: DocumentStatus,
        raw_data: &serde_json::Map<String, serde_json::Value>,
        actor_id: Option<&str>,
        strip_private: bool,
    ) -> ApiResult<serde_json::Value> {
        if schema.kind == ContentKind::Component {
            return Err(ApiError::BadRequest(
                "Cannot create standalone entries for a component".into(),
            ));
        }

        let enforce_required =
            !schema.draft_and_publish || target_status == DocumentStatus::Published;
        let components_map = self.load_components_map().await?;
        let parsed = parse_and_validate_payload(
            schema,
            raw_data,
            &components_map,
            enforce_required,
            true,
        )?;

        let doc = self
            .repo
            .create_document(
                schema,
                locale,
                target_status,
                &parsed.scalars,
                &parsed.relations,
                &parsed.media_links,
                actor_id,
                strip_private,
            )
            .await?;

        if let Some(did) = doc.get("documentId").and_then(|v| v.as_str()) {
            self.emit_doc_event(&schema.uid, did).await;
        }
        Ok(doc)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_entry(
        &self,
        schema: &ContentTypeSchema,
        document_id: &str,
        locale: &str,
        target_status: DocumentStatus,
        raw_data: &serde_json::Map<String, serde_json::Value>,
        actor_id: Option<&str>,
        strip_private: bool,
    ) -> ApiResult<serde_json::Value> {
        let components_map = self.load_components_map().await?;
        // Partial updates validate provided fields; full required check runs on publish
        let parsed = parse_and_validate_payload(
            schema,
            raw_data,
            &components_map,
            false,
            false,
        )?;

        let doc = self
            .repo
            .update_document(
                schema,
                document_id,
                locale,
                target_status,
                &parsed.scalars,
                &parsed.relations,
                &parsed.media_links,
                actor_id,
                strip_private,
            )
            .await?;

        self.emit_doc_event(&schema.uid, document_id).await;
        Ok(doc)
    }

    pub async fn upsert_single(
        &self,
        schema: &ContentTypeSchema,
        locale: &str,
        target_status: DocumentStatus,
        raw_data: &serde_json::Map<String, serde_json::Value>,
        actor_id: Option<&str>,
        strip_private: bool,
    ) -> ApiResult<serde_json::Value> {
        let check_status = if schema.draft_and_publish {
            DocumentStatus::Draft
        } else {
            DocumentStatus::Published
        };
        if let Some(existing) = self
            .repo
            .find_first_single(schema, locale, check_status, &PopulateSpec::None, &[], false)
            .await?
        {
            if let Some(doc_id) = existing.get("documentId").and_then(|v| v.as_str()) {
                return self
                    .update_entry(
                        schema,
                        doc_id,
                        locale,
                        target_status,
                        raw_data,
                        actor_id,
                        strip_private,
                    )
                    .await;
            }
        }
        self.create_entry(
            schema,
            locale,
            target_status,
            raw_data,
            actor_id,
            strip_private,
        )
        .await
    }

    pub async fn publish_entry(
        &self,
        uid: &str,
        document_id: &str,
        locale: &str,
        actor_id: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let schema = self.schema_service.get_schema(uid).await?;

        // Validate required scalar fields on the draft before promoting to Published!
        if schema.draft_and_publish {
            let draft = self
                .repo
                .find_one(
                    &schema,
                    document_id,
                    locale,
                    DocumentStatus::Draft,
                    &PopulateSpec::None,
                    &[],
                    false,
                )
                .await?
                .ok_or_else(|| {
                    ApiError::NotFound(format!("Draft for '{document_id}' ({locale}) not found"))
                })?;

            if let Some(obj) = draft.as_object() {
                for f in schema.fields.iter().filter(|f| f.field_type.has_physical_column() && f.required) {
                    let missing = match obj.get(&f.name) {
                        None | Some(serde_json::Value::Null) => true,
                        Some(serde_json::Value::String(s)) => s.trim().is_empty(),
                        _ => false,
                    };
                    if missing {
                        return Err(ApiError::Validation {
                            message: format!(
                                "Cannot publish: required field '{}' is empty",
                                f.name
                            ),
                            details: serde_json::json!({ "field": f.name }),
                        });
                    }
                }
            }
        }

        let published = self
            .repo
            .publish_document(&schema, document_id, locale, actor_id)
            .await?;
        self.emit_doc_event(uid, document_id).await;
        Ok(published)
    }

    pub async fn unpublish_entry(
        &self,
        uid: &str,
        document_id: &str,
        locale: &str,
    ) -> ApiResult<serde_json::Value> {
        let schema = self.schema_service.get_schema(uid).await?;
        let draft = self
            .repo
            .unpublish_document(&schema, document_id, locale)
            .await?;
        self.emit_doc_event(uid, document_id).await;
        Ok(draft)
    }

    pub async fn delete_entry(
        &self,
        schema: &ContentTypeSchema,
        document_id: &str,
        locale: Option<&str>,
    ) -> ApiResult<()> {
        self.repo
            .delete_document(schema, document_id, locale)
            .await?;
        self.emit_doc_event(&schema.uid, document_id).await;
        Ok(())
    }

    pub async fn list_versions(
        &self,
        uid: &str,
        document_id: &str,
        locale: &str,
    ) -> ApiResult<Vec<ContentVersionDto>> {
        self.repo.list_versions(uid, document_id, locale).await
    }

    pub async fn restore_version(
        &self,
        uid: &str,
        document_id: &str,
        version_id: &str,
        actor_id: Option<&str>,
    ) -> ApiResult<serde_json::Value> {
        let schema = self.schema_service.get_schema(uid).await?;
        let doc = self
            .repo
            .restore_version(&schema, document_id, version_id, actor_id)
            .await?;
        self.emit_doc_event(uid, document_id).await;
        Ok(doc)
    }

    async fn load_components_map(&self) -> ApiResult<HashMap<String, ContentTypeSchema>> {
        let all = self.schema_service.list_schemas().await?;
        Ok(all
            .into_iter()
            .filter(|s| s.kind == ContentKind::Component)
            .map(|s| (s.uid.clone(), s))
            .collect())
    }

    async fn emit_doc_event(&self, uid: &str, document_id: &str) {
        let _ = self
            .pubsub
            .publish(InvalidationEvent::DocumentChanged {
                uid: uid.to_string(),
                document_id: document_id.to_string(),
            })
            .await;
    }
}

// --- Schema-Driven Payload Validation & Normalization ---

fn parse_and_validate_payload(
    schema: &ContentTypeSchema,
    raw: &serde_json::Map<String, serde_json::Value>,
    components: &HashMap<String, ContentTypeSchema>,
    enforce_required: bool,
    is_create: bool,
) -> ApiResult<ParsedWritePayload> {
    let mut scalars = BTreeMap::new();
    let mut relations = BTreeMap::new();
    let mut media_links = BTreeMap::new();

    for field in &schema.fields {
        let mut maybe_val = raw.get(&field.name).cloned();

        // Auto-slugify `uid` field from `targetField` on creation if omitted
        if is_create
            && field.field_type == FieldType::Uid
            && maybe_val
                .as_ref()
                .is_none_or(|v| v.is_null() || v.as_str().is_some_and(|s| s.trim().is_empty()))
        {
            if let Some(target_field) = &field.validation.target_field {
                if let Some(src_str) = raw.get(target_field).and_then(|v| v.as_str()) {
                    let generated = slugify_uid(src_str);
                    if !generated.is_empty() {
                        maybe_val = Some(serde_json::Value::String(generated));
                    }
                }
            } else if let Some(def) = &field.default_value {
                maybe_val = Some(def.clone());
            }
        } else if is_create && maybe_val.is_none() {
            if let Some(def) = &field.default_value {
                maybe_val = Some(def.clone());
            }
        }

        let is_missing = match &maybe_val {
            None | Some(serde_json::Value::Null) => true,
            Some(serde_json::Value::String(s))
                if field.required && field.field_type != FieldType::Password =>
            {
                s.trim().is_empty()
            }
            _ => false,
        };

        if is_missing {
            if enforce_required && field.required && field.field_type.has_physical_column() {
                return Err(ApiError::Validation {
                    message: format!("Field '{}' is required", field.name),
                    details: serde_json::json!({ "field": field.name }),
                });
            }
            if raw.contains_key(&field.name) {
                if field.field_type == FieldType::Relation {
                    relations.insert(
                        field.name.clone(),
                        RelationMutation {
                            set: Some(vec![]),
                            connect: vec![],
                            disconnect: vec![],
                        },
                    );
                } else if field.field_type == FieldType::Media {
                    media_links.insert(field.name.clone(), vec![]);
                } else {
                    scalars.insert(field.name.clone(), serde_json::Value::Null);
                }
            }
            continue;
        }

        let val = maybe_val.unwrap();

        match field.field_type {
            FieldType::Relation => {
                let mutation = RelationMutation::parse_value(&val).map_err(|msg| {
                    ApiError::Validation {
                        message: format!("Invalid relation '{}': {msg}", field.name),
                        details: serde_json::json!({ "field": field.name }),
                    }
                })?;
                relations.insert(field.name.clone(), mutation);
            }
            FieldType::Media => {
                let ids = parse_media_ids(&val).map_err(|msg| ApiError::Validation {
                    message: format!("Invalid media field '{}': {msg}", field.name),
                    details: serde_json::json!({ "field": field.name }),
                })?;
                media_links.insert(field.name.clone(), ids);
            }
            FieldType::Password => {
                let s = val.as_str().ok_or_else(|| ApiError::Validation {
                    message: format!("Password field '{}' must be a string", field.name),
                    details: serde_json::json!({ "field": field.name }),
                })?;
                if !s.is_empty() {
                    let salt = SaltString::generate(&mut OsRng);
                    let hashed = Argon2::default()
                        .hash_password(s.as_bytes(), &salt)
                        .map_err(|e| ApiError::Internal(format!("Password hash failed: {e}")))?
                        .to_string();
                    scalars.insert(field.name.clone(), serde_json::Value::String(hashed));
                }
            }
            FieldType::Component => {
                validate_component_value(field, &val, components, enforce_required, 0)?;
                scalars.insert(field.name.clone(), val);
            }
            FieldType::DynamicZone => {
                validate_dynamic_zone_value(field, &val, components, enforce_required)?;
                scalars.insert(field.name.clone(), val);
            }
            _ => {
                validate_scalar_value(field, &val)?;
                scalars.insert(field.name.clone(), val);
            }
        }
    }

    Ok(ParsedWritePayload {
        scalars,
        relations,
        media_links,
    })
}

fn validate_scalar_value(field: &FieldDefinition, val: &serde_json::Value) -> ApiResult<()> {
    match field.field_type {
        FieldType::String
        | FieldType::Text
        | FieldType::RichText
        | FieldType::Email
        | FieldType::Uid
        | FieldType::Date
        | FieldType::DateTime
        | FieldType::Time
        | FieldType::Enumeration => {
            let s = val.as_str().ok_or_else(|| ApiError::Validation {
                message: format!("Field '{}' must be a string", field.name),
                details: serde_json::json!({ "field": field.name }),
            })?;

            if let Some(min_l) = field.validation.min_length {
                if s.chars().count() < min_l {
                    return Err(ApiError::Validation {
                        message: format!(
                            "Field '{}' must be at least {min_l} characters",
                            field.name
                        ),
                        details: serde_json::json!({ "field": field.name }),
                    });
                }
            }
            if let Some(max_l) = field.validation.max_length {
                if s.chars().count() > max_l {
                    return Err(ApiError::Validation {
                        message: format!(
                            "Field '{}' must be at most {max_l} characters",
                            field.name
                        ),
                        details: serde_json::json!({ "field": field.name }),
                    });
                }
            }
            if let Some(pat) = &field.validation.regex {
                let re = Regex::new(pat).map_err(|e| ApiError::Validation {
                    message: format!("Invalid regex on '{}': {e}", field.name),
                    details: serde_json::json!({ "field": field.name }),
                })?;
                if !re.is_match(s) {
                    return Err(ApiError::Validation {
                        message: format!("Field '{}' does not match required pattern", field.name),
                        details: serde_json::json!({ "field": field.name }),
                    });
                }
            }
            if field.field_type == FieldType::Email && (!s.contains('@') || !s.contains('.')) {
                return Err(ApiError::Validation {
                    message: format!("Field '{}' must be a valid email address", field.name),
                    details: serde_json::json!({ "field": field.name }),
                });
            }
            if field.field_type == FieldType::Enumeration
                && !field.validation.enum_values.is_empty()
                && !field.validation.enum_values.iter().any(|ev| ev == s)
            {
                return Err(ApiError::Validation {
                    message: format!(
                        "Field '{}' must be one of [{}]",
                        field.name,
                        field.validation.enum_values.join(", ")
                    ),
                    details: serde_json::json!({ "field": field.name }),
                });
            }
        }
        FieldType::Integer | FieldType::BigInteger | FieldType::Float | FieldType::Decimal => {
            let num = val
                .as_f64()
                .or_else(|| val.as_str().and_then(|s| s.parse::<f64>().ok()))
                .ok_or_else(|| ApiError::Validation {
                    message: format!("Field '{}' must be a number", field.name),
                    details: serde_json::json!({ "field": field.name }),
                })?;
            if let Some(min_v) = field.validation.min {
                if num < min_v {
                    return Err(ApiError::Validation {
                        message: format!("Field '{}' must be >= {min_v}", field.name),
                        details: serde_json::json!({ "field": field.name }),
                    });
                }
            }
            if let Some(max_v) = field.validation.max {
                if num > max_v {
                    return Err(ApiError::Validation {
                        message: format!("Field '{}' must be <= {max_v}", field.name),
                        details: serde_json::json!({ "field": field.name }),
                    });
                }
            }
        }
        FieldType::Boolean if !val.is_boolean() => {
            return Err(ApiError::Validation {
                message: format!("Field '{}' must be a boolean", field.name),
                details: serde_json::json!({ "field": field.name }),
            });
        }
        FieldType::Blocks if !val.is_array() => {
            return Err(ApiError::Validation {
                message: format!(
                    "Blocks field '{}' must be a JSON array of block nodes",
                    field.name
                ),
                details: serde_json::json!({ "field": field.name }),
            });
        }
        _ => {}
    }
    Ok(())
}

fn validate_component_value(
    field: &FieldDefinition,
    val: &serde_json::Value,
    components: &HashMap<String, ContentTypeSchema>,
    enforce_required: bool,
    depth: usize,
) -> ApiResult<()> {
    if depth > 5 {
        return Ok(());
    }
    let Some(comp_cfg) = &field.component else {
        return Ok(());
    };
    let Some(comp_uid) = &comp_cfg.component_uid else {
        return Ok(());
    };
    let Some(comp_schema) = components.get(comp_uid) else {
        return Err(ApiError::Validation {
            message: format!("Unknown component '{comp_uid}'"),
            details: serde_json::json!({ "field": field.name }),
        });
    };

    if comp_cfg.repeatable {
        let arr = val.as_array().ok_or_else(|| ApiError::Validation {
            message: format!("Repeatable component '{}' must be an array", field.name),
            details: serde_json::json!({ "field": field.name }),
        })?;
        for item in arr {
            let obj = item.as_object().ok_or_else(|| ApiError::Validation {
                message: format!("Component item in '{}' must be an object", field.name),
                details: serde_json::json!({ "field": field.name }),
            })?;
            validate_component_object(comp_schema, obj, components, enforce_required, depth + 1)?;
        }
    } else {
        let obj = val.as_object().ok_or_else(|| ApiError::Validation {
            message: format!("Component '{}' must be an object", field.name),
            details: serde_json::json!({ "field": field.name }),
        })?;
        validate_component_object(comp_schema, obj, components, enforce_required, depth + 1)?;
    }
    Ok(())
}

fn validate_dynamic_zone_value(
    field: &FieldDefinition,
    val: &serde_json::Value,
    components: &HashMap<String, ContentTypeSchema>,
    enforce_required: bool,
) -> ApiResult<()> {
    let arr = val.as_array().ok_or_else(|| ApiError::Validation {
        message: format!("Dynamic zone '{}' must be an array of component blocks", field.name),
        details: serde_json::json!({ "field": field.name }),
    })?;
    let allowed = field
        .component
        .as_ref()
        .map(|c| c.components.as_slice())
        .unwrap_or(&[]);

    for block in arr {
        let obj = block.as_object().ok_or_else(|| ApiError::Validation {
            message: format!("Dynamic zone block in '{}' must be an object", field.name),
            details: serde_json::json!({ "field": field.name }),
        })?;
        let disc = obj
            .get("__component")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::Validation {
                message: format!(
                    "Every dynamic zone block in '{}' must include a '__component' UID string",
                    field.name
                ),
                details: serde_json::json!({ "field": field.name }),
            })?;

        if !allowed.iter().any(|c| c == disc) {
            return Err(ApiError::Validation {
                message: format!(
                    "Component '{disc}' is not permitted in dynamic zone '{}'",
                    field.name
                ),
                details: serde_json::json!({ "field": field.name, "__component": disc }),
            });
        }

        if let Some(comp_schema) = components.get(disc) {
            validate_component_object(comp_schema, obj, components, enforce_required, 1)?;
        }
    }
    Ok(())
}

fn validate_component_object(
    comp_schema: &ContentTypeSchema,
    obj: &serde_json::Map<String, serde_json::Value>,
    components: &HashMap<String, ContentTypeSchema>,
    enforce_required: bool,
    depth: usize,
) -> ApiResult<()> {
    for f in &comp_schema.fields {
        let maybe = obj.get(&f.name);
        if maybe.is_none_or(|v| v.is_null()) {
            if enforce_required && f.required {
                return Err(ApiError::Validation {
                    message: format!(
                        "Component '{}' requires field '{}'",
                        comp_schema.uid, f.name
                    ),
                    details: serde_json::json!({ "component": comp_schema.uid, "field": f.name }),
                });
            }
            continue;
        }
        let v = maybe.unwrap();
        if f.field_type == FieldType::Component {
            validate_component_value(f, v, components, enforce_required, depth)?;
        } else if f.field_type.has_physical_column() {
            validate_scalar_value(f, v)?;
        }
    }
    Ok(())
}

fn parse_media_ids(val: &serde_json::Value) -> Result<Vec<i64>, String> {
    match val {
        serde_json::Value::Null => Ok(vec![]),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(|id| vec![id])
            .ok_or_else(|| "Media ID must be an integer".into()),
        serde_json::Value::Object(obj) => obj
            .get("id")
            .and_then(|v| v.as_i64())
            .map(|id| vec![id])
            .ok_or_else(|| "Media object must contain numeric 'id'".into()),
        serde_json::Value::Array(arr) => {
            let mut ids = Vec::with_capacity(arr.len());
            for item in arr {
                if let Some(id) = item.as_i64() {
                    ids.push(id);
                } else if let Some(id) = item.get("id").and_then(|v| v.as_i64()) {
                    ids.push(id);
                } else {
                    return Err("Media array items must be numeric file IDs or { id } objects".into());
                }
            }
            Ok(ids)
        }
        _ => Err("Invalid media value".into()),
    }
}

fn slugify_uid(input: &str) -> String {
    let raw: String = input
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let mut out = String::with_capacity(raw.len());
    let mut prev_dash = true;
    for c in raw.chars() {
        if c == '-' {
            if !prev_dash {
                out.push('-');
            }
            prev_dash = true;
        } else {
            out.push(c);
            prev_dash = false;
        }
    }
    out.trim_end_matches('-').to_string()
}
