-- 0004_phase6_integrations.sql
-- Phase 6: End-user password reset expiry, local email outbox, default email templates, and webhook delivery indexes.

ALTER TABLE end_users ADD COLUMN reset_password_expires_at TEXT;

CREATE TABLE IF NOT EXISTS email_outbox (
    id TEXT PRIMARY KEY NOT NULL,
    to_address TEXT NOT NULL,
    from_address TEXT NOT NULL,
    subject TEXT NOT NULL,
    html_body TEXT NOT NULL,
    text_body TEXT NOT NULL,
    transport TEXT NOT NULL DEFAULT 'local', -- 'local' | 'smtp'
    status TEXT NOT NULL DEFAULT 'sent',
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_wh ON webhook_deliveries(webhook_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_end_users_reset ON end_users(reset_password_token);
CREATE INDEX IF NOT EXISTS idx_email_outbox_created ON email_outbox(created_at DESC);

INSERT OR IGNORE INTO email_templates (id, code, subject, html_body, text_body) VALUES
    (
        'tpl_reset_password',
        'reset_password',
        'Reset your password',
        '<h2>Password Reset Request</h2><p>Hello {{ username }},</p><p>Use the reset code below to set a new password:</p><p><code>{{ code }}</code></p>',
        'Hello {{ username }}, use this reset code to set a new password: {{ code }}'
    ),
    (
        'tpl_welcome',
        'welcome',
        'Welcome to RAPI',
        '<h2>Welcome, {{ username }}!</h2><p>Your account ({{ email }}) is ready.</p>',
        'Welcome, {{ username }}! Your account ({{ email }}) is ready.'
    );
