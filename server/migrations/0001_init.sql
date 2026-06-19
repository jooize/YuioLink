-- A single table backs both redirects and text links. `content` holds either
-- the redirect target or the text body; when `encrypted` is set it is opaque
-- ciphertext (the `yl1.` sealed format) the server can never read.
--
-- Every link is ephemeral: `expires_at` is NOT NULL and a reaper recycles names
-- once they pass. `name` is COLLATE NOCASE so lookups and uniqueness are
-- case-insensitive (the alternating-case display form is cosmetic).
CREATE TABLE IF NOT EXISTS links (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT    NOT NULL COLLATE NOCASE UNIQUE,
    kind         TEXT    NOT NULL CHECK (kind IN ('redirect', 'text')),
    content      TEXT    NOT NULL,
    content_type TEXT,
    encrypted    INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    expires_at   TEXT    NOT NULL,
    max_uses     INTEGER,
    hits         INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_links_name ON links (name);
CREATE INDEX IF NOT EXISTS idx_links_expires ON links (expires_at);
