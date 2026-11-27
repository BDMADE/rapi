use cms_core::{
    ApiError, ApiResult, DocumentQuery, DocumentStatus, PopulateFieldOpts, PopulateSpec, SortSpec,
};
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
    #[serde(default, alias = "_q")]
    pub q: Option<String>,
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
    #[serde(default)]
    pub start: Option<u64>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default = "default_with_count")]
    pub with_count: bool,
}

impl Default for PaginationQuery {
    fn default() -> Self {
        Self {
            page: default_page(),
            page_size: default_page_size(),
            start: None,
            limit: None,
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

    pub fn into_document_query(self) -> DocumentQuery {
        let populate = match self.populate {
            None => PopulateSpec::None,
            Some(PopulateParam::Wildcard(s)) if s == "*" || s == "true" => PopulateSpec::All,
            Some(PopulateParam::Wildcard(field)) => {
                let mut map = BTreeMap::new();
                for part in field.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                    map.insert(part.to_string(), PopulateFieldOpts::default());
                }
                PopulateSpec::Fields(map)
            }
            Some(PopulateParam::List(list)) => {
                if list.iter().any(|s| s == "*") {
                    PopulateSpec::All
                } else {
                    let mut map = BTreeMap::new();
                    for f in list {
                        map.insert(f, PopulateFieldOpts::default());
                    }
                    PopulateSpec::Fields(map)
                }
            }
            Some(PopulateParam::Map(raw_map)) => {
                // Note: serde_qs may parse `populate[0]=a&populate[1]=b` as a Map with numeric keys!
                if raw_map.keys().all(|k| k.chars().all(|c| c.is_ascii_digit())) {
                    let mut map = BTreeMap::new();
                    for val in raw_map.values() {
                        if let Some(s) = val.as_str() {
                            if s == "*" {
                                return DocumentQuery {
                                    populate: PopulateSpec::All,
                                    filters: self.filters,
                                    fields: self.fields.map(StringOrList::into_vec).unwrap_or_default(),
                                    sort: parse_sorts(self.sort),
                                    page: self.pagination.page,
                                    page_size: self.pagination.page_size,
                                    start: self.pagination.start,
                                    limit: self.pagination.limit,
                                    locale: self.locale,
                                    status: self.status,
                                    search_query: self.q,
                                };
                            }
                            map.insert(s.to_string(), PopulateFieldOpts::default());
                        }
                    }
                    PopulateSpec::Fields(map)
                } else {
                    let mut map = BTreeMap::new();
                    for (field_name, cfg) in raw_map {
                        let mut opts = PopulateFieldOpts::default();
                        if let Some(obj) = cfg.as_object() {
                            if let Some(f_val) = obj.get("fields") {
                                if let Some(arr) = f_val.as_array() {
                                    opts.fields = arr
                                        .iter()
                                        .filter_map(|x| x.as_str().map(String::from))
                                        .collect();
                                } else if let Some(f_obj) = f_val.as_object() {
                                    opts.fields = f_obj
                                        .values()
                                        .filter_map(|x| x.as_str().map(String::from))
                                        .collect();
                                }
                            }
                            if obj.contains_key("populate") {
                                opts.populate_nested = true;
                            }
                        } else if cfg.as_bool() == Some(true) || cfg.as_str() == Some("*") {
                            opts.populate_nested = cfg.as_str() == Some("*");
                        }
                        map.insert(field_name, opts);
                    }
                    PopulateSpec::Fields(map)
                }
            }
        };

        DocumentQuery {
            filters: self.filters,
            populate,
            fields: self.fields.map(StringOrList::into_vec).unwrap_or_default(),
            sort: parse_sorts(self.sort),
            page: self.pagination.page,
            page_size: self.pagination.page_size,
            start: self.pagination.start,
            limit: self.pagination.limit,
            locale: self.locale,
            status: self.status,
            search_query: self.q,
        }
    }
}

fn parse_sorts(raw: Option<StringOrList>) -> Vec<SortSpec> {
    raw.map(StringOrList::into_vec)
        .unwrap_or_default()
        .into_iter()
        .map(|s| SortSpec::parse(&s))
        .collect()
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
