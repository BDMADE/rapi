use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomFieldDto {
    pub uid: String,
    pub plugin_name: Option<String>,
    pub name: String,
    pub underlying_type: String,
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginDto {
    pub name: String,
    pub version: String,
    pub kind: String, // "native" | "wasm"
    pub description: String,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePluginDto {
    pub name: String,
    pub version: String,
    pub description: String,
    pub category: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SsoProviderDto {
    pub id: String,
    pub name: String,
    pub issuer_url: String,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub userinfo_endpoint: Option<String>,
    pub client_id: String,
    pub has_client_secret: bool,
    pub scopes: String,
    pub default_role_id: String,
    pub auto_provision: bool,
    pub is_enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublicSsoProviderDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsDto {
    pub is_enabled: bool,
    pub provider: String,
    pub api_url: String,
    pub has_api_key: bool,
    pub model: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiCompletionOutputDto {
    pub result: serde_json::Value,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}
