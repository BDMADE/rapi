-- 0005_phase7_extensibility.sql
-- Phase 7: Built-in Custom Fields, OIDC SSO providers, Opt-in AI settings, and Plugin metadata.

ALTER TABLE plugin_registry ADD COLUMN description TEXT NOT NULL DEFAULT '';

CREATE TABLE IF NOT EXISTS sso_providers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    issuer_url TEXT NOT NULL,
    authorization_endpoint TEXT,
    token_endpoint TEXT,
    userinfo_endpoint TEXT,
    client_id TEXT NOT NULL,
    client_secret TEXT NOT NULL,
    scopes TEXT NOT NULL DEFAULT 'openid email profile',
    default_role_id TEXT NOT NULL DEFAULT 'role_author' REFERENCES admin_roles(id),
    auto_provision INTEGER NOT NULL DEFAULT 1,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS ai_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    is_enabled INTEGER NOT NULL DEFAULT 0,
    provider TEXT NOT NULL DEFAULT 'local-deterministic', -- 'openai-compatible' | 'local-deterministic'
    api_url TEXT NOT NULL DEFAULT 'https://api.openai.com/v1',
    api_key TEXT,
    model TEXT NOT NULL DEFAULT 'gpt-4o-mini',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

INSERT OR IGNORE INTO ai_settings (id, is_enabled, provider, api_url, model)
VALUES (1, 0, 'local-deterministic', 'https://api.openai.com/v1', 'gpt-4o-mini');

INSERT OR IGNORE INTO custom_fields (uid, plugin_name, name, underlying_type, options_json) VALUES
    ('global::color-picker', 'rapi-core', 'Color Picker (Hex)', 'string', '{"widget":"color","regex":"^#[0-9a-fA-F]{6}$"}'),
    ('global::slug-seo', 'rapi-core', 'SEO Permalink Slug', 'string', '{"widget":"seo-slug","maxLength":160}'),
    ('global::code-editor', 'rapi-core', 'Code Snippet Editor', 'text', '{"widget":"code","language":"json"}'),
    ('global::key-value', 'rapi-core', 'Key-Value Attributes', 'json', '{"widget":"key-value"}');
