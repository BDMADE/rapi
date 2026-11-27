use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ContentKind {
    CollectionType,
    SingleType,
    Component,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DocumentStatus {
    Draft,
    Published,
}

impl DocumentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlatformStats {
    pub content_types: i64,
    pub documents: i64,
    pub published_documents: i64,
    pub media_files: i64,
    pub locales: i64,
    pub admin_users: i64,
    pub webhooks: i64,
    pub pending_jobs: i64,
}
