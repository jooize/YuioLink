-- Drop the plain-text creator token column that 0007 left behind.
--
-- 0007 began storing only each token's SHA-256, and the server hashed and
-- cleared the plain-text tokens still here at every startup since. Every link
-- lives at most seven days, and 0007 went live on 2026-09-25, so no link from
-- before it survives and every `delete_token` is NULL. The startup step goes
-- with this column.
ALTER TABLE links DROP COLUMN delete_token;
