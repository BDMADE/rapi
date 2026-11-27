use crate::models::DocumentStatus;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Query options shared between REST Content API, Admin Content Manager, and GraphQL.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentQuery {
    #[serde(default)]
    pub filters: Option<serde_json::Value>,
    #[serde(default)]
    pub populate: PopulateSpec,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub sort: Vec<SortSpec>,
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_page_size")]
    pub page_size: u64,
    #[serde(default)]
    pub start: Option<u64>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub status: Option<DocumentStatus>,
    #[serde(default)]
    pub search_query: Option<String>,
}

fn default_page() -> u64 {
    1
}
fn default_page_size() -> u64 {
    25
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub enum PopulateSpec {
    #[default]
    None,
    All,
    Fields(BTreeMap<String, PopulateFieldOpts>),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopulateFieldOpts {
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub populate_nested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortSpec {
    pub field: String,
    pub desc: bool,
}

impl SortSpec {
    pub fn parse(raw: &str) -> Self {
        if let Some((f, dir)) = raw.split_once(':') {
            Self {
                field: f.trim().to_string(),
                desc: dir.trim().eq_ignore_ascii_case("desc"),
            }
        } else {
            Self {
                field: raw.trim().to_string(),
                desc: false,
            }
        }
    }
}

/// Strapi v5 relation mutation payload supporting `connect`, `disconnect`, `set`,
/// or direct `documentId` string / array shorthands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelationMutation {
    pub set: Option<Vec<String>>,
    pub connect: Vec<RelationConnectItem>,
    pub disconnect: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationConnectItem {
    pub document_id: String,
    pub position: RelationPosition,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RelationPosition {
    #[default]
    End,
    Start,
    Before(String),
    After(String),
}

impl RelationMutation {
    pub fn parse_value(val: &serde_json::Value) -> Result<Self, String> {
        match val {
            serde_json::Value::Null => Ok(Self {
                set: Some(vec![]),
                connect: vec![],
                disconnect: vec![],
            }),
            serde_json::Value::String(doc_id) => Ok(Self {
                set: Some(if doc_id.trim().is_empty() {
                    vec![]
                } else {
                    vec![doc_id.trim().to_string()]
                }),
                connect: vec![],
                disconnect: vec![],
            }),
            serde_json::Value::Array(items) => {
                let mut ids = Vec::with_capacity(items.len());
                for item in items {
                    if let Some(id) = extract_doc_id(item) {
                        ids.push(id);
                    } else {
                        return Err("Each relation array item must be a documentId string or { documentId } object".into());
                    }
                }
                Ok(Self {
                    set: Some(ids),
                    connect: vec![],
                    disconnect: vec![],
                })
            }
            serde_json::Value::Object(obj) => {
                // Single `{ "documentId": "..." }` object shorthand
                if obj.contains_key("documentId")
                    && !obj.contains_key("connect")
                    && !obj.contains_key("disconnect")
                    && !obj.contains_key("set")
                {
                    let id = obj["documentId"]
                        .as_str()
                        .ok_or_else(|| "documentId must be a string".to_string())?;
                    return Ok(Self {
                        set: Some(vec![id.to_string()]),
                        connect: vec![],
                        disconnect: vec![],
                    });
                }

                let has_set = obj.contains_key("set");
                let has_connect = obj.contains_key("connect");
                let has_disconnect = obj.contains_key("disconnect");

                if has_set && (has_connect || has_disconnect) {
                    return Err(
                        "Relation operator 'set' cannot be combined with 'connect' or 'disconnect'"
                            .into(),
                    );
                }

                let set = if let Some(set_val) = obj.get("set") {
                    Some(parse_doc_id_list(set_val)?)
                } else {
                    None
                };

                let connect = if let Some(conn_val) = obj.get("connect") {
                    parse_connect_list(conn_val)?
                } else {
                    vec![]
                };

                let disconnect = if let Some(disc_val) = obj.get("disconnect") {
                    parse_doc_id_list(disc_val)?
                } else {
                    vec![]
                };

                Ok(Self {
                    set,
                    connect,
                    disconnect,
                })
            }
            _ => Err("Invalid relation payload".into()),
        }
    }
}

fn extract_doc_id(val: &serde_json::Value) -> Option<String> {
    if let Some(s) = val.as_str() {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Some(obj) = val.as_object() {
        if let Some(s) = obj.get("documentId").and_then(|v| v.as_str()) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn parse_doc_id_list(val: &serde_json::Value) -> Result<Vec<String>, String> {
    let arr = val
        .as_array()
        .ok_or_else(|| "Expected an array of documentIds or { documentId } objects".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let id = extract_doc_id(item)
            .ok_or_else(|| "Relation item missing valid 'documentId'".to_string())?;
        out.push(id);
    }
    Ok(out)
}

fn parse_connect_list(val: &serde_json::Value) -> Result<Vec<RelationConnectItem>, String> {
    let arr = val
        .as_array()
        .ok_or_else(|| "'connect' must be an array".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let id = extract_doc_id(item)
            .ok_or_else(|| "'connect' item missing valid 'documentId'".to_string())?;
        let position = if let Some(pos_obj) = item.get("position").and_then(|v| v.as_object()) {
            if pos_obj.get("start").and_then(|v| v.as_bool()) == Some(true) {
                RelationPosition::Start
            } else if let Some(before) = pos_obj.get("before").and_then(|v| v.as_str()) {
                RelationPosition::Before(before.to_string())
            } else if let Some(after) = pos_obj.get("after").and_then(|v| v.as_str()) {
                RelationPosition::After(after.to_string())
            } else {
                RelationPosition::End
            }
        } else {
            RelationPosition::End
        };
        out.push(RelationConnectItem {
            document_id: id,
            position,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContentVersionDto {
    pub id: String,
    pub document_id: String,
    pub content_type_uid: String,
    pub version: i64,
    pub locale: String,
    pub status: String,
    pub data: serde_json::Value,
    pub created_by: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strapi_v5_connect_disconnect_and_set_operators() {
        let set_op = RelationMutation::parse_value(&serde_json::json!({
            "set": [{ "documentId": "doc1" }, { "documentId": "doc2" }]
        }))
        .unwrap();
        assert_eq!(
            set_op.set,
            Some(vec!["doc1".to_string(), "doc2".to_string()])
        );

        let conn_op = RelationMutation::parse_value(&serde_json::json!({
            "connect": [{ "documentId": "doc3", "position": { "start": true } }],
            "disconnect": [{ "documentId": "doc1" }]
        }))
        .unwrap();
        assert!(conn_op.set.is_none());
        assert_eq!(conn_op.connect[0].position, RelationPosition::Start);
        assert_eq!(conn_op.disconnect, vec!["doc1".to_string()]);

        let invalid = RelationMutation::parse_value(&serde_json::json!({
            "set": [{ "documentId": "doc1" }],
            "connect": [{ "documentId": "doc2" }]
        }));
        assert!(invalid.is_err());
    }
}
