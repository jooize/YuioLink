-- Anonymous, aggregate-only counters: one row per (UTC day, metric), bumped as
-- events happen.
--
-- The design constraint is that the site promises no tracking, and this table has
-- to keep that literally true. So there is deliberately no per-event row, and no
-- column that could hold an identity: no IP, no user agent, no referrer, no link
-- name, no destination, no session. A counter records that a thing happened N
-- times on a day, and nothing about who made it happen.
--
-- Day is the finest granularity stored, on purpose. Per-hour or per-minute buckets
-- on a low-traffic instance would start to place a specific person at a specific
-- moment, which is the line this table exists not to cross.
CREATE TABLE IF NOT EXISTS stats (
    day    TEXT    NOT NULL,           -- UTC 'YYYY-MM-DD', from date('now')
    metric TEXT    NOT NULL,           -- see db::Stat
    count  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, metric)
) WITHOUT ROWID;
