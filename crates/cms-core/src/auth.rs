use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserDto {
    pub id: String,
    pub email: String,
    pub username: Option<String>,
    pub firstname: String,
    pub lastname: String,
    pub is_active: bool,
    pub is_super_admin: bool,
    pub roles: Vec<AdminRoleSummary>,
    pub last_login_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminRoleSummary {
    pub id: String,
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminRoleDto {
    pub id: String,
    pub code: String,
    pub name: String,
    pub description: String,
    pub user_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRule {
    pub action: String,
    #[serde(default)]
    pub target_uid: Option<String>,
    #[serde(default)]
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermissionCatalogItem {
    pub action: String,
    pub label: String,
    pub category: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TokenType {
    FullAccess,
    ReadOnly,
    Custom,
}

impl TokenType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FullAccess => "full-access",
            Self::ReadOnly => "read-only",
            Self::Custom => "custom",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "full-access" => Some(Self::FullAccess),
            "read-only" => Some(Self::ReadOnly),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiTokenDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub token_type: TokenType,
    pub lifespan_ms: Option<i64>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub permissions: Vec<PermissionRule>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatedTokenDto {
    pub access_key: String,
    #[serde(flatten)]
    pub token: ApiTokenDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthTokenResponse {
    pub token: String,
    pub refresh_token: String,
    pub user: AdminUserDto,
    pub permissions: Vec<PermissionRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Actor {
    AdminUser {
        user_id: String,
        email: String,
        is_super_admin: bool,
        role_ids: Vec<String>,
        permissions: Vec<PermissionRule>,
    },
    ApiToken {
        token_id: String,
        name: String,
        token_type: TokenType,
        permissions: Vec<PermissionRule>,
    },
    AdminToken {
        token_id: String,
        name: String,
        token_type: TokenType,
        permissions: Vec<PermissionRule>,
    },
    Public {
        permissions: Vec<PermissionRule>,
    },
}

impl Actor {
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::AdminUser { .. } | Self::AdminToken { .. })
    }

    pub fn can(&self, action: &str, target_uid: Option<&str>) -> bool {
        match self {
            Self::AdminUser {
                is_super_admin,
                permissions,
                ..
            } => *is_super_admin || matches_permission(permissions, action, target_uid),
            Self::AdminToken {
                token_type,
                permissions,
                ..
            }
            | Self::ApiToken {
                token_type,
                permissions,
                ..
            } => match token_type {
                TokenType::FullAccess => true,
                TokenType::ReadOnly => {
                    action.ends_with(".read")
                        || action.ends_with(".find")
                        || action.ends_with(".findOne")
                }
                TokenType::Custom => matches_permission(permissions, action, target_uid),
            },
            Self::Public { permissions } => matches_permission(permissions, action, target_uid),
        }
    }
}

fn matches_permission(rules: &[PermissionRule], action: &str, target_uid: Option<&str>) -> bool {
    rules.iter().any(|rule| {
        let action_ok = rule.action == "*" || rule.action == action;
        let target_ok = match (&rule.target_uid, target_uid) {
            (None, _) => true,
            (Some(r), _) if r == "*" => true,
            (Some(r), Some(t)) => r == t,
            (Some(_), None) => false,
        };
        action_ok && target_ok
    })
}

/// Built-in RBAC permission catalog matching Strapi admin domains.
pub fn builtin_permission_catalog() -> Vec<PermissionCatalogItem> {
    vec![
        PermissionCatalogItem {
            action: "admin::system.overview.read".into(),
            label: "Read system overview".into(),
            category: "System".into(),
            description: "View runtime telemetry, database mode, and platform counts.".into(),
        },
        PermissionCatalogItem {
            action: "admin::users.read".into(),
            label: "Read admin users".into(),
            category: "Users & Roles".into(),
            description: "List and inspect administrator accounts.".into(),
        },
        PermissionCatalogItem {
            action: "admin::users.create".into(),
            label: "Create admin users".into(),
            category: "Users & Roles".into(),
            description: "Invite or create new administrator accounts.".into(),
        },
        PermissionCatalogItem {
            action: "admin::users.update".into(),
            label: "Update admin users".into(),
            category: "Users & Roles".into(),
            description: "Modify administrator profiles, status, and assigned roles.".into(),
        },
        PermissionCatalogItem {
            action: "admin::users.delete".into(),
            label: "Delete admin users".into(),
            category: "Users & Roles".into(),
            description: "Remove administrator accounts.".into(),
        },
        PermissionCatalogItem {
            action: "admin::roles.read".into(),
            label: "Read roles & permissions".into(),
            category: "Users & Roles".into(),
            description: "View roles and their permission matrix.".into(),
        },
        PermissionCatalogItem {
            action: "admin::roles.create".into(),
            label: "Create roles".into(),
            category: "Users & Roles".into(),
            description: "Create new custom admin roles.".into(),
        },
        PermissionCatalogItem {
            action: "admin::roles.update".into(),
            label: "Update roles & permissions".into(),
            category: "Users & Roles".into(),
            description: "Edit role details and toggle permissions in the RBAC matrix.".into(),
        },
        PermissionCatalogItem {
            action: "admin::roles.delete".into(),
            label: "Delete roles".into(),
            category: "Users & Roles".into(),
            description: "Delete custom admin roles.".into(),
        },
        PermissionCatalogItem {
            action: "admin::api-tokens.read".into(),
            label: "Read API & admin tokens".into(),
            category: "Tokens".into(),
            description: "List configured API and admin access tokens.".into(),
        },
        PermissionCatalogItem {
            action: "admin::api-tokens.create".into(),
            label: "Create API & admin tokens".into(),
            category: "Tokens".into(),
            description: "Issue new scoped API or admin tokens.".into(),
        },
        PermissionCatalogItem {
            action: "admin::api-tokens.delete".into(),
            label: "Revoke API & admin tokens".into(),
            category: "Tokens".into(),
            description: "Permanently revoke existing tokens.".into(),
        },
        PermissionCatalogItem {
            action: "plugin::content-type-builder.read".into(),
            label: "Read content-type schemas".into(),
            category: "Content-Type Builder".into(),
            description: "Inspect collection types, single types, and components.".into(),
        },
        PermissionCatalogItem {
            action: "plugin::content-type-builder.update".into(),
            label: "Modify content-type schemas".into(),
            category: "Content-Type Builder".into(),
            description: "Create, alter, or delete runtime content-type schemas.".into(),
        },
        PermissionCatalogItem {
            action: "plugin::content-manager.explorer.read".into(),
            label: "Read entries".into(),
            category: "Content Manager".into(),
            description: "View draft and published entries.".into(),
        },
        PermissionCatalogItem {
            action: "plugin::content-manager.explorer.create".into(),
            label: "Create entries".into(),
            category: "Content Manager".into(),
            description: "Author new entries in collection types.".into(),
        },
        PermissionCatalogItem {
            action: "plugin::content-manager.explorer.update".into(),
            label: "Update entries".into(),
            category: "Content Manager".into(),
            description: "Edit existing entries.".into(),
        },
        PermissionCatalogItem {
            action: "plugin::content-manager.explorer.publish".into(),
            label: "Publish & unpublish entries".into(),
            category: "Content Manager".into(),
            description: "Promote drafts to published status or unpublish live documents.".into(),
        },
        PermissionCatalogItem {
            action: "plugin::content-manager.explorer.delete".into(),
            label: "Delete entries".into(),
            category: "Content Manager".into(),
            description: "Delete content entries.".into(),
        },
        PermissionCatalogItem {
            action: "plugin::upload.read".into(),
            label: "Read media assets".into(),
            category: "Media Library".into(),
            description: "Browse uploaded files and folders.".into(),
        },
        PermissionCatalogItem {
            action: "plugin::upload.assets.create".into(),
            label: "Upload media assets".into(),
            category: "Media Library".into(),
            description: "Upload new files and create folders.".into(),
        },
        PermissionCatalogItem {
            action: "plugin::i18n.locale.read".into(),
            label: "Read locales".into(),
            category: "Internationalization".into(),
            description: "List configured i18n locales.".into(),
        },
        PermissionCatalogItem {
            action: "plugin::i18n.locale.update".into(),
            label: "Manage locales".into(),
            category: "Internationalization".into(),
            description: "Create, update, set default, and delete i18n locales.".into(),
        },
        PermissionCatalogItem {
            action: "admin::preview.update".into(),
            label: "Configure preview URLs".into(),
            category: "Governance".into(),
            description: "Manage frontend preview URL templates per content type.".into(),
        },
        PermissionCatalogItem {
            action: "admin::releases.read".into(),
            label: "Read releases".into(),
            category: "Governance".into(),
            description: "Inspect scheduled and completed release groups.".into(),
        },
        PermissionCatalogItem {
            action: "admin::releases.update".into(),
            label: "Manage & publish releases".into(),
            category: "Governance".into(),
            description: "Create, edit, schedule, and publish release groups.".into(),
        },
        PermissionCatalogItem {
            action: "admin::workflows.read".into(),
            label: "Read review workflows".into(),
            category: "Governance".into(),
            description: "View workflow stages and entry assignments.".into(),
        },
        PermissionCatalogItem {
            action: "admin::workflows.update".into(),
            label: "Manage workflow stages".into(),
            category: "Governance".into(),
            description: "Create, reorder, and configure role/publish gates on workflow stages.".into(),
        },
        PermissionCatalogItem {
            action: "admin::audit-logs.read".into(),
            label: "Read audit logs".into(),
            category: "Governance".into(),
            description: "Query and inspect administrator mutation audit trails.".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_permission_checks_enforce_scopes() {
        let super_admin = Actor::AdminUser {
            user_id: "u1".into(),
            email: "admin@rapi.dev".into(),
            is_super_admin: true,
            role_ids: vec![],
            permissions: vec![],
        };
        assert!(super_admin.can("admin::users.delete", None));

        let author = Actor::AdminUser {
            user_id: "u2".into(),
            email: "author@rapi.dev".into(),
            is_super_admin: false,
            role_ids: vec!["role_author".into()],
            permissions: vec![PermissionRule {
                action: "plugin::content-manager.explorer.create".into(),
                target_uid: Some("api::article.article".into()),
                fields: vec![],
            }],
        };
        assert!(author.can(
            "plugin::content-manager.explorer.create",
            Some("api::article.article")
        ));
        assert!(!author.can(
            "plugin::content-manager.explorer.create",
            Some("api::page.page")
        ));
        assert!(!author.can("admin::users.delete", None));

        let read_only = Actor::ApiToken {
            token_id: "t1".into(),
            name: "CI Reader".into(),
            token_type: TokenType::ReadOnly,
            permissions: vec![],
        };
        assert!(read_only.can("api::article.article.find", None));
        assert!(!read_only.can("api::article.article.create", None));
    }
}
