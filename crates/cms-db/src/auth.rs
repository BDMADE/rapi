use crate::pool::DbPools;
use cms_core::{
    AdminRoleDto, AdminRoleSummary, AdminUserDto, ApiError, ApiResult, ApiTokenDto, PermissionRule,
    TokenType,
};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AdminUserRecord {
    pub id: String,
    pub email: String,
    pub username: Option<String>,
    pub password_hash: String,
    pub firstname: String,
    pub lastname: String,
    pub is_active: bool,
    pub is_super_admin: bool,
    pub last_login_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: String,
    pub user_type: String,
    pub user_id: String,
    pub expires_at: String,
}

#[derive(Clone)]
pub struct AuthRepository {
    pools: DbPools,
}

impl AuthRepository {
    pub fn new(pools: DbPools) -> Self {
        Self { pools }
    }

    pub async fn count_admin_users(&self) -> ApiResult<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM admin_users")
            .fetch_one(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("count admin_users failed: {e}")))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_admin_user(
        &self,
        email: &str,
        username: Option<&str>,
        password_hash: &str,
        firstname: &str,
        lastname: &str,
        is_active: bool,
        is_super_admin: bool,
        role_ids: &[String],
    ) -> ApiResult<AdminUserDto> {
        let id = Uuid::now_v7().to_string();
        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin tx failed: {e}")))?;

        let res = sqlx::query(
            r#"
            INSERT INTO admin_users (id, email, username, password_hash, firstname, lastname, is_active, is_super_admin)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(email.to_lowercase())
        .bind(username)
        .bind(password_hash)
        .bind(firstname)
        .bind(lastname)
        .bind(if is_active { 1 } else { 0 })
        .bind(if is_super_admin { 1 } else { 0 })
        .execute(&mut *tx)
        .await;

        if let Err(e) = res {
            let msg = e.to_string();
            if msg.contains("UNIQUE") {
                return Err(ApiError::Conflict(
                    "An administrator with that email or username already exists".to_string(),
                ));
            }
            return Err(ApiError::Internal(format!("insert admin_user failed: {e}")));
        }

        for role_id in role_ids {
            sqlx::query("INSERT OR IGNORE INTO admin_user_roles (user_id, role_id) VALUES (?, ?)")
                .bind(&id)
                .bind(role_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("assign role failed: {e}")))?;
        }

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit admin_user tx failed: {e}")))?;

        self.find_admin_dto_by_id(&id)
            .await?
            .ok_or_else(|| ApiError::Internal("Created admin user not found".to_string()))
    }

    pub async fn find_admin_record_by_email(&self, email: &str) -> ApiResult<Option<AdminUserRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, email, username, password_hash, firstname, lastname, is_active, is_super_admin, last_login_at, created_at, updated_at
            FROM admin_users
            WHERE lower(email) = lower(?)
            LIMIT 1
            "#,
        )
        .bind(email)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("find admin by email failed: {e}")))?;

        Ok(row.map(map_admin_record))
    }

    pub async fn find_admin_record_by_id(&self, id: &str) -> ApiResult<Option<AdminUserRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, email, username, password_hash, firstname, lastname, is_active, is_super_admin, last_login_at, created_at, updated_at
            FROM admin_users
            WHERE id = ?
            LIMIT 1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("find admin by id failed: {e}")))?;

        Ok(row.map(map_admin_record))
    }

    pub async fn find_admin_dto_by_id(&self, id: &str) -> ApiResult<Option<AdminUserDto>> {
        let Some(rec) = self.find_admin_record_by_id(id).await? else {
            return Ok(None);
        };
        let roles = self.get_user_roles(&rec.id).await?;
        Ok(Some(record_to_dto(rec, roles)))
    }

    pub async fn list_admin_users(&self) -> ApiResult<Vec<AdminUserDto>> {
        let rows = sqlx::query(
            r#"
            SELECT id, email, username, password_hash, firstname, lastname, is_active, is_super_admin, last_login_at, created_at, updated_at
            FROM admin_users
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list admin users failed: {e}")))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let rec = map_admin_record(row);
            let roles = self.get_user_roles(&rec.id).await?;
            out.push(record_to_dto(rec, roles));
        }
        Ok(out)
    }

    pub async fn update_admin_user(
        &self,
        id: &str,
        firstname: Option<&str>,
        lastname: Option<&str>,
        is_active: Option<bool>,
        password_hash: Option<&str>,
        role_ids: Option<&[String]>,
    ) -> ApiResult<AdminUserDto> {
        let Some(existing) = self.find_admin_record_by_id(id).await? else {
            return Err(ApiError::NotFound(format!("Admin user '{id}' not found")));
        };

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin tx failed: {e}")))?;

        let new_first = firstname.unwrap_or(&existing.firstname);
        let new_last = lastname.unwrap_or(&existing.lastname);
        let new_active = is_active.unwrap_or(existing.is_active);
        let new_pw = password_hash.unwrap_or(&existing.password_hash);

        sqlx::query(
            r#"
            UPDATE admin_users
            SET firstname = ?, lastname = ?, is_active = ?, password_hash = ?,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?
            "#,
        )
        .bind(new_first)
        .bind(new_last)
        .bind(if new_active { 1 } else { 0 })
        .bind(new_pw)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("update admin user failed: {e}")))?;

        if let Some(roles) = role_ids {
            sqlx::query("DELETE FROM admin_user_roles WHERE user_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("clear user roles failed: {e}")))?;
            for role_id in roles {
                sqlx::query("INSERT OR IGNORE INTO admin_user_roles (user_id, role_id) VALUES (?, ?)")
                    .bind(id)
                    .bind(role_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| ApiError::Internal(format!("assign role failed: {e}")))?;
            }
            let is_super = roles.iter().any(|r| r == "role_super_admin");
            sqlx::query("UPDATE admin_users SET is_super_admin = ? WHERE id = ?")
                .bind(if is_super { 1 } else { 0 })
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("update super_admin flag failed: {e}")))?;
        }

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit update_admin_user failed: {e}")))?;

        self.find_admin_dto_by_id(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Admin user '{id}' not found")))
    }

    pub async fn delete_admin_user(&self, id: &str) -> ApiResult<()> {
        let res = sqlx::query("DELETE FROM admin_users WHERE id = ?")
            .bind(id)
            .execute(&self.pools.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("delete admin user failed: {e}")))?;
        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("Admin user '{id}' not found")));
        }
        Ok(())
    }

    pub async fn touch_last_login(&self, user_id: &str) -> ApiResult<()> {
        sqlx::query(
            "UPDATE admin_users SET last_login_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(user_id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("touch last_login_at failed: {e}")))?;
        Ok(())
    }

    pub async fn get_user_roles(&self, user_id: &str) -> ApiResult<Vec<AdminRoleSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT r.id, r.code, r.name
            FROM admin_roles r
            INNER JOIN admin_user_roles ur ON ur.role_id = r.id
            WHERE ur.user_id = ?
            ORDER BY r.name ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get user roles failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| AdminRoleSummary {
                id: r.get("id"),
                code: r.get("code"),
                name: r.get("name"),
            })
            .collect())
    }

    pub async fn create_session(
        &self,
        user_type: &str,
        user_id: &str,
        refresh_token_hash: &str,
        expires_at: &str,
        user_agent: Option<&str>,
        ip_address: Option<&str>,
    ) -> ApiResult<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            r#"
            INSERT INTO sessions (id, user_type, user_id, refresh_token_hash, user_agent, ip_address, expires_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(user_type)
        .bind(user_id)
        .bind(refresh_token_hash)
        .bind(user_agent)
        .bind(ip_address)
        .bind(expires_at)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("create session failed: {e}")))?;
        Ok(id)
    }

    pub async fn find_session_by_hash(&self, hash: &str) -> ApiResult<Option<SessionRecord>> {
        let row = sqlx::query(
            "SELECT id, user_type, user_id, expires_at FROM sessions WHERE refresh_token_hash = ? LIMIT 1",
        )
        .bind(hash)
        .fetch_optional(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("find session failed: {e}")))?;

        Ok(row.map(|r| SessionRecord {
            id: r.get("id"),
            user_type: r.get("user_type"),
            user_id: r.get("user_id"),
            expires_at: r.get("expires_at"),
        }))
    }

    pub async fn delete_session_by_hash(&self, hash: &str) -> ApiResult<()> {
        sqlx::query("DELETE FROM sessions WHERE refresh_token_hash = ?")
            .bind(hash)
            .execute(&self.pools.writer)
            .await
            .map_err(|e| ApiError::Internal(format!("delete session failed: {e}")))?;
        Ok(())
    }

    // --- Roles & Permissions ---

    pub async fn list_roles(&self) -> ApiResult<Vec<AdminRoleDto>> {
        let rows = sqlx::query(
            r#"
            SELECT r.id, r.code, r.name, r.description, r.created_at, r.updated_at,
                   (SELECT COUNT(*) FROM admin_user_roles ur WHERE ur.role_id = r.id) AS user_count
            FROM admin_roles r
            ORDER BY r.created_at ASC
            "#,
        )
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("list roles failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| AdminRoleDto {
                id: r.get("id"),
                code: r.get("code"),
                name: r.get("name"),
                description: r.get("description"),
                user_count: r.get("user_count"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn create_role(&self, name: &str, description: &str) -> ApiResult<AdminRoleDto> {
        let id = format!("role_{}", Uuid::now_v7().simple());
        let code = slugify_role_code(name);
        let res = sqlx::query(
            "INSERT INTO admin_roles (id, code, name, description) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&code)
        .bind(name)
        .bind(description)
        .execute(&self.pools.writer)
        .await;

        if let Err(e) = res {
            if e.to_string().contains("UNIQUE") {
                return Err(ApiError::Conflict(format!(
                    "Role with code '{code}' already exists"
                )));
            }
            return Err(ApiError::Internal(format!("create role failed: {e}")));
        }

        self.list_roles()
            .await?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| ApiError::Internal("Created role not found".to_string()))
    }

    pub async fn update_role(
        &self,
        id: &str,
        name: &str,
        description: &str,
    ) -> ApiResult<AdminRoleDto> {
        let res = sqlx::query(
            "UPDATE admin_roles SET name = ?, description = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
        )
        .bind(name)
        .bind(description)
        .bind(id)
        .execute(&self.pools.writer)
        .await
        .map_err(|e| ApiError::Internal(format!("update role failed: {e}")))?;

        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("Role '{id}' not found")));
        }

        self.list_roles()
            .await?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| ApiError::NotFound(format!("Role '{id}' not found")))
    }

    pub async fn delete_role(&self, id: &str) -> ApiResult<()> {
        if id == "role_super_admin" || id == "role_editor" || id == "role_author" {
            return Err(ApiError::BadRequest(
                "Built-in system roles cannot be deleted".to_string(),
            ));
        }
        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin tx failed: {e}")))?;

        sqlx::query("DELETE FROM permissions WHERE subject_type = 'admin_role' AND subject_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete role permissions failed: {e}")))?;

        let res = sqlx::query("DELETE FROM admin_roles WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete role failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit delete_role failed: {e}")))?;

        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("Role '{id}' not found")));
        }
        Ok(())
    }

    pub async fn get_subject_permissions(
        &self,
        subject_type: &str,
        subject_id: &str,
    ) -> ApiResult<Vec<PermissionRule>> {
        let rows = sqlx::query(
            "SELECT action, target_uid, properties_json FROM permissions WHERE subject_type = ? AND subject_id = ?",
        )
        .bind(subject_type)
        .bind(subject_id)
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get permissions failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let props_raw: String = r.get("properties_json");
                let props: serde_json::Value =
                    serde_json::from_str(&props_raw).unwrap_or_else(|_| serde_json::json!({}));
                let fields = props
                    .get("fields")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                PermissionRule {
                    action: r.get("action"),
                    target_uid: r.get("target_uid"),
                    fields,
                }
            })
            .collect())
    }

    pub async fn get_user_effective_permissions(&self, user_id: &str) -> ApiResult<Vec<PermissionRule>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT p.action, p.target_uid, p.properties_json
            FROM permissions p
            INNER JOIN admin_user_roles ur ON ur.role_id = p.subject_id
            WHERE p.subject_type = 'admin_role' AND ur.user_id = ?
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pools.reader)
        .await
        .map_err(|e| ApiError::Internal(format!("get user permissions failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let props_raw: String = r.get("properties_json");
                let props: serde_json::Value =
                    serde_json::from_str(&props_raw).unwrap_or_else(|_| serde_json::json!({}));
                let fields = props
                    .get("fields")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                PermissionRule {
                    action: r.get("action"),
                    target_uid: r.get("target_uid"),
                    fields,
                }
            })
            .collect())
    }

    pub async fn replace_subject_permissions(
        &self,
        subject_type: &str,
        subject_id: &str,
        rules: &[PermissionRule],
    ) -> ApiResult<Vec<PermissionRule>> {
        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin tx failed: {e}")))?;

        sqlx::query("DELETE FROM permissions WHERE subject_type = ? AND subject_id = ?")
            .bind(subject_type)
            .bind(subject_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("clear permissions failed: {e}")))?;

        for rule in rules {
            let pid = Uuid::now_v7().to_string();
            let props_json = serde_json::json!({ "fields": rule.fields }).to_string();
            sqlx::query(
                r#"
                INSERT INTO permissions (id, subject_type, subject_id, action, target_uid, properties_json)
                VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(pid)
            .bind(subject_type)
            .bind(subject_id)
            .bind(&rule.action)
            .bind(&rule.target_uid)
            .bind(props_json)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("insert permission failed: {e}")))?;
        }

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit permissions tx failed: {e}")))?;

        self.get_subject_permissions(subject_type, subject_id).await
    }

    // --- API Tokens & Admin Tokens ---

    #[allow(clippy::too_many_arguments)]
    pub async fn create_token(
        &self,
        is_admin_token: bool,
        name: &str,
        description: &str,
        token_type: TokenType,
        access_key_hash: &str,
        admin_user_id: Option<&str>,
        lifespan_ms: Option<i64>,
        expires_at: Option<&str>,
        permissions: &[PermissionRule],
    ) -> ApiResult<ApiTokenDto> {
        let id = Uuid::now_v7().to_string();
        let subject_type = if is_admin_token { "admin_token" } else { "api_token" };

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin tx failed: {e}")))?;

        let res = if is_admin_token {
            sqlx::query(
                r#"
                INSERT INTO admin_tokens (id, name, description, token_type, access_key_hash, admin_user_id, lifespan_ms, expires_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(name)
            .bind(description)
            .bind(token_type.as_str())
            .bind(access_key_hash)
            .bind(admin_user_id)
            .bind(lifespan_ms)
            .bind(expires_at)
            .execute(&mut *tx)
            .await
        } else {
            sqlx::query(
                r#"
                INSERT INTO api_tokens (id, name, description, token_type, access_key_hash, lifespan_ms, expires_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(name)
            .bind(description)
            .bind(token_type.as_str())
            .bind(access_key_hash)
            .bind(lifespan_ms)
            .bind(expires_at)
            .execute(&mut *tx)
            .await
        };

        if let Err(e) = res {
            if e.to_string().contains("UNIQUE") {
                return Err(ApiError::Conflict(format!(
                    "A token named '{name}' already exists"
                )));
            }
            return Err(ApiError::Internal(format!("insert token failed: {e}")));
        }

        if token_type == TokenType::Custom {
            for rule in permissions {
                let pid = Uuid::now_v7().to_string();
                let props_json = serde_json::json!({ "fields": rule.fields }).to_string();
                sqlx::query(
                    "INSERT INTO permissions (id, subject_type, subject_id, action, target_uid, properties_json) VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(pid)
                .bind(subject_type)
                .bind(&id)
                .bind(&rule.action)
                .bind(&rule.target_uid)
                .bind(props_json)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiError::Internal(format!("insert token permission failed: {e}")))?;
            }
        }

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit token tx failed: {e}")))?;

        self.list_tokens(is_admin_token)
            .await?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| ApiError::Internal("Created token not found".to_string()))
    }

    pub async fn list_tokens(&self, is_admin_token: bool) -> ApiResult<Vec<ApiTokenDto>> {
        let table = if is_admin_token { "admin_tokens" } else { "api_tokens" };
        let subject_type = if is_admin_token { "admin_token" } else { "api_token" };
        let sql = format!(
            "SELECT id, name, description, token_type, lifespan_ms, expires_at, last_used_at, created_at, updated_at FROM {table} ORDER BY created_at DESC"
        );
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("list tokens failed: {e}")))?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let id: String = r.get("id");
            let ttype_str: String = r.get("token_type");
            let token_type = TokenType::parse_str(&ttype_str).unwrap_or(TokenType::ReadOnly);
            let permissions = if token_type == TokenType::Custom {
                self.get_subject_permissions(subject_type, &id).await?
            } else {
                vec![]
            };
            out.push(ApiTokenDto {
                id,
                name: r.get("name"),
                description: r.get("description"),
                token_type,
                lifespan_ms: r.get("lifespan_ms"),
                expires_at: r.get("expires_at"),
                last_used_at: r.get("last_used_at"),
                permissions,
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            });
        }
        Ok(out)
    }

    pub async fn find_token_by_hash(
        &self,
        is_admin_token: bool,
        access_key_hash: &str,
    ) -> ApiResult<Option<ApiTokenDto>> {
        let table = if is_admin_token { "admin_tokens" } else { "api_tokens" };
        let subject_type = if is_admin_token { "admin_token" } else { "api_token" };
        let sql = format!(
            "SELECT id, name, description, token_type, lifespan_ms, expires_at, last_used_at, created_at, updated_at FROM {table} WHERE access_key_hash = ? LIMIT 1"
        );
        let row = sqlx::query(&sql)
            .bind(access_key_hash)
            .fetch_optional(&self.pools.reader)
            .await
            .map_err(|e| ApiError::Internal(format!("find token by hash failed: {e}")))?;

        let Some(r) = row else {
            return Ok(None);
        };

        let id: String = r.get("id");
        let ttype_str: String = r.get("token_type");
        let token_type = TokenType::parse_str(&ttype_str).unwrap_or(TokenType::ReadOnly);
        let permissions = if token_type == TokenType::Custom {
            self.get_subject_permissions(subject_type, &id).await?
        } else {
            vec![]
        };

        Ok(Some(ApiTokenDto {
            id,
            name: r.get("name"),
            description: r.get("description"),
            token_type,
            lifespan_ms: r.get("lifespan_ms"),
            expires_at: r.get("expires_at"),
            last_used_at: r.get("last_used_at"),
            permissions,
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    pub async fn delete_token(&self, is_admin_token: bool, id: &str) -> ApiResult<()> {
        let table = if is_admin_token { "admin_tokens" } else { "api_tokens" };
        let subject_type = if is_admin_token { "admin_token" } else { "api_token" };

        let mut tx = self
            .pools
            .writer
            .begin()
            .await
            .map_err(|e| ApiError::Internal(format!("begin tx failed: {e}")))?;

        sqlx::query("DELETE FROM permissions WHERE subject_type = ? AND subject_id = ?")
            .bind(subject_type)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete token permissions failed: {e}")))?;

        let sql = format!("DELETE FROM {table} WHERE id = ?");
        let res = sqlx::query(&sql)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiError::Internal(format!("delete token failed: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| ApiError::Internal(format!("commit delete token failed: {e}")))?;

        if res.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("Token '{id}' not found")));
        }
        Ok(())
    }
}

fn map_admin_record(row: sqlx::sqlite::SqliteRow) -> AdminUserRecord {
    AdminUserRecord {
        id: row.get("id"),
        email: row.get("email"),
        username: row.get("username"),
        password_hash: row.get("password_hash"),
        firstname: row.get("firstname"),
        lastname: row.get("lastname"),
        is_active: row.get::<i64, _>("is_active") != 0,
        is_super_admin: row.get::<i64, _>("is_super_admin") != 0,
        last_login_at: row.get("last_login_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn record_to_dto(rec: AdminUserRecord, roles: Vec<AdminRoleSummary>) -> AdminUserDto {
    AdminUserDto {
        id: rec.id,
        email: rec.email,
        username: rec.username,
        firstname: rec.firstname,
        lastname: rec.lastname,
        is_active: rec.is_active,
        is_super_admin: rec.is_super_admin,
        roles,
        last_login_at: rec.last_login_at,
        created_at: rec.created_at,
        updated_at: rec.updated_at,
    }
}

fn slugify_role_code(name: &str) -> String {
    let base: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = base.trim_matches('-');
    if trimmed.is_empty() {
        format!("custom-{}", &Uuid::now_v7().simple().to_string()[..8])
    } else {
        format!("rapi-{trimmed}")
    }
}
