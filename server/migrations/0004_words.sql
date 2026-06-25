-- Per-name word count, so the create path can read live occupancy per tier and
-- hand a public link the shortest name still available (occupancy-driven default,
-- with short-lived links given priority on the scarce short tiers). Private and
-- single-use links are always 4 words regardless.
--
-- Existing rows default to 1; they are short-lived (<=7 days) and wash out, and a
-- slight 1-word over-count only makes new public names a touch longer (the safe
-- direction) until they expire.
ALTER TABLE links ADD COLUMN words INTEGER NOT NULL DEFAULT 1;

-- The reaper's periodic `... WHERE <live> GROUP BY words` occupancy sweep.
CREATE INDEX IF NOT EXISTS idx_links_words ON links (words);
