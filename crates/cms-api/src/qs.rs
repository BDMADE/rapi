use cms_core::{ApiError, ApiResult, DocumentStatus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Strapi v5-compatible query parameters parsed via `serde_qs` (nested bracket syntax).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentQueryParams {
    #[serde(default)]
    pub filters: Option<serde_json::Value>,
    #[serde(default)]
    pub populate: Option<PopulateParam>,
    #[serde(default)]
    pub fields: Option<StringOrList>,
    #[serde(default)]
    pub sort: Option<StringOrList>,
    #[serde(default)]
    pub pagination: PaginationQuery,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub status: Option<DocumentStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PopulateParam {
    Wildcard(String),
    List(Vec<String>),
    Map(BTreeMap<String, serde_json::Value>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum StringOrList {
    Single(String),
    Multiple(Vec<String>),
}

impl StringOrList {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            Self::Single(s) => s
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect(),
            Self::Multiple(list) => list,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginationQuery {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_page_size")]
    pub page_size: u64,
    #[serde(default = "default_with_count")]
    pub with_count: bool,
}

impl Default for PaginationQuery {
    fn default() -> Self {
        Self {
            page: default_page(),
            page_size: default_page_size(),
            with_count: default_with_count(),
        }
    }
}

fn default_page() -> u64 {
    1
}
fn default_page_size() -> u64 {
    25
}
fn default_with_count() -> bool {
    true
}

impl ContentQueryParams {
    pub fn parse_query_str(raw: Option<&str>) -> ApiResult<Self> {
        let Some(raw) = raw.filter(|s| !s.is_empty()) else {
            return Ok(Self::default());
        };

        let config = serde_qs::Config::new(10, false);
        config
            .deserialize_str(raw)
            .map_err(|e| ApiError::BadRequest(format!("Invalid query parameters: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strapi_v5_nested_qs_syntax() {
        let raw = "filters[title][$containsi]=rust&populate[author][fields][0]=name&fields[0]=title&fields[1]=slug&sort[0]=createdAt:desc&pagination[page]=2&pagination[pageSize]=10&locale=fr&status=draft";
        let parsed = ContentQueryParams::parse_query_str(Some(raw)).unwrap();

        assert_eq!(parsed.pagination.page, 2);
        assert_eq!(parsed.pagination.page_size, 10);
        assert_eq!(parsed.locale.as_deref(), Some("fr"));
        assert_eq!(parsed.status, Some(DocumentStatus::Draft));
        assert_eq!(
            parsed.fields.unwrap().into_vec(),
            vec!["title".to_string(), "slug".to_string()]
        );
        assert!(matches!(parsed.populate, Some(PopulateParam::Map(_))));
        assert_eq!(parsed.filters.unwrap()["title"]["$containsi"], "rust");
    }

    #[test]
    fn parses_wildcard_populate() {
        let parsed = ContentQueryParams::parse_query_str(Some("populate=*")).unwrap();
        assert_eq!(
            parsed.populate,
            Some(PopulateParam::Wildcard("*".to_string()))
        );
    }
}
