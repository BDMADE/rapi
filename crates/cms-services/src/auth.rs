use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::{Duration as ChronoDuration, Utc};
use cms_core::{
    Actor, AdminRoleDto, AdminUserDto, ApiError, ApiResult, ApiTokenDto, AppConfig,
    AuthTokenResponse, CacheBackend, CreatedTokenDto, InvalidationEvent, PermissionRule,
    PubSubBackend, TokenType,
};
use cms_db::AuthRepository;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;

const ACCESS_TOKEN_TTL_SECS: i64 = 15 * 60; // 15 minutes
const REFRESH_TOKEN_TTL_DAYS: i64 = 30;
const TOKEN_CACHE_TTL_SECS: u64 = 60;

#[derive(Debug, Serialize, Deserialize)]
struct AccessClaims {
    sub: String,
    email: String,
    typ: String,
    iat: usize,
    exp: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterAdminInput {
    pub email: String,
    pub password: String,
    pub firstname: String,
    #[serde(default)]
    pub lastname: String,
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAdminUserInput {
    pub email: String,
    pub password: String,
    pub firstname: String,
    #[serde(default)]
    pub lastname: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub role_ids: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAdminUserInput {
    pub firstname: Option<String>,
    pub lastname: Option<String>,
    pub is_active: Option<bool>,
    pub password: Option<String>,
    pub role_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertRoleInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "type")]
    pub token_type: TokenType,
    pub lifespan_ms: Option<i64>,
    #[serde(default)]
    pub permissions: Vec<PermissionRule>,
}

#[derive(Clone)]
pub struct AuthService {
    repo: AuthRepository,
    config: Arc<AppConfig>,
    cache: Arc<dyn CacheBackend>,
    pubsub: Arc<dyn PubSubBackend>,
}

impl AuthService {
    pub fn new(
        repo: AuthRepository,
        config: Arc<AppConfig>,
        cache: Arc<dyn CacheBackend>,
        pubsub: Arc<dyn PubSubBackend>,
    ) -> Self {
        Self {
            repo,
            config,
            cache,
            pubsub,
        }
    }

    pub async fn register_first_admin(
        &self,
        input: RegisterAdminInput,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> ApiResult<AuthTokenResponse> {
        validate_email(&input.email)?;
        validate_password(&input.password)?;
        if input.firstname.trim().is_empty() {
            return Err(ApiError::Validation {
                message: "First name is required".into(),
                details: serde_json::json!({ "field": "firstname" }),
            });
        }

        if self.repo.count_admin_users().await? > 0 {
            return Err(ApiError::Conflict(
                "Initial administrator already exists".to_string(),
            ));
        }

        let pw_hash = hash_password(&input.password)?;
        let user = self
            .repo
            .create_admin_user(
                input.email.trim(),
                input.username.as_deref(),
                &pw_hash,
                input.firstname.trim(),
                input.lastname.trim(),
                true,
                true,
                &["role_super_admin".to_string()],
            )
            .await?;

        self.issue_admin_session(user, user_agent, ip).await
    }

    pub async fn login_admin(
        &self,
        input: LoginInput,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> ApiResult<AuthTokenResponse> {
        let Some(rec) = self
            .repo
            .find_admin_record_by_email(input.email.trim())
            .await?
        else {
            return Err(ApiError::Unauthorized(
                "Invalid email or password".to_string(),
            ));
        };

        if !rec.is_active {
            return Err(ApiError::Forbidden(
                "Administrator account is deactivated".to_string(),
            ));
        }

        if !verify_password(&input.password, &rec.password_hash)? {
            return Err(ApiError::Unauthorized(
                "Invalid email or password".to_string(),
            ));
        }

        let _ = self.repo.touch_last_login(&rec.id).await;
        let user = self
            .repo
            .find_admin_dto_by_id(&rec.id)
            .await?
            .ok_or_else(|| ApiError::Unauthorized("Invalid email or password".to_string()))?;

        self.issue_admin_session(user, user_agent, ip).await
    }

    pub async fn refresh_admin_token(
        &self,
        refresh_token: &str,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> ApiResult<AuthTokenResponse> {
        let hash = sha256_hex(refresh_token);
        let Some(session) = self.repo.find_session_by_hash(&hash).await? else {
            return Err(ApiError::Unauthorized("Invalid or expired refresh token".to_string()));
        };

        // Always delete used refresh token (rotation)
        self.repo.delete_session_by_hash(&hash).await?;

        let expires = chrono::DateTime::parse_from_rfc3339(&session.expires_at)
            .map_err(|_| ApiError::Unauthorized("Invalid session timestamp".to_string()))?;
        if expires.with_timezone(&Utc) <= Utc::now() {
            return Err(ApiError::Unauthorized("Refresh token has expired".to_string()));
        }

        let Some(user) = self.repo.find_admin_dto_by_id(&session.user_id).await? else {
            return Err(ApiError::Unauthorized("User account no longer exists".to_string()));
        };
        if !user.is_active {
            return Err(ApiError::Forbidden("Account is deactivated".to_string()));
        }

        self.issue_admin_session(user, user_agent, ip).await
    }

    pub async fn logout(&self, refresh_token: &str) -> ApiResult<()> {
        let hash = sha256_hex(refresh_token);
        self.repo.delete_session_by_hash(&hash).await
    }

    pub async fn resolve_bearer_actor(&self, bearer: &str) -> ApiResult<Actor> {
        let token = bearer.trim();
        if token.is_empty() {
            return Err(ApiError::Unauthorized("Missing bearer token".to_string()));
        }

        if token.starts_with("rapi_api_") {
            return self.resolve_static_token(false, token).await;
        }
        if token.starts_with("rapi_adm_") {
            return self.resolve_static_token(true, token).await;
        }

        // Otherwise verify as Admin Access JWT
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        let data = decode::<AccessClaims>(
            token,
            &DecodingKey::from_secret(self.config.jwt_secret.as_bytes()),
            &validation,
        )
        .map_err(|_| ApiError::Unauthorized("Invalid or expired access token".to_string()))?;

        if data.claims.typ != "admin_access" {
            return Err(ApiError::Unauthorized("Invalid token type".to_string()));
        }

        let Some(user) = self.repo.find_admin_dto_by_id(&data.claims.sub).await? else {
            return Err(ApiError::Unauthorized("User not found".to_string()));
        };
        if !user.is_active {
            return Err(ApiError::Forbidden("Account is deactivated".to_string()));
        }

        let permissions = if user.is_super_admin {
            vec![]
        } else {
            self.repo.get_user_effective_permissions(&user.id).await?
        };

        Ok(Actor::AdminUser {
            user_id: user.id,
            email: user.email,
            is_super_admin: user.is_super_admin,
            role_ids: user.roles.into_iter().map(|r| r.id).collect(),
            permissions,
        })
    }

    pub async fn resolve_public_actor(&self) -> ApiResult<Actor> {
        let permissions = self
            .repo
            .get_subject_permissions("public", "urole_public")
            .await?;
        Ok(Actor::Public { permissions })
    }

    async fn resolve_static_token(&self, is_admin: bool, raw_token: &str) -> ApiResult<Actor> {
        let hash = sha256_hex(raw_token);
        let cache_key = format!("rapi:token:{hash}");

        if let Ok(Some(cached)) = self.cache.get(&cache_key).await {
            if let Ok(actor) = serde_json::from_str::<Actor>(&cached) {
                return Ok(actor);
            }
        }

        let Some(tok) = self.repo.find_token_by_hash(is_admin, &hash).await? else {
            return Err(ApiError::Unauthorized("Invalid API token".to_string()));
        };

        if let Some(exp_str) = &tok.expires_at {
            if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(exp_str) {
                if exp.with_timezone(&Utc) <= Utc::now() {
                    return Err(ApiError::Unauthorized("API token has expired".to_string()));
                }
            }
        }

        let actor = if is_admin {
            Actor::AdminToken {
                token_id: tok.id,
                name: tok.name,
                token_type: tok.token_type,
                permissions: tok.permissions,
            }
        } else {
            Actor::ApiToken {
                token_id: tok.id,
                name: tok.name,
                token_type: tok.token_type,
                permissions: tok.permissions,
            }
        };

        if let Ok(serialized) = serde_json::to_string(&actor) {
            let _ = self
                .cache
                .set(
                    &cache_key,
                    &serialized,
                    Some(Duration::from_secs(TOKEN_CACHE_TTL_SECS)),
                )
                .await;
        }

        Ok(actor)
    }

    /// Sliding/fixed window rate limiter backed by CacheBackend (local or Redis).
    pub async fn check_rate_limit(
        &self,
        scope: &str,
        key: &str,
        limit: u32,
        window_secs: u64,
    ) -> ApiResult<()> {
        let cache_key = format!("rapi:rl:{scope}:{key}");
        let current: u32 = self
            .cache
            .get(&cache_key)
            .await?
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);

        if current >= limit {
            return Err(ApiError::RateLimited(
                "Too many requests. Please wait a minute before trying again.".to_string(),
            ));
        }

        self.cache
            .set(
                &cache_key,
                &(current + 1).to_string(),
                Some(Duration::from_secs(window_secs)),
            )
            .await?;
        Ok(())
    }

    // --- Admin Users ---

    pub async fn get_admin_profile(&self, user_id: &str) -> ApiResult<(AdminUserDto, Vec<PermissionRule>)> {
        let user = self
            .repo
            .find_admin_dto_by_id(user_id)
            .await?
            .ok_or_else(|| ApiError::NotFound("Admin user not found".to_string()))?;
        let perms = if user.is_super_admin {
            vec![PermissionRule {
                action: "*".into(),
                target_uid: None,
                fields: vec![],
            }]
        } else {
            self.repo.get_user_effective_permissions(&user.id).await?
        };
        Ok((user, perms))
    }

    pub async fn list_admin_users(&self) -> ApiResult<Vec<AdminUserDto>> {
        self.repo.list_admin_users().await
    }

    pub async fn create_admin_user(&self, input: CreateAdminUserInput) -> ApiResult<AdminUserDto> {
        validate_email(&input.email)?;
        validate_password(&input.password)?;
        if input.firstname.trim().is_empty() {
            return Err(ApiError::Validation {
                message: "First name is required".into(),
                details: serde_json::json!({ "field": "firstname" }),
            });
        }
        let pw_hash = hash_password(&input.password)?;
        let role_ids = if input.role_ids.is_empty() {
            vec!["role_author".to_string()]
        } else {
            input.role_ids
        };
        let is_super = role_ids.iter().any(|r| r == "role_super_admin");
        self.repo
            .create_admin_user(
                input.email.trim(),
                input.username.as_deref(),
                &pw_hash,
                input.firstname.trim(),
                input.lastname.trim(),
                input.is_active,
                is_super,
                &role_ids,
            )
            .await
    }

    pub async fn update_admin_user(
        &self,
        id: &str,
        input: UpdateAdminUserInput,
    ) -> ApiResult<AdminUserDto> {
        let pw_hash = match input.password.as_deref().filter(|p| !p.is_empty()) {
            Some(pw) => {
                validate_password(pw)?;
                Some(hash_password(pw)?)
            }
            None => None,
        };
        self.repo
            .update_admin_user(
                id,
                input.firstname.as_deref(),
                input.lastname.as_deref(),
                input.is_active,
                pw_hash.as_deref(),
                input.role_ids.as_deref(),
            )
            .await
    }

    pub async fn delete_admin_user(&self, id: &str, acting_user_id: Option<&str>) -> ApiResult<()> {
        if acting_user_id == Some(id) {
            return Err(ApiError::BadRequest(
                "You cannot delete your own administrator account".to_string(),
            ));
        }
        self.repo.delete_admin_user(id).await
    }

    // --- Roles & Permissions ---

    pub async fn list_roles(&self) -> ApiResult<Vec<AdminRoleDto>> {
        self.repo.list_roles().await
    }

    pub async fn create_role(&self, input: UpsertRoleInput) -> ApiResult<AdminRoleDto> {
        if input.name.trim().is_empty() {
            return Err(ApiError::Validation {
                message: "Role name is required".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }
        self.repo
            .create_role(input.name.trim(), input.description.trim())
            .await
    }

    pub async fn update_role(&self, id: &str, input: UpsertRoleInput) -> ApiResult<AdminRoleDto> {
        if input.name.trim().is_empty() {
            return Err(ApiError::Validation {
                message: "Role name is required".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }
        self.repo
            .update_role(id, input.name.trim(), input.description.trim())
            .await
    }

    pub async fn delete_role(&self, id: &str) -> ApiResult<()> {
        self.repo.delete_role(id).await
    }

    pub async fn get_role_permissions(&self, role_id: &str) -> ApiResult<Vec<PermissionRule>> {
        self.repo
            .get_subject_permissions("admin_role", role_id)
            .await
    }

    pub async fn update_role_permissions(
        &self,
        role_id: &str,
        rules: &[PermissionRule],
    ) -> ApiResult<Vec<PermissionRule>> {
        let updated = self
            .repo
            .replace_subject_permissions("admin_role", role_id, rules)
            .await?;
        let _ = self
            .pubsub
            .publish(InvalidationEvent::PermissionsUpdated {
                subject: format!("admin_role:{role_id}"),
            })
            .await;
        Ok(updated)
    }

    pub async fn get_public_permissions(&self) -> ApiResult<Vec<PermissionRule>> {
        self.repo
            .get_subject_permissions("public", "urole_public")
            .await
    }

    pub async fn update_public_permissions(
        &self,
        rules: &[PermissionRule],
    ) -> ApiResult<Vec<PermissionRule>> {
        self.repo
            .replace_subject_permissions("public", "urole_public", rules)
            .await
    }

    // --- API Tokens & Admin Tokens ---

    pub async fn list_tokens(&self, is_admin_token: bool) -> ApiResult<Vec<ApiTokenDto>> {
        self.repo.list_tokens(is_admin_token).await
    }

    pub async fn create_token(
        &self,
        is_admin_token: bool,
        admin_user_id: Option<&str>,
        input: CreateTokenInput,
    ) -> ApiResult<CreatedTokenDto> {
        if input.name.trim().is_empty() {
            return Err(ApiError::Validation {
                message: "Token name is required".into(),
                details: serde_json::json!({ "field": "name" }),
            });
        }
        let prefix = if is_admin_token { "rapi_adm_" } else { "rapi_api_" };
        let raw_key = format!("{prefix}{}", random_hex_32());
        let key_hash = sha256_hex(&raw_key);

        let expires_at = input
            .lifespan_ms
            .filter(|ms| *ms > 0)
            .map(|ms| (Utc::now() + ChronoDuration::milliseconds(ms)).to_rfc3339());

        let token = self
            .repo
            .create_token(
                is_admin_token,
                input.name.trim(),
                input.description.trim(),
                input.token_type,
                &key_hash,
                admin_user_id,
                input.lifespan_ms,
                expires_at.as_deref(),
                &input.permissions,
            )
            .await?;

        Ok(CreatedTokenDto {
            access_key: raw_key,
            token,
        })
    }

    pub async fn delete_token(&self, is_admin_token: bool, id: &str) -> ApiResult<()> {
        self.repo.delete_token(is_admin_token, id).await
    }

    // --- Helpers ---

    async fn issue_admin_session(
        &self,
        user: AdminUserDto,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> ApiResult<AuthTokenResponse> {
        let now = Utc::now();
        let exp = (now + ChronoDuration::seconds(ACCESS_TOKEN_TTL_SECS)).timestamp() as usize;
        let claims = AccessClaims {
            sub: user.id.clone(),
            email: user.email.clone(),
            typ: "admin_access".to_string(),
            iat: now.timestamp() as usize,
            exp,
        };

        let access_token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.config.jwt_secret.as_bytes()),
        )
        .map_err(|e| ApiError::Internal(format!("JWT sign failed: {e}")))?;

        let refresh_token = format!("rapi_rt_{}", random_hex_32());
        let refresh_hash = sha256_hex(&refresh_token);
        let refresh_exp = (now + ChronoDuration::days(REFRESH_TOKEN_TTL_DAYS)).to_rfc3339();

        self.repo
            .create_session("admin", &user.id, &refresh_hash, &refresh_exp, user_agent, ip)
            .await?;

        let permissions = if user.is_super_admin {
            vec![PermissionRule {
                action: "*".into(),
                target_uid: None,
                fields: vec![],
            }]
        } else {
            self.repo.get_user_effective_permissions(&user.id).await?
        };

        Ok(AuthTokenResponse {
            token: access_token,
            refresh_token,
            user,
            permissions,
        })
    }
}

fn hash_password(password: &str) -> ApiResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::Internal(format!("Password hashing failed: {e}")))
}

fn verify_password(password: &str, stored_hash: &str) -> ApiResult<bool> {
    let parsed = PasswordHash::new(stored_hash)
        .map_err(|e| ApiError::Internal(format!("Invalid stored password hash: {e}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

fn random_hex_32() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

fn validate_email(email: &str) -> ApiResult<()> {
    let trimmed = email.trim();
    if trimmed.len() < 5 || !trimmed.contains('@') || !trimmed.contains('.') {
        return Err(ApiError::Validation {
            message: "Enter a valid email address".into(),
            details: serde_json::json!({ "field": "email" }),
        });
    }
    Ok(())
}

fn validate_password(password: &str) -> ApiResult<()> {
    if password.len() < 8 {
        return Err(ApiError::Validation {
            message: "Password must be at least 8 characters long".into(),
            details: serde_json::json!({ "field": "password" }),
        });
    }
    Ok(())
}
