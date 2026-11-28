use cms_core::{
    validate_field_identifier, validate_schema_uid, ApiError, ApiResult, CacheBackend, ContentKind,
    ContentTypeSchema, FieldType, InvalidationEvent, PubSubBackend, RelationKind,
    SchemaInspectionDto,
};
use cms_db::SchemaEngine;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

const SCHEMA_CACHE_ALL_KEY: &str = "rapi:schema:__all__";
const SCHEMA_CACHE_TTL_SECS: u64 = 300;
const RESERVED_API_SLUGS: &[&str] = &[
    "auth", "upload", "users", "files", "folders", "graphql", "mcp", "admin", "health", "ready",
    "docs", "openapi",
];

#[derive(Clone)]
pub struct SchemaService {
    engine: SchemaEngine,
    cache: Arc<dyn CacheBackend>,
    pubsub: Arc<dyn PubSubBackend>,
}

impl SchemaService {
    pub fn new(
        engine: SchemaEngine,
        cache: Arc<dyn CacheBackend>,
        pubsub: Arc<dyn PubSubBackend>,
    ) -> Self {
        Self {
            engine,
            cache,
            pubsub,
        }
    }

    pub async fn list_schemas(&self) -> ApiResult<Vec<ContentTypeSchema>> {
        if let Ok(Some(cached)) = self.cache.get(SCHEMA_CACHE_ALL_KEY).await {
            if let Ok(list) = serde_json::from_str::<Vec<ContentTypeSchema>>(&cached) {
                return Ok(list);
            }
        }

        let schemas = self.engine.list_schemas().await?;
        if let Ok(json) = serde_json::to_string(&schemas) {
            let _ = self
                .cache
                .set(
                    SCHEMA_CACHE_ALL_KEY,
                    &json,
                    Some(Duration::from_secs(SCHEMA_CACHE_TTL_SECS)),
                )
                .await;
        }
        Ok(schemas)
    }

    pub async fn get_schema(&self, uid: &str) -> ApiResult<ContentTypeSchema> {
        let key = format!("rapi:schema:{uid}");
        if let Ok(Some(cached)) = self.cache.get(&key).await {
            if let Ok(schema) = serde_json::from_str::<ContentTypeSchema>(&cached) {
                return Ok(schema);
            }
        }

        let schema = self
            .engine
            .get_schema(uid)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Schema '{uid}' not found")))?;

        if let Ok(json) = serde_json::to_string(&schema) {
            let _ = self
                .cache
                .set(&key, &json, Some(Duration::from_secs(SCHEMA_CACHE_TTL_SECS)))
                .await;
        }
        Ok(schema)
    }

    pub async fn create_schema(&self, mut schema: ContentTypeSchema) -> ApiResult<ContentTypeSchema> {
        normalize_and_validate_schema(&mut schema)?;
        self.validate_custom_field_references(&schema).await?;
        let existing_all = self.engine.list_schemas().await?;
        check_api_slug_collisions(&schema, &existing_all, true)?;
        validate_cross_schema_references(&schema, &existing_all)?;
        detect_component_cycle(&schema, &existing_all)?;

        let created = self.engine.create_schema(&schema).await?;
        self.invalidate_schema(&created.uid).await;
        Ok(created)
    }

    pub async fn update_schema(
        &self,
        uid: &str,
        mut schema: ContentTypeSchema,
    ) -> ApiResult<ContentTypeSchema> {
        schema.uid = uid.to_string();
        normalize_and_validate_schema(&mut schema)?;
        self.validate_custom_field_references(&schema).await?;
        let existing_all = self.engine.list_schemas().await?;
        check_api_slug_collisions(&schema, &existing_all, false)?;
        validate_cross_schema_references(&schema, &existing_all)?;
        detect_component_cycle(&schema, &existing_all)?;

        let updated = self.engine.update_schema(&schema).await?;
        self.invalidate_schema(uid).await;
        Ok(updated)
    }

    async fn validate_custom_field_references(&self, schema: &ContentTypeSchema) -> ApiResult<()> {
        for f in &schema.fields {
            if f.field_type == FieldType::Custom {
                let cf_uid = f.custom_field_uid.as_deref().unwrap_or("").trim();
                if !self.engine.custom_field_exists(cf_uid).await? {
                    return Err(ApiError::Validation {
                        message: format!(
                            "Field '{}' references unregistered customFieldUid '{cf_uid}'",
                            f.name
                        ),
                        details: serde_json::json!({ "field": f.name, "customFieldUid": cf_uid }),
                    });
                }
            }
        }
        Ok(())
    }

    pub async fn delete_schema(&self, uid: &str) -> ApiResult<()> {
        let existing_all = self.engine.list_schemas().await?;
        // Block deletion if another schema still references this UID via Relation, Component, or DynamicZone
        for other in &existing_all {
            if other.uid == uid {
                continue;
            }
            for f in &other.fields {
                if let Some(rel) = &f.relation {
                    if rel.target_uid.as_deref() == Some(uid) {
                        return Err(ApiError::Conflict(format!(
                            "Schema '{uid}' is targeted by relation '{}.{}' and cannot be deleted",
                            other.uid, f.name
                        )));
                    }
                }
                if let Some(comp) = &f.component {
                    if comp.component_uid.as_deref() == Some(uid)
                        || comp.components.iter().any(|c| c == uid)
                    {
                        return Err(ApiError::Conflict(format!(
                            "Component '{uid}' is used in '{}.{}' and cannot be deleted",
                            other.uid, f.name
                        )));
                    }
                }
            }
        }

        self.engine.delete_schema(uid).await?;
        self.invalidate_schema(uid).await;
        Ok(())
    }

    pub async fn inspect_schema(&self, uid: &str) -> ApiResult<SchemaInspectionDto> {
        self.engine.inspect_schema(uid).await
    }

    async fn invalidate_schema(&self, uid: &str) {
        let _ = self.cache.delete(&format!("rapi:schema:{uid}")).await;
        let _ = self.cache.delete(SCHEMA_CACHE_ALL_KEY).await;
        let _ = self
            .pubsub
            .publish(InvalidationEvent::SchemaUpdated {
                uid: uid.to_string(),
            })
            .await;
    }
}

fn normalize_and_validate_schema(schema: &mut ContentTypeSchema) -> ApiResult<()> {
    schema.uid = schema.uid.trim().to_string();
    schema.display_name = schema.display_name.trim().to_string();
    schema.singular_name = schema.singular_name.trim().to_lowercase();
    schema.plural_name = schema.plural_name.trim().to_lowercase();

    validate_schema_uid(&schema.uid, schema.kind)?;

    if schema.display_name.is_empty() {
        return Err(ApiError::Validation {
            message: "Display name is required".into(),
            details: serde_json::json!({ "field": "displayName" }),
        });
    }
    if !is_api_slug(&schema.singular_name) || !is_api_slug(&schema.plural_name) {
        return Err(ApiError::Validation {
            message: "singularName and pluralName must be lowercase slugs (letters, digits, hyphens)"
                .into(),
            details: serde_json::json!({
                "singularName": schema.singular_name,
                "pluralName": schema.plural_name
            }),
        });
    }
    if schema.kind == ContentKind::CollectionType && schema.singular_name == schema.plural_name {
        return Err(ApiError::Validation {
            message: "Collection types require distinct singularName and pluralName".into(),
            details: serde_json::json!({
                "singularName": schema.singular_name,
                "pluralName": schema.plural_name
            }),
        });
    }
    if schema.kind == ContentKind::Component {
        let cat = schema
            .category
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| schema.uid.split('.').next().unwrap_or("shared"))
            .to_lowercase();
        schema.category = Some(cat);
        schema.draft_and_publish = false;
        schema.i18n_enabled = false;
    } else if RESERVED_API_SLUGS.contains(&schema.singular_name.as_str())
        || RESERVED_API_SLUGS.contains(&schema.plural_name.as_str())
    {
        return Err(ApiError::Validation {
            message: format!(
                "API slug '{}' / '{}' is reserved by built-in platform routes",
                schema.singular_name, schema.plural_name
            ),
            details: serde_json::json!({
                "singularName": schema.singular_name,
                "pluralName": schema.plural_name
            }),
        });
    }

    let mut seen_fields = HashSet::new();
    for f in &schema.fields {
        validate_field_identifier(&f.name)?;
        let lower = f.name.to_ascii_lowercase();
        if !seen_fields.insert(lower) {
            return Err(ApiError::Validation {
                message: format!("Duplicate field name '{}'", f.name),
                details: serde_json::json!({ "field": f.name }),
            });
        }

        if let (Some(min_l), Some(max_l)) = (f.validation.min_length, f.validation.max_length) {
            if min_l > max_l {
                return Err(ApiError::Validation {
                    message: format!(
                        "Field '{}': minLength ({min_l}) cannot exceed maxLength ({max_l})",
                        f.name
                    ),
                    details: serde_json::json!({ "field": f.name }),
                });
            }
        }
        if let (Some(min_v), Some(max_v)) = (f.validation.min, f.validation.max) {
            if min_v > max_v {
                return Err(ApiError::Validation {
                    message: format!("Field '{}': min ({min_v}) cannot exceed max ({max_v})", f.name),
                    details: serde_json::json!({ "field": f.name }),
                });
            }
        }
        if let Some(regex) = &f.validation.regex {
            if regex.len() > 256 {
                return Err(ApiError::Validation {
                    message: format!(
                        "Field '{}': regex pattern exceeds maximum length of 256 characters",
                        f.name
                    ),
                    details: serde_json::json!({ "field": f.name }),
                });
            }
        }

        match f.field_type {
            FieldType::Enumeration => {
                if f.validation.enum_values.is_empty() {
                    return Err(ApiError::Validation {
                        message: format!(
                            "Enumeration field '{}' requires at least one enum value",
                            f.name
                        ),
                        details: serde_json::json!({ "field": f.name }),
                    });
                }
            }
            FieldType::Relation => {
                let Some(rel) = &f.relation else {
                    return Err(ApiError::Validation {
                        message: format!("Relation field '{}' is missing relation config", f.name),
                        details: serde_json::json!({ "field": f.name }),
                    });
                };
                if !matches!(rel.relation_type, RelationKind::MorphOne | RelationKind::MorphMany)
                    && rel.target_uid.as_deref().unwrap_or("").is_empty()
                {
                    return Err(ApiError::Validation {
                        message: format!("Relation field '{}' requires targetUid", f.name),
                        details: serde_json::json!({ "field": f.name }),
                    });
                }
            }
            FieldType::Component => {
                let Some(comp) = &f.component else {
                    return Err(ApiError::Validation {
                        message: format!(
                            "Component field '{}' is missing component config",
                            f.name
                        ),
                        details: serde_json::json!({ "field": f.name }),
                    });
                };
                let target = comp.component_uid.as_deref().unwrap_or("");
                if target.is_empty() {
                    return Err(ApiError::Validation {
                        message: format!("Component field '{}' requires componentUid", f.name),
                        details: serde_json::json!({ "field": f.name }),
                    });
                }
                if target == schema.uid {
                    return Err(ApiError::Validation {
                        message: format!("Component '{}' cannot embed itself", schema.uid),
                        details: serde_json::json!({ "field": f.name }),
                    });
                }
            }
            FieldType::DynamicZone => {
                if schema.kind == ContentKind::Component {
                    return Err(ApiError::Validation {
                        message: "Dynamic zones cannot be nested inside components".into(),
                        details: serde_json::json!({ "field": f.name }),
                    });
                }
                let Some(comp) = &f.component else {
                    return Err(ApiError::Validation {
                        message: format!(
                            "Dynamic zone '{}' requires a list of allowed components",
                            f.name
                        ),
                        details: serde_json::json!({ "field": f.name }),
                    });
                };
                if comp.components.is_empty() {
                    return Err(ApiError::Validation {
                        message: format!(
                            "Dynamic zone '{}' must include at least one component",
                            f.name
                        ),
                        details: serde_json::json!({ "field": f.name }),
                    });
                }
            }
            FieldType::Uid => {
                if let Some(target_field) = &f.validation.target_field {
                    if !schema.fields.iter().any(|other| &other.name == target_field) {
                        return Err(ApiError::Validation {
                            message: format!(
                                "UID field '{}' targets unknown field '{target_field}'",
                                f.name
                            ),
                            details: serde_json::json!({ "field": f.name }),
                        });
                    }
                }
            }
            FieldType::Custom
                if f.custom_field_uid
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty() =>
            {
                return Err(ApiError::Validation {
                    message: format!(
                        "Custom field '{}' requires a registered customFieldUid",
                        f.name
                    ),
                    details: serde_json::json!({ "field": f.name }),
                });
            }
            _ => {}
        }
    }

    Ok(())
}

fn check_api_slug_collisions(
    candidate: &ContentTypeSchema,
    existing: &[ContentTypeSchema],
    is_create: bool,
) -> ApiResult<()> {
    if candidate.kind == ContentKind::Component {
        return Ok(());
    }

    for other in existing {
        if other.kind == ContentKind::Component {
            continue;
        }
        if !is_create && other.uid == candidate.uid {
            continue;
        }
        if other.plural_name == candidate.plural_name
            || other.singular_name == candidate.singular_name
            || other.plural_name == candidate.singular_name
            || other.singular_name == candidate.plural_name
        {
            return Err(ApiError::Conflict(format!(
                "API route slug '{}' / '{}' conflicts with existing schema '{}'",
                candidate.singular_name, candidate.plural_name, other.uid
            )));
        }
    }
    Ok(())
}

fn validate_cross_schema_references(
    candidate: &ContentTypeSchema,
    existing: &[ContentTypeSchema],
) -> ApiResult<()> {
    for f in &candidate.fields {
        if f.field_type == FieldType::Relation {
            if let Some(rel) = &f.relation {
                if let Some(target_uid) = &rel.target_uid {
                    if target_uid != &candidate.uid {
                        let found = existing.iter().any(|s| {
                            &s.uid == target_uid && s.kind != ContentKind::Component
                        });
                        if !found {
                            return Err(ApiError::Validation {
                                message: format!(
                                    "Relation '{}' targets unknown content type '{target_uid}'",
                                    f.name
                                ),
                                details: serde_json::json!({ "field": f.name, "targetUid": target_uid }),
                            });
                        }
                    }
                }
            }
        } else if f.field_type == FieldType::Component {
            if let Some(comp) = &f.component {
                if let Some(comp_uid) = &comp.component_uid {
                    let found = existing
                        .iter()
                        .any(|s| &s.uid == comp_uid && s.kind == ContentKind::Component);
                    if !found {
                        return Err(ApiError::Validation {
                            message: format!(
                                "Component field '{}' references unknown component '{comp_uid}'",
                                f.name
                            ),
                            details: serde_json::json!({ "field": f.name, "componentUid": comp_uid }),
                        });
                    }
                }
            }
        } else if f.field_type == FieldType::DynamicZone {
            if let Some(comp) = &f.component {
                for comp_uid in &comp.components {
                    let found = existing
                        .iter()
                        .any(|s| &s.uid == comp_uid && s.kind == ContentKind::Component);
                    if !found {
                        return Err(ApiError::Validation {
                            message: format!(
                                "Dynamic zone '{}' references unknown component '{comp_uid}'",
                                f.name
                            ),
                            details: serde_json::json!({ "field": f.name, "componentUid": comp_uid }),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

fn is_api_slug(s: &str) -> bool {
    if s.is_empty() || s.len() > 48 {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    first.is_ascii_lowercase()
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Detects transitive circular component references (`A -> B -> A`) via DFS.
fn detect_component_cycle(
    candidate: &ContentTypeSchema,
    existing: &[ContentTypeSchema],
) -> ApiResult<()> {
    if candidate.kind != ContentKind::Component {
        return Ok(());
    }

    let mut graph: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for s in existing {
        if s.kind == ContentKind::Component && s.uid != candidate.uid {
            graph.insert(s.uid.clone(), component_edges(s));
        }
    }
    graph.insert(candidate.uid.clone(), component_edges(candidate));

    let mut visited = HashSet::new();
    let mut stack = vec![candidate.uid.clone()];
    while let Some(node) = stack.pop() {
        if let Some(neighbors) = graph.get(&node) {
            for next in neighbors {
                if next == &candidate.uid {
                    return Err(ApiError::Validation {
                        message: format!(
                            "Circular component reference detected involving '{}'",
                            candidate.uid
                        ),
                        details: serde_json::json!({ "uid": candidate.uid }),
                    });
                }
                if visited.insert(next.clone()) {
                    stack.push(next.clone());
                }
            }
        }
    }
    Ok(())
}

fn component_edges(schema: &ContentTypeSchema) -> Vec<String> {
    let mut edges = Vec::new();
    for f in &schema.fields {
        if f.field_type == FieldType::Component {
            if let Some(comp) = &f.component {
                if let Some(uid) = &comp.component_uid {
                    edges.push(uid.clone());
                }
            }
        }
    }
    edges
}
