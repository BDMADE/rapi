use crate::error::{ApiError, ApiResult};
use crate::models::ContentKind;
use serde::{Deserialize, Serialize};

pub const RESERVED_FIELD_NAMES: &[&str] = &[
    "id",
    "document_id",
    "documentId",
    "locale",
    "status",
    "version",
    "published_at",
    "publishedAt",
    "created_at",
    "createdAt",
    "updated_at",
    "updatedAt",
    "created_by",
    "createdBy",
    "updated_by",
    "updatedBy",
];

/// All Strapi v5 field types supported by RAPI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Text,
    RichText,
    Blocks,
    Integer,
    BigInteger,
    Float,
    Decimal,
    Boolean,
    Date,
    DateTime,
    Time,
    Email,
    Password,
    Enumeration,
    Json,
    Uid,
    Media,
    Relation,
    Component,
    DynamicZone,
    Custom,
}

impl FieldType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Text => "text",
            Self::RichText => "richtext",
            Self::Blocks => "blocks",
            Self::Integer => "integer",
            Self::BigInteger => "biginteger",
            Self::Float => "float",
            Self::Decimal => "decimal",
            Self::Boolean => "boolean",
            Self::Date => "date",
            Self::DateTime => "datetime",
            Self::Time => "time",
            Self::Email => "email",
            Self::Password => "password",
            Self::Enumeration => "enumeration",
            Self::Json => "json",
            Self::Uid => "uid",
            Self::Media => "media",
            Self::Relation => "relation",
            Self::Component => "component",
            Self::DynamicZone => "dynamiczone",
            Self::Custom => "custom",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "string" => Some(Self::String),
            "text" => Some(Self::Text),
            "richtext" => Some(Self::RichText),
            "blocks" => Some(Self::Blocks),
            "integer" => Some(Self::Integer),
            "biginteger" => Some(Self::BigInteger),
            "float" => Some(Self::Float),
            "decimal" => Some(Self::Decimal),
            "boolean" => Some(Self::Boolean),
            "date" => Some(Self::Date),
            "datetime" => Some(Self::DateTime),
            "time" => Some(Self::Time),
            "email" => Some(Self::Email),
            "password" => Some(Self::Password),
            "enumeration" => Some(Self::Enumeration),
            "json" => Some(Self::Json),
            "uid" => Some(Self::Uid),
            "media" => Some(Self::Media),
            "relation" => Some(Self::Relation),
            "component" => Some(Self::Component),
            "dynamiczone" => Some(Self::DynamicZone),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    /// Whether this field has a physical column on `ct_<uid>` (Media and Relation live in status-isolated join tables).
    pub fn has_physical_column(&self) -> bool {
        !matches!(self, Self::Media | Self::Relation)
    }

    /// Canonical SQLite storage affinity used to detect column type changes on schema edits.
    pub fn sqlite_affinity(&self) -> &'static str {
        match self {
            Self::Integer | Self::BigInteger | Self::Boolean => "INTEGER",
            Self::Float | Self::Decimal => "REAL",
            _ => "TEXT",
        }
    }

    /// Whether text content in this field should be indexed in the content type's FTS5 table.
    pub fn is_searchable_text(&self) -> bool {
        matches!(
            self,
            Self::String | Self::Text | Self::RichText | Self::Email | Self::Uid
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FieldValidation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_values: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_types: Vec<String>,
    /// Optional conditional visibility rule (e.g. `{ "visible": { "field": "kind", "eq": "tutorial" } }`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditions: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RelationKind {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
    MorphOne,
    MorphMany,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelationConfig {
    pub relation_type: RelationKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inversed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapped_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentConfig {
    /// Target component UID when `field_type == Component` (e.g. `shared.seo`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_uid: Option<String>,
    #[serde(default)]
    pub repeatable: bool,
    /// Allowed component UIDs when `field_type == DynamicZone`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FieldDefinition {
    pub name: String,
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub localized: bool,
    #[serde(default)]
    pub private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
    #[serde(default)]
    pub validation: FieldValidation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation: Option<RelationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<ComponentConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_field_uid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContentTypeSchema {
    pub uid: String,
    pub kind: ContentKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub singular_name: String,
    pub plural_name: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default = "default_true")]
    pub draft_and_publish: bool,
    #[serde(default)]
    pub i18n_enabled: bool,
    #[serde(default = "empty_json_obj")]
    pub plugin_options: serde_json::Value,
    #[serde(default = "empty_json_obj")]
    pub options: serde_json::Value,
    #[serde(default)]
    pub fields: Vec<FieldDefinition>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn default_icon() -> String {
    "file-text".to_string()
}

fn default_true() -> bool {
    true
}

fn empty_json_obj() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalColumnInfo {
    pub cid: i64,
    pub name: String,
    pub data_type: String,
    pub not_null: bool,
    pub primary_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SchemaMigrationRecord {
    pub id: i64,
    pub content_type_uid: String,
    pub operation: String,
    pub ddl_sql: String,
    pub applied_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SchemaInspectionDto {
    pub uid: String,
    pub table_name: Option<String>,
    pub fts_table_name: Option<String>,
    pub columns: Vec<PhysicalColumnInfo>,
    pub migrations: Vec<SchemaMigrationRecord>,
}

/// Validates a safe SQL/API field or slug identifier (`[a-zA-Z][a-zA-Z0-9_]*`, max 64 chars).
pub fn validate_field_identifier(name: &str) -> ApiResult<()> {
    if name.is_empty() || name.len() > 64 {
        return Err(ApiError::Validation {
            message: format!("Field name '{name}' must be 1–64 characters"),
            details: serde_json::json!({ "field": name }),
        });
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() {
        return Err(ApiError::Validation {
            message: format!("Field name '{name}' must start with an ASCII letter"),
            details: serde_json::json!({ "field": name }),
        });
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(ApiError::Validation {
            message: format!(
                "Field name '{name}' may only contain ASCII letters, digits, and underscores"
            ),
            details: serde_json::json!({ "field": name }),
        });
    }
    if RESERVED_FIELD_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(name))
    {
        return Err(ApiError::Validation {
            message: format!("Field name '{name}' is reserved by the Document Service"),
            details: serde_json::json!({ "field": name }),
        });
    }
    Ok(())
}

/// Validates a Strapi-compatible UID (`api::article.article` or `shared.seo`).
pub fn validate_schema_uid(uid: &str, kind: ContentKind) -> ApiResult<()> {
    if uid.len() < 3 || uid.len() > 96 {
        return Err(ApiError::Validation {
            message: "Schema UID must be between 3 and 96 characters".into(),
            details: serde_json::json!({ "uid": uid }),
        });
    }

    match kind {
        ContentKind::CollectionType | ContentKind::SingleType => {
            let Some(rest) = uid.strip_prefix("api::") else {
                return Err(ApiError::Validation {
                    message: "Content-type UID must start with 'api::' (e.g. 'api::article.article')"
                        .into(),
                    details: serde_json::json!({ "uid": uid }),
                });
            };
            let parts: Vec<&str> = rest.split('.').collect();
            if parts.len() != 2 || parts.iter().any(|p| !is_kebab_or_snake_slug(p)) {
                return Err(ApiError::Validation {
                    message: "Content-type UID must match 'api::<api-name>.<singular-name>'".into(),
                    details: serde_json::json!({ "uid": uid }),
                });
            }
        }
        ContentKind::Component => {
            let parts: Vec<&str> = uid.split('.').collect();
            if parts.len() != 2 || parts.iter().any(|p| !is_kebab_or_snake_slug(p)) {
                return Err(ApiError::Validation {
                    message: "Component UID must match '<category>.<name>' (e.g. 'shared.seo')"
                        .into(),
                    details: serde_json::json!({ "uid": uid }),
                });
            }
        }
    }

    Ok(())
}

fn is_kebab_or_snake_slug(s: &str) -> bool {
    if s.is_empty() || s.len() > 48 {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    first.is_ascii_lowercase()
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Deterministically maps a validated UID to a safe SQLite physical table name (`ct_...`).
pub fn table_name_for_uid(uid: &str) -> String {
    let sanitized: String = uid
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    // Collapse consecutive underscores
    let mut compact = String::with_capacity(sanitized.len() + 4);
    compact.push_str("ct_");
    let mut prev_underscore = true;
    for ch in sanitized.chars() {
        if ch == '_' {
            if !prev_underscore {
                compact.push('_');
            }
            prev_underscore = true;
        } else {
            compact.push(ch);
            prev_underscore = false;
        }
    }
    compact.trim_end_matches('_').to_string()
}

pub fn fts_table_name_for_uid(uid: &str) -> String {
    let ct = table_name_for_uid(uid);
    format!("fts_{}", ct.trim_start_matches("ct_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_uids_and_generates_deterministic_table_names() {
        validate_schema_uid("api::article.article", ContentKind::CollectionType).unwrap();
        validate_schema_uid("api::homepage.homepage", ContentKind::SingleType).unwrap();
        validate_schema_uid("shared.seo-meta", ContentKind::Component).unwrap();

        assert!(validate_schema_uid("article", ContentKind::CollectionType).is_err());
        assert!(validate_schema_uid("api::Article.Article", ContentKind::CollectionType).is_err());

        assert_eq!(
            table_name_for_uid("api::article.article"),
            "ct_api_article_article"
        );
        assert_eq!(
            fts_table_name_for_uid("api::article.article"),
            "fts_api_article_article"
        );
    }

    #[test]
    fn blocks_reserved_and_unsafe_field_names() {
        validate_field_identifier("title").unwrap();
        validate_field_identifier("hero_banner_2").unwrap();
        assert!(validate_field_identifier("documentId").is_err());
        assert!(validate_field_identifier("id").is_err());
        assert!(validate_field_identifier("1bad").is_err());
        assert!(validate_field_identifier("bad;drop").is_err());
    }
}
