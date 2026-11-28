-- 0002_media_fts.sql
-- Full-text search virtual table for Media Library filenames, alt text, and captions.

CREATE VIRTUAL TABLE IF NOT EXISTS fts_upload_files USING fts5(
    file_id UNINDEXED,
    name,
    alternative_text,
    caption,
    mime
);

INSERT INTO fts_upload_files (file_id, name, alternative_text, caption, mime)
SELECT CAST(id AS TEXT), name, COALESCE(alternative_text, ''), COALESCE(caption, ''), mime
FROM upload_files;
