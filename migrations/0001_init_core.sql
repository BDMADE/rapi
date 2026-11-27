-- 0001_init_core.sql
-- Core RAPI platform metadata, RBAC, document index, status-isolated relations, media, and fallback job queue.

CREATE TABLE IF NOT EXISTS admin_users (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL UNIQUE,
    username TEXT UNIQUE,
    password_hash TEXT NOT NULL,
    firstname TEXT NOT NULL DEFAULT '',
    lastname TEXT NOT NULL DEFAULT '',
    is_active INTEGER NOT NULL DEFAULT 1,
    is_super_admin INTEGER NOT NULL DEFAULT 0,
    last_login_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS admin_roles (
    id TEXT PRIMARY KEY NOT NULL,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS admin_user_roles (
    user_id TEXT NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES admin_roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

CREATE TABLE IF NOT EXISTS permissions (
    id TEXT PRIMARY KEY NOT NULL,
    subject_type TEXT NOT NULL, -- 'admin_role' | 'user_role' | 'api_token' | 'public'
    subject_id TEXT NOT NULL,
    action TEXT NOT NULL,       -- e.g. 'plugin::content-manager.explorer.read' or 'api::article.article.find'
    target_uid TEXT,
    properties_json TEXT NOT NULL DEFAULT '{}',
    conditions_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_permissions_subject ON permissions(subject_type, subject_id);

CREATE TABLE IF NOT EXISTS api_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    token_type TEXT NOT NULL CHECK (token_type IN ('full-access', 'read-only', 'custom')),
    access_key_hash TEXT NOT NULL UNIQUE,
    lifespan_ms INTEGER,
    expires_at TEXT,
    last_used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS admin_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    token_type TEXT NOT NULL CHECK (token_type IN ('full-access', 'read-only', 'custom')),
    access_key_hash TEXT NOT NULL UNIQUE,
    admin_user_id TEXT REFERENCES admin_users(id) ON DELETE CASCADE,
    lifespan_ms INTEGER,
    expires_at TEXT,
    last_used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS user_roles (
    id TEXT PRIMARY KEY NOT NULL,
    role_type TEXT NOT NULL UNIQUE, -- e.g. 'authenticated', 'public'
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS end_users (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL UNIQUE,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    provider TEXT NOT NULL DEFAULT 'local',
    password_hash TEXT NOT NULL,
    reset_password_token TEXT,
    confirmation_token TEXT,
    confirmed INTEGER NOT NULL DEFAULT 1,
    blocked INTEGER NOT NULL DEFAULT 0,
    role_id TEXT NOT NULL REFERENCES user_roles(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_type TEXT NOT NULL CHECK (user_type IN ('admin', 'end_user')),
    user_id TEXT NOT NULL,
    refresh_token_hash TEXT NOT NULL UNIQUE,
    user_agent TEXT,
    ip_address TEXT,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_type, user_id);

CREATE TABLE IF NOT EXISTS locales (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS content_types (
    uid TEXT PRIMARY KEY NOT NULL, -- e.g. 'api::article.article' or 'shared.seo'
    kind TEXT NOT NULL CHECK (kind IN ('collectionType', 'singleType', 'component')),
    category TEXT,
    singular_name TEXT NOT NULL,
    plural_name TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    icon TEXT NOT NULL DEFAULT 'file-text',
    draft_and_publish INTEGER NOT NULL DEFAULT 1,
    i18n_enabled INTEGER NOT NULL DEFAULT 0,
    plugin_options_json TEXT NOT NULL DEFAULT '{}',
    options_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS field_definitions (
    id TEXT PRIMARY KEY NOT NULL,
    content_type_uid TEXT NOT NULL REFERENCES content_types(uid) ON DELETE CASCADE,
    name TEXT NOT NULL,
    field_type TEXT NOT NULL, -- string, text, richtext, blocks, integer, biginteger, float, decimal, boolean, date, datetime, time, email, password, enumeration, json, uid, media, relation, component, dynamiczone, custom
    required INTEGER NOT NULL DEFAULT 0,
    unique_field INTEGER NOT NULL DEFAULT 0,
    localized INTEGER NOT NULL DEFAULT 0,
    private_field INTEGER NOT NULL DEFAULT 0,
    position INTEGER NOT NULL DEFAULT 0,
    default_value_json TEXT,
    validation_json TEXT NOT NULL DEFAULT '{}',
    relation_json TEXT,
    component_json TEXT,
    custom_field_uid TEXT,
    UNIQUE(content_type_uid, name)
);

CREATE INDEX IF NOT EXISTS idx_fields_ct ON field_definitions(content_type_uid, position);

CREATE TABLE IF NOT EXISTS schema_migrations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    content_type_uid TEXT NOT NULL,
    operation TEXT NOT NULL,
    ddl_sql TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Cross-type authoritative Document Service index (synced transactionally with ct_<uid> tables)
CREATE TABLE IF NOT EXISTS documents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id TEXT NOT NULL,
    content_type_uid TEXT NOT NULL REFERENCES content_types(uid) ON DELETE CASCADE,
    locale TEXT NOT NULL DEFAULT 'en',
    status TEXT NOT NULL CHECK (status IN ('draft', 'published')),
    version INTEGER NOT NULL DEFAULT 1,
    published_at TEXT,
    created_by TEXT,
    updated_by TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(document_id, locale, status)
);

CREATE INDEX IF NOT EXISTS idx_documents_lookup ON documents(content_type_uid, status, locale);
CREATE INDEX IF NOT EXISTS idx_documents_doc_id ON documents(document_id);

CREATE TABLE IF NOT EXISTS content_versions (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL,
    content_type_uid TEXT NOT NULL,
    version INTEGER NOT NULL,
    locale TEXT NOT NULL,
    status TEXT NOT NULL,
    data_json TEXT NOT NULL,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_versions_doc ON content_versions(document_id, locale, version DESC);

-- Status-isolated ordered relations (supports root fields and component-nested fields)
CREATE TABLE IF NOT EXISTS content_relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_content_type_uid TEXT NOT NULL,
    from_document_id TEXT NOT NULL,
    from_status TEXT NOT NULL CHECK (from_status IN ('draft', 'published')),
    from_locale TEXT NOT NULL DEFAULT 'en',
    field_name TEXT NOT NULL,
    component_path TEXT,
    to_content_type_uid TEXT NOT NULL,
    to_document_id TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_relations_from ON content_relations(from_document_id, from_status, from_locale, field_name);
CREATE INDEX IF NOT EXISTS idx_relations_to ON content_relations(to_document_id);

CREATE TABLE IF NOT EXISTS upload_folders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    path_id INTEGER NOT NULL UNIQUE,
    path TEXT NOT NULL UNIQUE,
    parent_id INTEGER REFERENCES upload_folders(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS upload_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    alternative_text TEXT,
    caption TEXT,
    width INTEGER,
    height INTEGER,
    formats_json TEXT NOT NULL DEFAULT '{}',
    hash TEXT NOT NULL,
    ext TEXT NOT NULL,
    mime TEXT NOT NULL,
    size_kb REAL NOT NULL,
    url TEXT NOT NULL,
    preview_url TEXT,
    provider TEXT NOT NULL DEFAULT 'local',
    folder_id INTEGER REFERENCES upload_folders(id) ON DELETE SET NULL,
    folder_path TEXT NOT NULL DEFAULT '/',
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS upload_file_relations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_document_id TEXT NOT NULL,
    from_status TEXT NOT NULL CHECK (from_status IN ('draft', 'published')),
    from_locale TEXT NOT NULL DEFAULT 'en',
    field_name TEXT NOT NULL,
    component_path TEXT,
    file_id INTEGER NOT NULL REFERENCES upload_files(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_upload_rel_from ON upload_file_relations(from_document_id, from_status, from_locale, field_name);

CREATE TABLE IF NOT EXISTS webhooks (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    headers_json TEXT NOT NULL DEFAULT '{}',
    events_json TEXT NOT NULL DEFAULT '[]',
    secret TEXT,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id TEXT PRIMARY KEY NOT NULL,
    webhook_id TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event TEXT NOT NULL,
    status_code INTEGER,
    success INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 1,
    request_json TEXT NOT NULL,
    response_body TEXT,
    duration_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY NOT NULL,
    action TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    actor_id TEXT,
    target_uid TEXT,
    target_id TEXT,
    ip_address TEXT,
    user_agent TEXT,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_created ON audit_logs(created_at DESC);

CREATE TABLE IF NOT EXISTS releases (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    scheduled_at TEXT,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'done', 'failed')),
    released_at TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS release_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    content_type_uid TEXT NOT NULL,
    document_id TEXT NOT NULL,
    locale TEXT NOT NULL DEFAULT 'en',
    action TEXT NOT NULL DEFAULT 'publish' CHECK (action IN ('publish', 'unpublish')),
    UNIQUE(release_id, document_id, locale)
);

CREATE TABLE IF NOT EXISTS workflow_stages (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#4f46e5',
    position INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS workflow_entries (
    document_id TEXT NOT NULL,
    locale TEXT NOT NULL DEFAULT 'en',
    stage_id TEXT NOT NULL REFERENCES workflow_stages(id) ON DELETE CASCADE,
    assignee_id TEXT REFERENCES admin_users(id) ON DELETE SET NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (document_id, locale)
);

CREATE TABLE IF NOT EXISTS email_templates (
    id TEXT PRIMARY KEY NOT NULL,
    code TEXT NOT NULL UNIQUE,
    subject TEXT NOT NULL,
    html_body TEXT NOT NULL,
    text_body TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Persistent job queue used in zero-config local mode (and dead-letter storage)
CREATE TABLE IF NOT EXISTS job_queue (
    id TEXT PRIMARY KEY NOT NULL,
    queue TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'processing', 'completed', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    available_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_job_queue_poll ON job_queue(queue, status, available_at);

CREATE TABLE IF NOT EXISTS plugin_registry (
    name TEXT PRIMARY KEY NOT NULL,
    version TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('native', 'wasm')),
    enabled INTEGER NOT NULL DEFAULT 1,
    wasm_path TEXT,
    config_json TEXT NOT NULL DEFAULT '{}',
    installed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS custom_fields (
    uid TEXT PRIMARY KEY NOT NULL,
    plugin_name TEXT,
    name TEXT NOT NULL,
    underlying_type TEXT NOT NULL,
    options_json TEXT NOT NULL DEFAULT '{}'
);

-- Seed built-in Strapi-equivalent default locale and admin roles
INSERT OR IGNORE INTO locales (code, name, is_default)
VALUES ('en', 'English (en)', 1);

INSERT OR IGNORE INTO admin_roles (id, code, name, description) VALUES
    ('role_super_admin', 'strapi-super-admin', 'Super Admin', 'Super Admins can access and manage all features and settings.'),
    ('role_editor', 'strapi-editor', 'Editor', 'Editors can manage and publish contents including those of other users.'),
    ('role_author', 'strapi-author', 'Author', 'Authors can manage the content they have created.');

INSERT OR IGNORE INTO user_roles (id, role_type, name, description) VALUES
    ('urole_authenticated', 'authenticated', 'Authenticated', 'Default role given to authenticated end users.'),
    ('urole_public', 'public', 'Public', 'Default role given to unauthenticated requests.');

INSERT OR IGNORE INTO workflow_stages (id, name, color, position) VALUES
    ('stage_todo', 'To do', '#71717a', 0),
    ('stage_in_progress', 'In progress', '#4f46e5', 1),
    ('stage_ready_review', 'Ready to review', '#d97706', 2),
    ('stage_reviewed', 'Reviewed', '#059669', 3);
