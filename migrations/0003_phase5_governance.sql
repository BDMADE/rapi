-- 0003_phase5_governance.sql
-- Phase 5: Preview URL configurations, Release error tracking, Workflow role/publish gates, and Audit Log indexes.

CREATE TABLE IF NOT EXISTS preview_configs (
    content_type_uid TEXT PRIMARY KEY NOT NULL REFERENCES content_types(uid) ON DELETE CASCADE,
    url_template TEXT NOT NULL,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    open_mode TEXT NOT NULL DEFAULT 'split' CHECK (open_mode IN ('split', 'tab')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

ALTER TABLE releases ADD COLUMN error_message TEXT;

ALTER TABLE workflow_stages ADD COLUMN allowed_role_ids_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE workflow_stages ADD COLUMN require_for_publish INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_releases_due ON releases(status, scheduled_at);
CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_actor ON audit_logs(actor_id, created_at DESC);
