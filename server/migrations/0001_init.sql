-- A single table backs both redirects and pastes. `content` holds either the
-- redirect target or the paste body; when `encrypted` is set it is opaque
-- ciphertext (the `yl1.` sealed format) the server can never read.
CREATE TABLE IF NOT EXISTS links (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT    NOT NULL UNIQUE,
    kind         TEXT    NOT NULL CHECK (kind IN ('redirect', 'paste')),
    content      TEXT    NOT NULL,
    content_type TEXT,
    encrypted    INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now')),
    hits         INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_links_name ON links (name);
