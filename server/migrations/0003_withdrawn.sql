-- Tombstone flag for creator-withdrawn links. Deleting a link no longer removes
-- the row: it sets `withdrawn = 1`, which stops the link resolving (it serves a
-- 410 Gone) but KEEPS the name reserved until `expires_at`. Only the reaper
-- (DELETE WHERE expires_at <= now) ever frees a name back into the namespace, so
-- a name can never be silently repurposed within a link's stated life.
ALTER TABLE links ADD COLUMN withdrawn INTEGER NOT NULL DEFAULT 0;
