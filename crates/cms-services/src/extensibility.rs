use crate::auth::{random_hex_32, AuthService};
use crate::content::ContentService;
use crate::governance::GovernanceService;
use crate::integrations::{verify_webhook_dns_not_private, IntegrationsService};
use crate::media::MediaService;
use crate::schema::SchemaService;
use base64::Engine as _;
use cms_core::{
    validate_webhook_url, Actor, AiCompletionOutputDto, AiSettingsDto, ApiError, ApiResult,
    AppConfig, AuthTokenResponse, CacheBackend, ContentKind, CustomFieldDto, DocumentQuery,
    DocumentStatus, InstalledPluginDto, MarketplacePluginDto, McpToolDescriptor, MediaListFilter,
    PopulateSpec, PublicSsoProviderDto, SsoProviderDto,
};
use cms_db::ExtensibilityRepository;
use cms_plugin_api::{builtin_marketplace_catalog, PluginRegistry};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const OIDC_STATE_TTL_SECS: u64 = 600;
const BUILTIN_CUSTOM_FIELD_UIDS: &[&str] = &[
    "global::color-picker",
    "global::slug-seo",
    "global::code-editor",
    "global::key-value",
];

// ============================================================================
// Input / Output DTOs
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterCustomFieldInput {
    pub uid: String,
    pub plugin_name: Option<String>,
    pub name: String,
    pub underlying_type: String,
    #[serde(default = "empty_json_obj")]
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallCustomWasmInput {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub wasm_base64: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePluginInput {
    pub enabled: Option<bool>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvokePluginInput {
    #[serde(default = "default_invoke_action")]
    pub action: String,
    #[serde(default = "empty_json_obj")]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSsoProviderInput {
    pub name: String,
    pub issuer_url: String,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub userinfo_endpoint: Option<String>,
    pub client_id: String,
    pub client_secret: String,
    #[serde(default = "default_oidc_scopes")]
    pub scopes: String,
    #[serde(default = "default_editor_role")]
    pub default_role_id: String,
    #[serde(default = "default_true")]
    pub auto_provision: bool,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsoAuthorizeUrlDto {
    pub authorize_url: String,
    pub state: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SsoCallbackInput {
    pub code: String,
    pub state: String,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSsoProviderInput {
    pub is_enabled: Option<bool>,
    pub auto_provision: Option<bool>,
    pub default_role_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAiSettingsInput {
    pub is_enabled: Option<bool>,
    pub provider: Option<String>,
    pub api_url: Option<String>,
    pub api_key: Option<Option<String>>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCompletionInput {
    pub task: String,
    pub content: String,
    pub target_locale: Option<String>,
    pub context: Option<String>,
}

fn empty_json_obj() -> serde_json::Value {
    serde_json::json!({})
}

fn default_version() -> String {
    "0.1.0".to_string()
}

fn default_invoke_action() -> String {
    "run".to_string()
}

fn default_oidc_scopes() -> String {
    "openid email profile".to_string()
}

fn default_editor_role() -> String {
    "role_editor".to_string()
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Service
// ============================================================================

#[derive(Clone)]
pub struct ExtensibilityService {
    repo: ExtensibilityRepository,
    config: Arc<AppConfig>,
    plugins: PluginRegistry,
    cache: Arc<dyn CacheBackend>,
    auth: AuthService,
    schema: SchemaService,
    content: ContentService,
    media: MediaService,
    governance: GovernanceService,
    integrations: IntegrationsService,
    http_client: reqwest::Client,
}

impl ExtensibilityService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: ExtensibilityRepository,
        config: Arc<AppConfig>,
        plugins: PluginRegistry,
        cache: Arc<dyn CacheBackend>,
        auth: AuthService,
        schema: SchemaService,
        content: ContentService,
        media: MediaService,
        governance: GovernanceService,
        integrations: IntegrationsService,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();

        Self {
            repo,
            config,
            plugins,
            cache,
            auth,
            schema,
            content,
            media,
            governance,
            integrations,
            http_client,
        }
    }

    // =========================================================================
    // 1. Custom Fields Registry
    // =========================================================================

    pub async fn list_custom_fields(&self) -> ApiResult<Vec<CustomFieldDto>> {
        self.repo.list_custom_fields().await
    }

    pub async fn register_custom_field(
        &self,
        input: RegisterCustomFieldInput,
    ) -> ApiResult<CustomFieldDto> {
        let uid = input.uid.trim().to_lowercase();
        let valid_uid = uid.split_once("::").is_some_and(|(ns, slug)| {
            !ns.is_empty()
                && !slug.is_empty()
                && ns.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        });
        if !valid_uid || uid.len() > 80 {
            return Err(ApiError::Validation {
                message: "Custom field UID must match '<namespace>::<slug>' (lowercase ASCII & hyphens)".into(),
                details: serde_json::json!({ "field": "uid" }),
            });
        }

        let name = input.name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(ApiError::Validation {
                message: "Custom field display name must be 1–80 characters".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }

        let underlying = input.underlying_type.trim().to_lowercase();
        if !matches!(
            underlying.as_str(),
            "string" | "text" | "integer" | "float" | "boolean" | "json"
        ) {
            return Err(ApiError::Validation {
                message: "underlyingType must be one of: string, text, integer, float, boolean, json".into(),
                details: serde_json::json!({ "field": "underlyingType" }),
            });
        }

        let plugin_name = input
            .plugin_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        self.repo
            .upsert_custom_field(&uid, plugin_name, name, &underlying, &input.options)
            .await
    }

    pub async fn delete_custom_field(&self, uid: &str) -> ApiResult<()> {
        if BUILTIN_CUSTOM_FIELD_UIDS.contains(&uid) {
            return Err(ApiError::BadRequest(format!(
                "Built-in custom field '{uid}' cannot be deleted"
            )));
        }
        self.repo.delete_custom_field(uid).await
    }

    // =========================================================================
    // 2. Native & Sandboxed WASM Plugins + Marketplace
    // =========================================================================

    pub async fn sync_native_plugins(&self) -> ApiResult<()> {
        let existing: HashSet<String> = self
            .repo
            .list_db_plugins()
            .await?
            .into_iter()
            .map(|p| p.dto.name)
            .collect();

        for desc in self.plugins.descriptors() {
            if !existing.contains(&desc.name) {
                self.repo
                    .upsert_db_plugin(
                        &desc.name,
                        &desc.version,
                        "native",
                        &desc.description,
                        true,
                        None,
                        &serde_json::json!({}),
                    )
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn list_installed_plugins(&self) -> ApiResult<Vec<InstalledPluginDto>> {
        self.sync_native_plugins().await?;
        Ok(self
            .repo
            .list_db_plugins()
            .await?
            .into_iter()
            .map(|r| r.dto)
            .collect())
    }

    pub async fn list_marketplace_plugins(&self) -> ApiResult<Vec<MarketplacePluginDto>> {
        let installed: HashSet<String> = self
            .repo
            .list_db_plugins()
            .await?
            .into_iter()
            .map(|r| r.dto.name)
            .collect();

        Ok(builtin_marketplace_catalog()
            .into_iter()
            .map(|(name, version, category, description, _)| MarketplacePluginDto {
                name: name.to_string(),
                version: version.to_string(),
                category: category.to_string(),
                description: description.to_string(),
                installed: installed.contains(name),
            })
            .collect())
    }

    pub async fn install_marketplace_plugin(&self, name: &str) -> ApiResult<InstalledPluginDto> {
        let catalog = builtin_marketplace_catalog();
        let Some((cname, version, _cat, description, wasm_bytes)) =
            catalog.into_iter().find(|(n, _, _, _, _)| *n == name)
        else {
            return Err(ApiError::NotFound(format!(
                "Marketplace plugin '{name}' not found"
            )));
        };

        self.plugins.wasm_host().validate_module(&wasm_bytes)?;
        let wasm_file = self.write_wasm_artifact(cname, &wasm_bytes)?;
        let saved = self
            .repo
            .upsert_db_plugin(
                cname,
                version,
                "wasm",
                description,
                true,
                Some(&wasm_file),
                &serde_json::json!({}),
            )
            .await?;
        Ok(saved.dto)
    }

    pub async fn install_custom_wasm_plugin(
        &self,
        input: InstallCustomWasmInput,
    ) -> ApiResult<InstalledPluginDto> {
        let name = input.name.trim().to_lowercase();
        if name.len() < 2
            || name.len() > 64
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(ApiError::Validation {
                message: "Plugin name must be 2–64 lowercase ASCII letters, digits, hyphens, or underscores".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }
        if self.plugins.get_native(&name).is_some() {
            return Err(ApiError::Conflict(format!(
                "Cannot overwrite compile-time native plugin '{name}'"
            )));
        }

        let cleaned_b64: String = input
            .wasm_base64
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let wasm_bytes = base64::engine::general_purpose::STANDARD
            .decode(cleaned_b64)
            .map_err(|e| ApiError::Validation {
                message: format!("Invalid base64 WASM payload: {e}"),
                details: serde_json::json!({ "field": "wasmBase64" }),
            })?;

        self.plugins.wasm_host().validate_module(&wasm_bytes)?;
        let wasm_file = self.write_wasm_artifact(&name, &wasm_bytes)?;
        let version = {
            let v = input.version.trim();
            if v.is_empty() { "0.1.0" } else { v }
        };
        let description = {
            let d = input.description.trim();
            if d.is_empty() {
                "Custom sandboxed WebAssembly plugin."
            } else {
                d
            }
        };

        let saved = self
            .repo
            .upsert_db_plugin(
                &name,
                version,
                "wasm",
                description,
                true,
                Some(&wasm_file),
                &serde_json::json!({}),
            )
            .await?;
        Ok(saved.dto)
    }

    pub async fn update_plugin(
        &self,
        name: &str,
        input: UpdatePluginInput,
    ) -> ApiResult<InstalledPluginDto> {
        self.sync_native_plugins().await?;
        let existing = self
            .repo
            .get_db_plugin(name)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Plugin '{name}' not found")))?;

        let next_enabled = input.enabled.unwrap_or(existing.dto.enabled);
        let next_cfg = input.config.as_ref().unwrap_or(&existing.dto.config);

        let updated = self
            .repo
            .upsert_db_plugin(
                &existing.dto.name,
                &existing.dto.version,
                &existing.dto.kind,
                &existing.dto.description,
                next_enabled,
                existing.wasm_path.as_deref(),
                next_cfg,
            )
            .await?;
        Ok(updated.dto)
    }

    pub async fn uninstall_plugin(&self, name: &str) -> ApiResult<()> {
        let wasm_path = self.repo.delete_db_plugin(name).await?;
        if let Some(p) = wasm_path {
            let _ = std::fs::remove_file(p);
        }
        Ok(())
    }

    pub async fn invoke_plugin(
        &self,
        name: &str,
        input: InvokePluginInput,
    ) -> ApiResult<serde_json::Value> {
        self.sync_native_plugins().await?;
        let rec = self
            .repo
            .get_db_plugin(name)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Plugin '{name}' not found")))?;

        if !rec.dto.enabled {
            return Err(ApiError::Forbidden(format!(
                "Plugin '{name}' is currently disabled"
            )));
        }

        if rec.dto.kind == "native" {
            let native = self
                .plugins
                .get_native(name)
                .ok_or_else(|| ApiError::NotFound(format!("Native plugin '{name}' unavailable")))?;
            let out = native.invoke(&input.action, &input.payload).await?;
            return Ok(serde_json::json!({
                "plugin": name,
                "runtime": "native",
                "output": out,
            }));
        }

        let wasm_path = rec
            .wasm_path
            .ok_or_else(|| ApiError::Internal(format!("WASM plugin '{name}' missing bytecode path")))?;
        let wasm_bytes = std::fs::read(&wasm_path).map_err(|e| {
            ApiError::Internal(format!("Failed reading WASM module for '{name}': {e}"))
        })?;

        let metric: i32 = input
            .payload
            .get("input")
            .and_then(|v| v.as_i64())
            .map(|n| n.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
            .or_else(|| {
                input
                    .payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().count().min(i32::MAX as usize) as i32)
            })
            .unwrap_or(0);

        let (output, fuel_consumed) = self
            .plugins
            .wasm_host()
            .execute_sandboxed(&wasm_bytes, metric)?;

        Ok(serde_json::json!({
            "plugin": name,
            "runtime": "wasm",
            "input": metric,
            "output": output,
            "fuelConsumed": fuel_consumed,
        }))
    }

    fn wasm_plugin_dir(&self) -> PathBuf {
        Path::new(&self.config.upload_dir).join("_plugins")
    }

    fn write_wasm_artifact(&self, safe_name: &str, bytes: &[u8]) -> ApiResult<String> {
        let dir = self.wasm_plugin_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| ApiError::Internal(format!("Create WASM plugin dir failed: {e}")))?;
        let file_name = format!(
            "{}.wasm",
            safe_name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                })
                .collect::<String>()
        );
        let full_path = dir.join(file_name);
        std::fs::write(&full_path, bytes)
            .map_err(|e| ApiError::Internal(format!("Write WASM module failed: {e}")))?;
        Ok(full_path.to_string_lossy().into_owned())
    }

    // =========================================================================
    // 3. JSON-RPC 2.0 Model Context Protocol (MCP) Server
    // =========================================================================

    pub fn list_mcp_tools(&self) -> Vec<McpToolDescriptor> {
        vec![
            McpToolDescriptor {
                name: "list_content_types".into(),
                description: "List all registered collection types, single types, and components.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
            },
            McpToolDescriptor {
                name: "get_content_type_schema".into(),
                description: "Inspect full field definitions and options for a schema UID (e.g. 'api::article.article').".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "uid": { "type": "string", "description": "Content-type or component UID" }
                    },
                    "required": ["uid"]
                }),
            },
            McpToolDescriptor {
                name: "list_documents".into(),
                description: "Query documents for a content type with optional locale, status, pagination, and FTS5 search.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "uid": { "type": "string", "description": "Content-type UID (e.g. 'api::article.article')" },
                        "locale": { "type": "string", "description": "Locale code (defaults to platform default locale)" },
                        "status": { "type": "string", "enum": ["draft", "published"] },
                        "search": { "type": "string", "description": "Full-text search query" },
                        "page": { "type": "integer", "minimum": 1 },
                        "pageSize": { "type": "integer", "minimum": 1, "maximum": 100 }
                    },
                    "required": ["uid"]
                }),
            },
            McpToolDescriptor {
                name: "get_document".into(),
                description: "Fetch a single document by content-type UID and documentId.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "uid": { "type": "string" },
                        "documentId": { "type": "string" },
                        "locale": { "type": "string" },
                        "status": { "type": "string", "enum": ["draft", "published"] }
                    },
                    "required": ["uid", "documentId"]
                }),
            },
            McpToolDescriptor {
                name: "create_document".into(),
                description: "Create a new document (draft by default when draftAndPublish is enabled).".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "uid": { "type": "string" },
                        "locale": { "type": "string" },
                        "status": { "type": "string", "enum": ["draft", "published"] },
                        "data": { "type": "object" }
                    },
                    "required": ["uid", "data"]
                }),
            },
            McpToolDescriptor {
                name: "update_document".into(),
                description: "Update an existing document by UID and documentId.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "uid": { "type": "string" },
                        "documentId": { "type": "string" },
                        "locale": { "type": "string" },
                        "status": { "type": "string", "enum": ["draft", "published"] },
                        "data": { "type": "object" }
                    },
                    "required": ["uid", "documentId", "data"]
                }),
            },
            McpToolDescriptor {
                name: "publish_document".into(),
                description: "Publish an existing draft document (enforcing review workflow gates).".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "uid": { "type": "string" },
                        "documentId": { "type": "string" },
                        "locale": { "type": "string" }
                    },
                    "required": ["uid", "documentId"]
                }),
            },
            McpToolDescriptor {
                name: "list_media_files".into(),
                description: "Search and list uploaded media library assets.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "search": { "type": "string" },
                        "page": { "type": "integer", "minimum": 1 },
                        "pageSize": { "type": "integer", "minimum": 1, "maximum": 100 }
                    }
                }),
            },
        ]
    }

    /// Processes a JSON-RPC 2.0 request (single object or batch array), returning `None` for notifications.
    pub async fn handle_mcp_rpc(
        &self,
        actor: &Actor,
        payload: serde_json::Value,
    ) -> Option<serde_json::Value> {
        if let Some(arr) = payload.as_array() {
            if arr.is_empty() {
                return Some(jsonrpc_error(serde_json::Value::Null, -32600, "Invalid Request"));
            }
            let mut responses = Vec::new();
            for item in arr {
                if let Some(resp) = self.handle_single_mcp_rpc(actor, item).await {
                    responses.push(resp);
                }
            }
            return if responses.is_empty() {
                None
            } else {
                Some(serde_json::Value::Array(responses))
            };
        }
        self.handle_single_mcp_rpc(actor, &payload).await
    }

    async fn handle_single_mcp_rpc(
        &self,
        actor: &Actor,
        req: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        let Some(obj) = req.as_object() else {
            return Some(jsonrpc_error(serde_json::Value::Null, -32600, "Invalid Request"));
        };
        let id_opt = obj.get("id").cloned();
        let jsonrpc = obj.get("jsonrpc").and_then(|v| v.as_str()).unwrap_or("");
        if jsonrpc != "2.0" {
            return Some(jsonrpc_error(
                id_opt.unwrap_or(serde_json::Value::Null),
                -32600,
                "Invalid Request: jsonrpc must equal '2.0'",
            ));
        }

        let Some(method) = obj.get("method").and_then(|v| v.as_str()) else {
            return Some(jsonrpc_error(
                id_opt.unwrap_or(serde_json::Value::Null),
                -32600,
                "Invalid Request: missing method",
            ));
        };

        // JSON-RPC 2.0 notifications omit `id` and expect no response frame
        let id = id_opt?;
        let params = obj
            .get("params")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        match method {
            "initialize" => Some(jsonrpc_ok(
                id,
                serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": false }
                    },
                    "serverInfo": {
                        "name": "rapi-cms-mcp",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            )),
            "ping" => Some(jsonrpc_ok(id, serde_json::json!({}))),
            "tools/list" => Some(jsonrpc_ok(
                id,
                serde_json::json!({
                    "tools": self.list_mcp_tools()
                }),
            )),
            "tools/call" => {
                let Some(tool_name) = params.get("name").and_then(|v| v.as_str()) else {
                    return Some(jsonrpc_error(id, -32602, "Invalid params: missing tool 'name'"));
                };
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));

                let known = self.list_mcp_tools().iter().any(|t| t.name == tool_name);
                if !known {
                    return Some(jsonrpc_error(
                        id,
                        -32602,
                        &format!("Unknown MCP tool '{tool_name}'"),
                    ));
                }

                match self.execute_mcp_tool(actor, tool_name, &args).await {
                    Ok(val) => {
                        let text = serde_json::to_string_pretty(&val)
                            .unwrap_or_else(|_| val.to_string());
                        Some(jsonrpc_ok(
                            id,
                            serde_json::json!({
                                "content": [{ "type": "text", "text": text }],
                                "structuredContent": val,
                                "isError": false
                            }),
                        ))
                    }
                    Err(err) => Some(jsonrpc_ok(
                        id,
                        serde_json::json!({
                            "content": [{ "type": "text", "text": err.to_string() }],
                            "isError": true
                        }),
                    )),
                }
            }
            _ => Some(jsonrpc_error(
                id,
                -32601,
                &format!("Method not found: {method}"),
            )),
        }
    }

    async fn execute_mcp_tool(
        &self,
        actor: &Actor,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> ApiResult<serde_json::Value> {
        let strip_private = !actor.is_admin();
        let actor_id = match actor {
            Actor::AdminUser { user_id, .. } => Some(user_id.as_str()),
            _ => None,
        };

        match tool_name {
            "list_content_types" => {
                let all = self.schema.list_schemas().await?;
                let visible: Vec<_> = all
                    .into_iter()
                    .filter(|s| {
                        actor.can("plugin::content-type-builder.read", None)
                            || actor.can("plugin::content-manager.explorer.read", Some(&s.uid))
                            || actor.can(&format!("{}.find", s.uid), Some(&s.uid))
                    })
                    .map(|s| {
                        serde_json::json!({
                            "uid": s.uid,
                            "kind": s.kind,
                            "displayName": s.display_name,
                            "singularName": s.singular_name,
                            "pluralName": s.plural_name,
                            "draftAndPublish": s.draft_and_publish,
                            "i18nEnabled": s.i18n_enabled,
                            "fieldCount": s.fields.len(),
                        })
                    })
                    .collect();
                Ok(serde_json::json!({ "schemas": visible }))
            }
            "get_content_type_schema" => {
                let uid = require_str_arg(args, "uid")?;
                if !actor.can("plugin::content-type-builder.read", None)
                    && !actor.can("plugin::content-manager.explorer.read", Some(uid))
                    && !actor.can(&format!("{uid}.find"), Some(uid))
                {
                    return Err(ApiError::Forbidden(format!(
                        "Missing permission to inspect schema '{uid}'"
                    )));
                }
                let s = self.schema.get_schema(uid).await?;
                serde_json::to_value(s).map_err(|e| ApiError::Internal(e.to_string()))
            }
            "list_documents" => {
                let uid = require_str_arg(args, "uid")?;
                if !actor.can("plugin::content-manager.explorer.read", Some(uid))
                    && !actor.can(&format!("{uid}.find"), Some(uid))
                {
                    return Err(ApiError::Forbidden(format!(
                        "Missing permission to read '{uid}'"
                    )));
                }
                let schema = self.schema.get_schema(uid).await?;
                if schema.kind == ContentKind::Component {
                    return Err(ApiError::BadRequest(
                        "Components cannot be queried as standalone documents".into(),
                    ));
                }
                let loc = self
                    .governance
                    .resolve_and_validate_locale(
                        schema.i18n_enabled,
                        args.get("locale").and_then(|v| v.as_str()),
                        false,
                    )
                    .await?;
                let requested_status = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .and_then(parse_doc_status)
                    .unwrap_or(if actor.is_admin() && schema.draft_and_publish {
                        DocumentStatus::Draft
                    } else {
                        DocumentStatus::Published
                    });
                if requested_status == DocumentStatus::Draft && !actor.is_admin() {
                    return Err(ApiError::Forbidden(
                        "Draft documents require an administrator or admin token".into(),
                    ));
                }

                let page = args
                    .get("page")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1)
                    .max(1);
                let page_size = args
                    .get("pageSize")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(25)
                    .clamp(1, 100);

                let query = DocumentQuery {
                    locale: Some(loc),
                    status: Some(requested_status),
                    search_query: args.get("search").and_then(|v| v.as_str()).map(String::from),
                    page,
                    page_size,
                    populate: PopulateSpec::All,
                    ..Default::default()
                };
                let (rows, pagination) = if strip_private {
                    self.content.find_many_public(&schema, &query).await?
                } else {
                    self.content.find_many_admin_raw(&schema, &query).await?
                };
                Ok(serde_json::json!({
                    "data": rows,
                    "pagination": pagination,
                }))
            }
            "get_document" => {
                let uid = require_str_arg(args, "uid")?;
                let doc_id = require_str_arg(args, "documentId")?;
                if !actor.can("plugin::content-manager.explorer.read", Some(uid))
                    && !actor.can(&format!("{uid}.findOne"), Some(uid))
                {
                    return Err(ApiError::Forbidden(format!(
                        "Missing permission to read '{uid}'"
                    )));
                }
                let schema = self.schema.get_schema(uid).await?;
                let loc = self
                    .governance
                    .resolve_and_validate_locale(
                        schema.i18n_enabled,
                        args.get("locale").and_then(|v| v.as_str()),
                        false,
                    )
                    .await?;
                let status = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .and_then(parse_doc_status)
                    .unwrap_or(if actor.is_admin() && schema.draft_and_publish {
                        DocumentStatus::Draft
                    } else {
                        DocumentStatus::Published
                    });
                if status == DocumentStatus::Draft && !actor.is_admin() {
                    return Err(ApiError::Forbidden(
                        "Draft documents require an administrator or admin token".into(),
                    ));
                }
                self.content
                    .find_one(
                        &schema,
                        doc_id,
                        &loc,
                        status,
                        &PopulateSpec::All,
                        &[],
                        strip_private,
                    )
                    .await
            }
            "create_document" => {
                let uid = require_str_arg(args, "uid")?;
                if !actor.can("plugin::content-manager.explorer.create", Some(uid))
                    && !actor.can(&format!("{uid}.create"), Some(uid))
                {
                    return Err(ApiError::Forbidden(format!(
                        "Missing permission to create '{uid}'"
                    )));
                }
                let schema = self.schema.get_schema(uid).await?;
                let data_map = args
                    .get("data")
                    .and_then(|v| v.as_object())
                    .ok_or_else(|| ApiError::BadRequest("Argument 'data' must be a JSON object".into()))?;
                let loc = self
                    .governance
                    .resolve_and_validate_locale(
                        schema.i18n_enabled,
                        args.get("locale").and_then(|v| v.as_str()),
                        true,
                    )
                    .await?;
                let target_status = if !schema.draft_and_publish {
                    DocumentStatus::Published
                } else {
                    args.get("status")
                        .and_then(|v| v.as_str())
                        .and_then(parse_doc_status)
                        .unwrap_or(DocumentStatus::Draft)
                };
                if target_status == DocumentStatus::Published && schema.draft_and_publish {
                    if !actor.can("plugin::content-manager.explorer.publish", Some(uid))
                        && !actor.can(&format!("{uid}.create"), Some(uid))
                    {
                        return Err(ApiError::Forbidden(format!(
                            "Missing permission to publish '{uid}'"
                        )));
                    }
                    self.governance.enforce_new_entry_publish_gate().await?;
                }
                let created = self
                    .content
                    .create_entry(
                        &schema,
                        &loc,
                        target_status,
                        data_map,
                        actor_id,
                        strip_private,
                    )
                    .await?;
                self.integrations
                    .emit_webhook_event("entry.create", Some(&schema.uid), created.clone())
                    .await;
                if target_status == DocumentStatus::Published {
                    self.integrations
                        .emit_webhook_event("entry.publish", Some(&schema.uid), created.clone())
                        .await;
                }
                Ok(created)
            }
            "update_document" => {
                let uid = require_str_arg(args, "uid")?;
                let doc_id = require_str_arg(args, "documentId")?;
                if !actor.can("plugin::content-manager.explorer.update", Some(uid))
                    && !actor.can(&format!("{uid}.update"), Some(uid))
                {
                    return Err(ApiError::Forbidden(format!(
                        "Missing permission to update '{uid}'"
                    )));
                }
                let schema = self.schema.get_schema(uid).await?;
                let data_map = args
                    .get("data")
                    .and_then(|v| v.as_object())
                    .ok_or_else(|| ApiError::BadRequest("Argument 'data' must be a JSON object".into()))?;
                let loc = self
                    .governance
                    .resolve_and_validate_locale(
                        schema.i18n_enabled,
                        args.get("locale").and_then(|v| v.as_str()),
                        true,
                    )
                    .await?;
                let target_status = if !schema.draft_and_publish {
                    DocumentStatus::Published
                } else {
                    args.get("status")
                        .and_then(|v| v.as_str())
                        .and_then(parse_doc_status)
                        .unwrap_or(DocumentStatus::Draft)
                };
                if target_status == DocumentStatus::Published && schema.draft_and_publish {
                    if !actor.can("plugin::content-manager.explorer.publish", Some(uid))
                        && !actor.can(&format!("{uid}.update"), Some(uid))
                    {
                        return Err(ApiError::Forbidden(format!(
                            "Missing permission to publish '{uid}'"
                        )));
                    }
                    self.governance
                        .enforce_publish_workflow_gate(doc_id, &loc)
                        .await?;
                }
                let updated = self
                    .content
                    .update_entry(
                        &schema,
                        doc_id,
                        &loc,
                        target_status,
                        data_map,
                        actor_id,
                        strip_private,
                    )
                    .await?;
                self.integrations
                    .emit_webhook_event("entry.update", Some(&schema.uid), updated.clone())
                    .await;
                Ok(updated)
            }
            "publish_document" => {
                let uid = require_str_arg(args, "uid")?;
                let doc_id = require_str_arg(args, "documentId")?;
                if !actor.can("plugin::content-manager.explorer.publish", Some(uid))
                    && !actor.can(&format!("{uid}.update"), Some(uid))
                {
                    return Err(ApiError::Forbidden(format!(
                        "Missing permission to publish '{uid}'"
                    )));
                }
                let schema = self.schema.get_schema(uid).await?;
                let loc = self
                    .governance
                    .resolve_and_validate_locale(
                        schema.i18n_enabled,
                        args.get("locale").and_then(|v| v.as_str()),
                        true,
                    )
                    .await?;
                self.governance
                    .enforce_publish_workflow_gate(doc_id, &loc)
                    .await?;
                let published = self
                    .content
                    .publish_entry(uid, doc_id, &loc, actor_id)
                    .await?;
                self.integrations
                    .emit_webhook_event("entry.publish", Some(&schema.uid), published.clone())
                    .await;
                Ok(published)
            }
            "list_media_files" => {
                if !actor.can("plugin::upload.read", None) {
                    return Err(ApiError::Forbidden(
                        "Missing permission to read media library".into(),
                    ));
                }
                let page = args
                    .get("page")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1)
                    .max(1);
                let page_size = args
                    .get("pageSize")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(25)
                    .clamp(1, 100);
                let filter = MediaListFilter {
                    folder_id: None,
                    mime_prefix: None,
                    search_query: args.get("search").and_then(|v| v.as_str()).map(String::from),
                    page,
                    page_size,
                };
                let (files, pagination) = self.media.list_files(&filter).await?;
                Ok(serde_json::json!({
                    "files": files,
                    "pagination": pagination,
                }))
            }
            _ => Err(ApiError::NotFound(format!("Unknown MCP tool '{tool_name}'"))),
        }
    }

    // =========================================================================
    // 4. OIDC Single Sign-On (SSO)
    // =========================================================================

    pub async fn list_sso_providers(&self) -> ApiResult<Vec<SsoProviderDto>> {
        self.repo.list_sso_providers().await
    }

    pub async fn list_public_sso_providers(&self) -> ApiResult<Vec<PublicSsoProviderDto>> {
        self.repo.list_public_sso_providers().await
    }

    pub async fn create_sso_provider(
        &self,
        input: CreateSsoProviderInput,
    ) -> ApiResult<SsoProviderDto> {
        let name = input.name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(ApiError::Validation {
                message: "Provider name must be 1–80 characters".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }
        if input.client_id.trim().is_empty() || input.client_secret.trim().is_empty() {
            return Err(ApiError::Validation {
                message: "Both clientId and clientSecret are required".into(),
                details: serde_json::json!({ "field": "clientId" }),
            });
        }

        let is_prod = self.config.is_production();
        let issuer = input.issuer_url.trim().trim_end_matches('/');
        validate_webhook_url(issuer, is_prod)?;

        let auth_ep = input
            .authorization_endpoint
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let token_ep = input
            .token_endpoint
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let userinfo_ep = input
            .userinfo_endpoint
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        for ep in [auth_ep, token_ep, userinfo_ep].into_iter().flatten() {
            validate_webhook_url(ep, is_prod)?;
        }

        let scopes = {
            let s = input.scopes.trim();
            if s.is_empty() {
                "openid email profile"
            } else {
                s
            }
        };
        let role_id = {
            let r = input.default_role_id.trim();
            if r.is_empty() { "role_editor" } else { r }
        };

        self.repo
            .create_sso_provider(
                name,
                issuer,
                auth_ep,
                token_ep,
                userinfo_ep,
                input.client_id.trim(),
                input.client_secret.trim(),
                scopes,
                role_id,
                input.auto_provision,
                input.is_enabled,
            )
            .await
    }

    pub async fn update_sso_provider(
        &self,
        id: &str,
        input: UpdateSsoProviderInput,
    ) -> ApiResult<SsoProviderDto> {
        self.repo
            .update_sso_provider(
                id,
                input.is_enabled,
                input.auto_provision,
                input.default_role_id.as_deref(),
            )
            .await
    }

    pub async fn delete_sso_provider(&self, id: &str) -> ApiResult<()> {
        self.repo.delete_sso_provider(id).await
    }

    pub async fn begin_sso_authorize(
        &self,
        provider_id: &str,
        redirect_uri: &str,
    ) -> ApiResult<SsoAuthorizeUrlDto> {
        let rec = self
            .repo
            .get_sso_provider(provider_id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("SSO provider '{provider_id}' not found")))?;
        if !rec.dto.is_enabled {
            return Err(ApiError::Forbidden("SSO provider is disabled".into()));
        }

        let clean_redirect = redirect_uri.trim();
        if !(clean_redirect.starts_with("https://") || clean_redirect.starts_with("http://")) {
            return Err(ApiError::Validation {
                message: "redirectUri must be a valid HTTP(S) callback URL".into(),
                details: serde_json::json!({ "field": "redirectUri" }),
            });
        }
        let allow_any_origin =
            !self.config.is_production() && self.config.cors_origins.iter().any(|o| o == "*");
        if !allow_any_origin {
            let redirect_origin = extract_url_origin(clean_redirect).ok_or_else(|| {
                ApiError::Validation {
                    message: "Malformed redirectUri origin".into(),
                    details: serde_json::json!({ "field": "redirectUri" }),
                }
            })?;
            let public_origin = extract_url_origin(&self.config.public_url);
            let allowed = public_origin.as_deref() == Some(redirect_origin.as_str())
                || self.config.cors_origins.iter().any(|o| {
                    extract_url_origin(o).as_deref() == Some(redirect_origin.as_str())
                });
            if !allowed {
                return Err(ApiError::Validation {
                    message: format!(
                        "redirectUri origin '{redirect_origin}' is not in trusted platform origins"
                    ),
                    details: serde_json::json!({ "field": "redirectUri" }),
                });
            }
        }

        let (auth_ep, _, _) = self.resolve_oidc_endpoints(&rec.dto).await?;
        let state = random_hex_32();
        let nonce = random_hex_32();

        let state_payload = serde_json::json!({
            "providerId": provider_id,
            "redirectUri": clean_redirect,
            "nonce": nonce,
        });
        self.cache
            .set(
                &format!("rapi:oidc:state:{state}"),
                &state_payload.to_string(),
                Some(Duration::from_secs(OIDC_STATE_TTL_SECS)),
            )
            .await?;

        let sep = if auth_ep.contains('?') { '&' } else { '?' };
        let authorize_url = format!(
            "{auth_ep}{sep}response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&nonce={}",
            percent_encode(&rec.dto.client_id),
            percent_encode(clean_redirect),
            percent_encode(&rec.dto.scopes),
            percent_encode(&state),
            percent_encode(&nonce),
        );

        Ok(SsoAuthorizeUrlDto {
            authorize_url,
            state,
        })
    }

    pub async fn complete_sso_callback(
        &self,
        provider_id: &str,
        input: SsoCallbackInput,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> ApiResult<AuthTokenResponse> {
        if input.code.trim().is_empty() || input.state.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "Missing OIDC authorization code or state".into(),
            ));
        }

        // Single-use CSRF state validation
        let state_key = format!("rapi:oidc:state:{}", input.state.trim());
        let Some(raw_state) = self.cache.get(&state_key).await? else {
            return Err(ApiError::Unauthorized(
                "Invalid or expired OIDC state parameter".into(),
            ));
        };
        let _ = self.cache.delete(&state_key).await;

        let saved_state: serde_json::Value = serde_json::from_str(&raw_state)
            .map_err(|_| ApiError::Unauthorized("Corrupted OIDC state".into()))?;
        if saved_state.get("providerId").and_then(|v| v.as_str()) != Some(provider_id)
            || saved_state.get("redirectUri").and_then(|v| v.as_str())
                != Some(input.redirect_uri.trim())
        {
            return Err(ApiError::Unauthorized(
                "OIDC state provider or redirectUri mismatch".into(),
            ));
        }

        let rec = self
            .repo
            .get_sso_provider(provider_id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("SSO provider '{provider_id}' not found")))?;
        if !rec.dto.is_enabled {
            return Err(ApiError::Forbidden("SSO provider is disabled".into()));
        }

        let (_, token_ep, userinfo_ep) = self.resolve_oidc_endpoints(&rec.dto).await?;
        if self.config.is_production() {
            verify_webhook_dns_not_private(&token_ep).await?;
            verify_webhook_dns_not_private(&userinfo_ep).await?;
        }

        let form_body = format!(
            "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&client_secret={}",
            percent_encode(input.code.trim()),
            percent_encode(input.redirect_uri.trim()),
            percent_encode(&rec.dto.client_id),
            percent_encode(&rec.client_secret),
        );

        let token_resp = self
            .http_client
            .post(&token_ep)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .body(form_body)
            .send()
            .await
            .map_err(|e| ApiError::BadRequest(format!("OIDC token exchange failed: {e}")))?;

        if !token_resp.status().is_success() {
            let status = token_resp.status();
            return Err(ApiError::Unauthorized(format!(
                "OIDC provider rejected authorization code (HTTP {status})"
            )));
        }

        let token_json: serde_json::Value = token_resp
            .json()
            .await
            .map_err(|e| ApiError::BadRequest(format!("Invalid OIDC token JSON: {e}")))?;
        let Some(idp_access_token) = token_json.get("access_token").and_then(|v| v.as_str()) else {
            return Err(ApiError::Unauthorized(
                "OIDC token response missing access_token".into(),
            ));
        };

        let userinfo_resp = self
            .http_client
            .get(&userinfo_ep)
            .bearer_auth(idp_access_token)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| ApiError::BadRequest(format!("OIDC userinfo request failed: {e}")))?;

        if !userinfo_resp.status().is_success() {
            return Err(ApiError::Unauthorized(
                "OIDC userinfo endpoint rejected access token".into(),
            ));
        }

        let claims: serde_json::Value = userinfo_resp
            .json()
            .await
            .map_err(|e| ApiError::BadRequest(format!("Invalid OIDC userinfo JSON: {e}")))?;

        if let Some(verified) = claims.get("email_verified").and_then(|v| v.as_bool()) {
            if !verified {
                return Err(ApiError::Forbidden(
                    "OIDC identity email is not verified by the identity provider".into(),
                ));
            }
        }

        let email = claims
            .get("email")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                ApiError::Unauthorized("OIDC userinfo did not return an email claim".into())
            })?;

        let firstname = claims
            .get("given_name")
            .and_then(|v| v.as_str())
            .or_else(|| claims.get("name").and_then(|v| v.as_str()))
            .unwrap_or("");
        let lastname = claims
            .get("family_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        self.auth
            .complete_sso_admin_login(
                email,
                firstname,
                lastname,
                &rec.dto.default_role_id,
                rec.dto.auto_provision,
                user_agent,
                ip,
            )
            .await
    }

    async fn resolve_oidc_endpoints(
        &self,
        dto: &SsoProviderDto,
    ) -> ApiResult<(String, String, String)> {
        if let (Some(a), Some(t), Some(u)) = (
            dto.authorization_endpoint.as_ref(),
            dto.token_endpoint.as_ref(),
            dto.userinfo_endpoint.as_ref(),
        ) {
            return Ok((a.clone(), t.clone(), u.clone()));
        }

        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            dto.issuer_url.trim_end_matches('/')
        );
        validate_webhook_url(&discovery_url, self.config.is_production())?;
        if self.config.is_production() {
            verify_webhook_dns_not_private(&discovery_url).await?;
        }

        let resp = self
            .http_client
            .get(&discovery_url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| ApiError::BadRequest(format!("OIDC discovery failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(ApiError::BadRequest(format!(
                "OIDC discovery returned HTTP {}",
                resp.status()
            )));
        }
        let doc: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::BadRequest(format!("Invalid OIDC discovery JSON: {e}")))?;

        let auth_ep = dto
            .authorization_endpoint
            .clone()
            .or_else(|| {
                doc.get("authorization_endpoint")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .ok_or_else(|| {
                ApiError::BadRequest("OIDC discovery missing authorization_endpoint".into())
            })?;
        let token_ep = dto
            .token_endpoint
            .clone()
            .or_else(|| {
                doc.get("token_endpoint")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .ok_or_else(|| ApiError::BadRequest("OIDC discovery missing token_endpoint".into()))?;
        let userinfo_ep = dto
            .userinfo_endpoint
            .clone()
            .or_else(|| {
                doc.get("userinfo_endpoint")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .ok_or_else(|| {
                ApiError::BadRequest("OIDC discovery missing userinfo_endpoint".into())
            })?;

        let is_prod = self.config.is_production();
        for ep in [&auth_ep, &token_ep, &userinfo_ep] {
            validate_webhook_url(ep, is_prod)?;
        }

        Ok((auth_ep, token_ep, userinfo_ep))
    }

    // =========================================================================
    // 5. Opt-in AI Editorial Assistant
    // =========================================================================

    pub async fn get_ai_settings(&self) -> ApiResult<AiSettingsDto> {
        Ok(self.repo.get_ai_settings().await?.dto)
    }

    pub async fn update_ai_settings(
        &self,
        input: UpdateAiSettingsInput,
    ) -> ApiResult<AiSettingsDto> {
        if let Some(ref prov) = input.provider {
            if !matches!(prov.as_str(), "local-deterministic" | "openai-compatible") {
                return Err(ApiError::Validation {
                    message: "provider must be 'local-deterministic' or 'openai-compatible'".into(),
                    details: serde_json::json!({ "field": "provider" }),
                });
            }
        }
        if let Some(ref url) = input.api_url {
            if !url.trim().is_empty() {
                validate_webhook_url(url.trim(), self.config.is_production())?;
            }
        }

        let key_change = input.api_key.as_ref().map(|opt| {
            opt.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        });

        self.repo
            .update_ai_settings(
                input.is_enabled,
                input.provider.as_deref(),
                input.api_url.as_deref(),
                key_change,
                input.model.as_deref(),
            )
            .await
    }

    pub async fn complete_ai_task(
        &self,
        input: AiCompletionInput,
    ) -> ApiResult<AiCompletionOutputDto> {
        let settings = self.repo.get_ai_settings().await?;
        if !settings.dto.is_enabled {
            return Err(ApiError::Forbidden(
                "AI assistant is disabled — enable opt-in AI under Settings -> AI Assistant".into(),
            ));
        }

        let content = input.content.trim();
        if content.is_empty() {
            return Err(ApiError::Validation {
                message: "Input content is required for AI completion".into(),
                details: serde_json::json!({ "field": "content" }),
            });
        }

        let task = input.task.trim();
        if !matches!(task, "translate" | "generate_draft" | "seo_summary") {
            return Err(ApiError::Validation {
                message: "task must be 'translate', 'generate_draft', or 'seo_summary'".into(),
                details: serde_json::json!({ "field": "task" }),
            });
        }

        if settings.dto.provider == "local-deterministic" {
            let result = run_local_deterministic_ai(
                task,
                content,
                input.target_locale.as_deref(),
                input.context.as_deref(),
            );
            return Ok(AiCompletionOutputDto {
                result,
                provider: settings.dto.provider,
                model: settings.dto.model,
            });
        }

        // OpenAI-compatible HTTP provider
        let api_key = settings.api_key.ok_or_else(|| {
            ApiError::BadRequest("Configure an API key for the openai-compatible AI provider".into())
        })?;
        let base_url = settings.dto.api_url.trim_end_matches('/');
        let completions_url = format!("{base_url}/chat/completions");
        validate_webhook_url(&completions_url, self.config.is_production())?;
        if self.config.is_production() {
            verify_webhook_dns_not_private(&completions_url).await?;
        }

        let system_prompt = match task {
            "translate" => format!(
                "Translate the user's CMS content faithfully into locale '{}'. Return only translated text.",
                input.target_locale.as_deref().unwrap_or("en")
            ),
            "seo_summary" => "Generate concise SEO metadata (meta title <= 60 chars and meta description <= 155 chars) for the user's content.".to_string(),
            _ => "Draft structured, clear editorial copy based on the user's topic.".to_string(),
        };

        let resp = self
            .http_client
            .post(&completions_url)
            .bearer_auth(&api_key)
            .json(&serde_json::json!({
                "model": settings.dto.model,
                "temperature": 0.3,
                "messages": [
                    { "role": "system", "content": system_prompt },
                    { "role": "user", "content": content }
                ]
            }))
            .send()
            .await
            .map_err(|e| ApiError::BadRequest(format!("AI provider request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ApiError::BadRequest(format!(
                "AI provider returned HTTP {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::BadRequest(format!("Invalid AI provider JSON: {e}")))?;
        let message = body
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        Ok(AiCompletionOutputDto {
            result: serde_json::json!({ "text": message }),
            provider: settings.dto.provider,
            model: settings.dto.model,
        })
    }
}

// ============================================================================
// Internal Pure Helpers
// ============================================================================

fn parse_doc_status(s: &str) -> Option<DocumentStatus> {
    match s {
        "draft" => Some(DocumentStatus::Draft),
        "published" => Some(DocumentStatus::Published),
        _ => None,
    }
}

fn require_str_arg<'a>(args: &'a serde_json::Value, key: &str) -> ApiResult<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::BadRequest(format!("Missing required string argument '{key}'")))
}

fn jsonrpc_ok(id: serde_json::Value, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn jsonrpc_error(id: serde_json::Value, code: i32, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

fn extract_url_origin(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let (scheme, rest) = trimmed.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    Some(format!(
        "{}://{}",
        scheme.to_ascii_lowercase(),
        authority.to_ascii_lowercase()
    ))
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn run_local_deterministic_ai(
    task: &str,
    content: &str,
    target_locale: Option<&str>,
    context: Option<&str>,
) -> serde_json::Value {
    match task {
        "translate" => {
            let loc = target_locale.unwrap_or("fr").trim();
            serde_json::json!({
                "targetLocale": loc,
                "translatedText": format!("[{loc}] {content}"),
            })
        }
        "seo_summary" => {
            let clean: String = content.split_whitespace().collect::<Vec<_>>().join(" ");
            let meta_title: String = clean.chars().take(58).collect();
            let meta_desc: String = clean.chars().take(152).collect();
            let mut seen = HashSet::new();
            let keywords: Vec<String> = clean
                .split(|c: char| !c.is_alphanumeric())
                .map(|w| w.to_lowercase())
                .filter(|w| w.len() >= 5 && seen.insert(w.clone()))
                .take(6)
                .collect();
            let slug = meta_title
                .to_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect::<String>()
                .split('-')
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("-");
            serde_json::json!({
                "metaTitle": meta_title,
                "metaDescription": meta_desc,
                "suggestedSlug": slug,
                "keywords": keywords,
            })
        }
        _ => {
            let ctx = context.unwrap_or("editorial").trim();
            let heading = content.lines().next().unwrap_or(content).trim();
            serde_json::json!({
                "title": heading,
                "body": format!(
                    "## {heading}\n\n{content}\n\n### Key Takeaways ({ctx})\n- Structured for headless omnichannel delivery.\n- Ready for localization and editorial review."
                ),
            })
        }
    }
}
