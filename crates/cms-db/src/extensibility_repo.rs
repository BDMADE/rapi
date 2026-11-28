use crate::pool::DbPools;
use cms_core::{
    AiSettingsDto, ApiError, ApiResult, CustomFieldDto, InstalledPluginDto, PublicSsoProviderDto,
    SsoProviderDto,
};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PluginRowRecord {
    pub dto: InstalledPluginDto,
    pub wasm_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SsoProviderRecord {
    pub dto: SsoProviderDto,
    pub client_secret: String,
}

#[derive(Debug, Clone)]
pub struct AiSettingsRecord {
    pub dto: AiSettingsDto,
    pub api_key: Option<String>,
}

#[derive(Clone)]
pub struct ExtensibilityRepository {
    pools: DbPools,
}

impl ExtensibilityRepository {
    pub fn new(pools: DbPools) -> Self {
        Self { pools }
    }

    // =========================================================================
    // Custom Fields (`custom_fields`)
    // =========================================================================

    pub async fn list_custom_fields(&self) -> ApiResult<Vec<CustomFieldDto>> {
        let rows = sqlx::query(
            "SELECT uid, plugin_name, name, underlying_type, options_json FROM custom_fields ORDER BY uid ASC",
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_custom_fields failed: {e}")))?;

        Ok(rows.into_iter().map(map_custom_field_row).collect())
    }

    pub async fn get_custom_field(&self, uid: &str) -> ApiResult<Option<CustomFieldDto>> {
        let row = sqlx::query(
            "SELECT uid, plugin_name, name, underlying_type, options_json FROM custom_fields WHERE uid = ? LIMIT 1",
        )
        .bind(uid)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get_custom_field failed: {e}")))?;
        Ok(row.map(map_custom_field_row))
    }

    pub async fn upsert_custom_field(
        &self,
        uid: &str,
        plugin_name: Option<&str>,
        name: &str,
        underlying_type: &str,
        options: &serde_json::Value,
    ) -> ApiResult<CustomFieldDto> {
        let opts_str = serde_json::to_string(options).unwrap_or_else(|_| "{}".to_string());
        sqlx::query(
            r#"
            INSERT INTO custom_fields (uid, plugin_name, name, underlying_type, options_json)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(uid) DO UPDATE SET
                plugin_name = excluded.plugin_name,
                name = excluded.name,
                underlying_type = excluded.underlying_type,
                options_json = excluded.options_json
            "#,
        )
        .bind(uid)
        .bind(plugin_name)
        .bind(name)
        .bind(underlying_type)
        .bind(&opts_str)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("upsert_custom_field failed: {e}")))?;

        self.get_custom_field(uid)
            .await?
            .ok_or_else(|| ApiError::Internal("Saved custom field not found".into()))
    }

    pub async fn delete_custom_field(&self, uid: &str) -> ApiResult<()> {
        let in_use: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM field_definitions WHERE custom_field_uid = ?",
        )
        .bind(uid)
        .fetch_one(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("check custom field usage failed: {e}")))?;
        if in_use > 0 {
            return Err(ApiError::Conflict(format!(
                "Custom field '{uid}' is referenced by {in_use} schema field(s)"
            )));
        }

        let res = sqlx::query("DELETE FROM custom_fields WHERE uid = ?")
            .bind(uid)
            .execute(&self.pools.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("delete_custom_field failed: {e}")))?;
        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("Custom field '{uid}' not found")));
        }
        Ok(())
    }

    // =========================================================================
    // Plugin Registry (`plugin_registry`)
    // =========================================================================

    pub async fn list_db_plugins(&self) -> ApiResult<Vec<PluginRowRecord>> {
        let rows = sqlx::query(
            "SELECT name, version, kind, description, enabled, wasm_path, config_json, installed_at FROM plugin_registry ORDER BY name ASC",
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_db_plugins failed: {e}")))?;

        Ok(rows.into_iter().map(map_plugin_row).collect())
    }

    pub async fn get_db_plugin(&self, name: &str) -> ApiResult<Option<PluginRowRecord>> {
        let row = sqlx::query(
            "SELECT name, version, kind, description, enabled, wasm_path, config_json, installed_at FROM plugin_registry WHERE name = ? LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get_db_plugin failed: {e}")))?;
        Ok(row.map(map_plugin_row))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_db_plugin(
        &self,
        name: &str,
        version: &str,
        kind: &str,
        description: &str,
        enabled: bool,
        wasm_path: Option<&str>,
        config: &serde_json::Value,
    ) -> ApiResult<PluginRowRecord> {
        let cfg_str = serde_json::to_string(config).unwrap_or_else(|_| "{}".to_string());
        sqlx::query(
            r#"
            INSERT INTO plugin_registry (name, version, kind, description, enabled, wasm_path, config_json)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(name) DO UPDATE SET
                version = excluded.version,
                description = excluded.description,
                enabled = excluded.enabled,
                wasm_path = COALESCE(excluded.wasm_path, plugin_registry.wasm_path),
                config_json = excluded.config_json
            "#,
        )
        .bind(name)
        .bind(version)
        .bind(kind)
        .bind(description)
        .bind(if enabled { 1 } else { 0 })
        .bind(wasm_path)
        .bind(&cfg_str)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("upsert_db_plugin failed: {e}")))?;

        self.get_db_plugin(name)
            .await?
            .ok_or_else(|| ApiError::Internal("Saved plugin not found".into()))
    }

    pub async fn delete_db_plugin(&self, name: &str) -> ApiResult<Option<String>> {
        let existing = self
            .get_db_plugin(name)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Plugin '{name}' not found")))?;
        if existing.dto.kind == "native" {
            return Err(ApiError::BadRequest(
                "Compile-time native plugins cannot be uninstalled (disable them instead)".into(),
            ));
        }

        sqlx::query("DELETE FROM plugin_registry WHERE name = ?")
            .bind(name)
            .execute(&self.pools.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("delete_db_plugin failed: {e}")))?;
        Ok(existing.wasm_path)
    }

    // =========================================================================
    // OIDC SSO Providers (`sso_providers`)
    // =========================================================================

    pub async fn list_sso_providers(&self) -> ApiResult<Vec<SsoProviderDto>> {
        let rows = sqlx::query(
            "SELECT * FROM sso_providers ORDER BY created_at ASC",
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_sso_providers failed: {e}")))?;
        Ok(rows.into_iter().map(|r| map_sso_row(r).dto).collect())
    }

    pub async fn list_public_sso_providers(&self) -> ApiResult<Vec<PublicSsoProviderDto>> {
        let rows = sqlx::query(
            "SELECT id, name FROM sso_providers WHERE is_enabled = 1 ORDER BY name ASC",
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list_public_sso_providers failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| PublicSsoProviderDto {
                id: r.get("id"),
                name: r.get("name"),
            })
            .collect())
    }

    pub async fn get_sso_provider(&self, id: &str) -> ApiResult<Option<SsoProviderRecord>> {
        let row = sqlx::query("SELECT * FROM sso_providers WHERE id = ? LIMIT 1")
            .bind(id)
            .fetch_optional(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("get_sso_provider failed: {e}")))?;
        Ok(row.map(map_sso_row))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_sso_provider(
        &self,
        name: &str,
        issuer_url: &str,
        authorization_endpoint: Option<&str>,
        token_endpoint: Option<&str>,
        userinfo_endpoint: Option<&str>,
        client_id: &str,
        client_secret: &str,
        scopes: &str,
        default_role_id: &str,
        auto_provision: bool,
        is_enabled: bool,
    ) -> ApiResult<SsoProviderDto> {
        let id = format!("sso_{}", Uuid::now_v7().simple());
        sqlx::query(
            r#"
            INSERT INTO sso_providers (
                id, name, issuer_url, authorization_endpoint, token_endpoint, userinfo_endpoint,
                client_id, client_secret, scopes, default_role_id, auto_provision, is_enabled
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(name)
        .bind(issuer_url)
        .bind(authorization_endpoint)
        .bind(token_endpoint)
        .bind(userinfo_endpoint)
        .bind(client_id)
        .bind(client_secret)
        .bind(scopes)
        .bind(default_role_id)
        .bind(if auto_provision { 1 } else { 0 })
        .bind(if is_enabled { 1 } else { 0 })
        .execute(&self.pools.writer)
        .await
        .map_err(|e| {
            if e.to_string().contains("FOREIGN KEY") {
                ApiError::BadRequest(format!("Default admin role '{default_role_id}' not found"))
            } else {
                ApiError::Internal(format!("create_sso_provider failed: {e}"))
            }
        })?;

        self.get_sso_provider(&id)
            .await?
            .map(|r| r.dto)
            .ok_or_else(|| ApiError::Internal("Created SSO provider not found".into()))
    }

    pub async fn update_sso_provider(
        &self,
        id: &str,
        is_enabled: Option<bool>,
        auto_provision: Option<bool>,
        default_role_id: Option<&str>,
    ) -> ApiResult<SsoProviderDto> {
        let existing = self
            .get_sso_provider(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("SSO provider '{id}' not found")))?;
        let next_enabled = is_enabled.unwrap_or(existing.dto.is_enabled);
        let next_auto = auto_provision.unwrap_or(existing.dto.auto_provision);
        let next_role = default_role_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.dto.default_role_id);

        sqlx::query(
            r#"
            UPDATE sso_providers
            SET is_enabled = ?, auto_provision = ?, default_role_id = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?
            "#,
        )
        .bind(if next_enabled { 1 } else { 0 })
        .bind(if next_auto { 1 } else { 0 })
        .bind(next_role)
        .bind(id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| {
            if e.to_string().contains("FOREIGN KEY") {
                ApiError::BadRequest(format!("Default admin role '{next_role}' not found"))
            } else {
                ApiError::Internal(format!("update_sso_provider failed: {e}"))
            }
        })?;

        self.get_sso_provider(id)
            .await?
            .map(|r| r.dto)
            .ok_or_else(|| ApiError::Internal("Updated SSO provider not found".into()))
    }

    pub async fn delete_sso_provider(&self, id: &str) -> ApiResult<()> {
        let res = sqlx::query("DELETE FROM sso_providers WHERE id = ?")
            .bind(id)
            .execute(&self.pools.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("delete_sso_provider failed: {e}")))?;
        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("SSO provider '{id}' not found")));
        }
        Ok(())
    }

    // =========================================================================
    // Opt-in AI Settings (`ai_settings`)
    // =========================================================================

    pub async fn get_ai_settings(&self) -> ApiResult<AiSettingsRecord> {
        let row = sqlx::query("SELECT * FROM ai_settings WHERE id = 1 LIMIT 1")
            .fetch_one(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("get_ai_settings failed: {e}")))?;

        let api_key: Option<String> = row
            .get::<Option<String>, _>("api_key")
            .filter(|s| !s.is_empty());

        Ok(AiSettingsRecord {
            dto: AiSettingsDto {
                is_enabled: row.get::<i64, _>("is_enabled") != 0,
                provider: row.get("provider"),
                api_url: row.get("api_url"),
                has_api_key: api_key.is_some(),
                model: row.get("model"),
                updated_at: row.get("updated_at"),
            },
            api_key,
        })
    }

    pub async fn update_ai_settings(
        &self,
        is_enabled: Option<bool>,
        provider: Option<&str>,
        api_url: Option<&str>,
        api_key: Option<Option<&str>>,
        model: Option<&str>,
    ) -> ApiResult<AiSettingsDto> {
        let existing = self.get_ai_settings().await?;
        let next_enabled = is_enabled.unwrap_or(existing.dto.is_enabled);
        let next_provider = provider
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.dto.provider);
        let next_url = api_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.dto.api_url);
        let next_key = match api_key {
            Some(v) => v.map(String::from),
            None => existing.api_key,
        };
        let next_model = model
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.dto.model);

        sqlx::query(
            r#"
            UPDATE ai_settings
            SET is_enabled = ?, provider = ?, api_url = ?, api_key = ?, model = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = 1
            "#,
        )
        .bind(if next_enabled { 1 } else { 0 })
        .bind(next_provider)
        .bind(next_url)
        .bind(&next_key)
        .bind(next_model)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("update_ai_settings failed: {e}")))?;

        Ok(self.get_ai_settings().await?.dto)
    }
}

fn map_custom_field_row(r: sqlx::sqlite::SqliteRow) -> CustomFieldDto {
    let raw_opts: String = r.get("options_json");
    let options = serde_json::from_str(&raw_opts).unwrap_or_else(|_| serde_json::json!({}));
    CustomFieldDto {
        uid: r.get("uid"),
        plugin_name: r.get("plugin_name"),
        name: r.get("name"),
        underlying_type: r.get("underlying_type"),
        options,
    }
}

fn map_plugin_row(r: sqlx::sqlite::SqliteRow) -> PluginRowRecord {
    let raw_cfg: String = r.get("config_json");
    let config = serde_json::from_str(&raw_cfg).unwrap_or_else(|_| serde_json::json!({}));
    PluginRowRecord {
        dto: InstalledPluginDto {
            name: r.get("name"),
            version: r.get("version"),
            kind: r.get("kind"),
            description: r.get("description"),
            enabled: r.get::<i64, _>("enabled") != 0,
            config,
            installed_at: r.get("installed_at"),
        },
        wasm_path: r.get("wasm_path"),
    }
}

fn map_sso_row(r: sqlx::sqlite::SqliteRow) -> SsoProviderRecord {
    let secret: String = r.get("client_secret");
    SsoProviderRecord {
        dto: SsoProviderDto {
            id: r.get("id"),
            name: r.get("name"),
            issuer_url: r.get("issuer_url"),
            authorization_endpoint: r.get("authorization_endpoint"),
            token_endpoint: r.get("token_endpoint"),
            userinfo_endpoint: r.get("userinfo_endpoint"),
            client_id: r.get("client_id"),
            has_client_secret: !secret.is_empty(),
            scopes: r.get("scopes"),
            default_role_id: r.get("default_role_id"),
            auto_provision: r.get::<i64, _>("auto_provision") != 0,
            is_enabled: r.get::<i64, _>("is_enabled") != 0,
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        },
        client_secret: secret,
    }
}
