//! SQLite access via a shared pool (created once, not per-request like the
//! original Go handlers).

use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

use yuiolink_core::{generate_name, words_for};

#[derive(sqlx::FromRow)]
pub struct LinkDetail {
    pub name: String,
    pub kind: String,
    pub content: String,
    // Selected so the row maps cleanly; minimal Text only renders plaintext, so
    // nothing reads this yet.
    #[allow(dead_code)]
    pub content_type: Option<String>,
    pub hits: i64,
    pub created_at: String,
    pub expires_at: String,
    pub max_uses: Option<i64>,
}

/// Columns selected for a [`LinkDetail`], in struct order.
const LINK_COLUMNS: &str =
    "name, kind, content, content_type, hits, created_at, expires_at, max_uses";

/// A link still resolvable: not past `expires_at`, not over `max_uses`, and not
/// withdrawn by its creator.
const LIVE_PREDICATE: &str =
    "expires_at > datetime('now') AND (max_uses IS NULL OR hits < max_uses) AND withdrawn = 0";

/// Fields needed to create a link; the name and `expires_at` are derived here.
pub struct NewLink<'a> {
    pub kind: &'a str,
    pub content: &'a str,
    pub content_type: Option<&'a str>,
    pub ttl_seconds: i64,
    pub max_uses: Option<i64>,
    /// Request a private (long, unguessable) name even for an unlimited link. A
    /// limited link is always given the long name regardless of this flag.
    pub private: bool,
    /// Secret that authorizes deleting this link later; `None` means the link
    /// cannot be deleted via the API (no holder).
    pub delete_token: Option<&'a str>,
}

/// Live name counts per word-tier: `[0]` = live 1-word names, `[1]` = 2-word, and
/// so on up to four words. Feeds the public allocation policy
/// ([`yuiolink_core::words_for`]); refreshed by the reaper.
pub type Occupancy = [u64; 4];

/// An empty namespace (every tier free) — public links get the shortest name.
/// Handy at first startup and in tests.
pub const EMPTY_OCCUPANCY: Occupancy = [0; 4];

/// The result of creating a link: its generated name, computed expiry, and the
/// word count of the name actually issued (after any collision growth).
pub struct InsertedLink {
    pub name: String,
    pub expires_at: String,
    pub words: usize,
}

pub async fn connect(db_path: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        // WAL + a busy timeout keep the reaper's DELETEs from colliding with
        // concurrent reads/writes under the connection pool.
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// Read a link without consuming it — for the interstitial preview and the REST
/// API. Expired, withdrawn, or use-exhausted links read as `None`. Lookup is
/// case-insensitive (the `name` column is `COLLATE NOCASE`).
pub async fn get_link_live(
    pool: &SqlitePool,
    name: &str,
) -> Result<Option<LinkDetail>, sqlx::Error> {
    let sql = format!("SELECT {LINK_COLUMNS} FROM links WHERE name = ? AND {LIVE_PREDICATE}");
    sqlx::query_as::<_, LinkDetail>(&sql)
        .bind(name)
        .fetch_optional(pool)
        .await
}

/// Read a still-reserved row regardless of whether it is live: it must only be
/// unexpired. This is the tombstone reader — it returns links that are
/// use-exhausted or creator-withdrawn (so the resolver can answer 410 Gone, and
/// the revealed view can still read content after the final use was spent),
/// while expired/recycled/unknown names read as `None` (the resolver's 404).
pub async fn get_link_any(
    pool: &SqlitePool,
    name: &str,
) -> Result<Option<LinkDetail>, sqlx::Error> {
    let sql = format!(
        "SELECT {LINK_COLUMNS} FROM links WHERE name = ? AND expires_at > datetime('now')"
    );
    sqlx::query_as::<_, LinkDetail>(&sql)
        .bind(name)
        .fetch_optional(pool)
        .await
}

/// Atomically count a hit and return the link, but only while it is still live.
/// One UPDATE … RETURNING does the not-expired / uses-left check and the
/// increment together, so a burn-after-read link cannot be resolved twice even
/// under concurrent requests. Returns `None` when expired or exhausted.
pub async fn consume_link(
    pool: &SqlitePool,
    name: &str,
) -> Result<Option<LinkDetail>, sqlx::Error> {
    let sql = format!(
        "UPDATE links SET hits = hits + 1 WHERE name = ? AND {LIVE_PREDICATE} RETURNING {LINK_COLUMNS}"
    );
    sqlx::query_as::<_, LinkDetail>(&sql)
        .bind(name)
        .fetch_optional(pool)
        .await
}

/// Insert a link under a freshly generated name. The starting word count comes
/// from the link's type and the current per-tier `occupancy`: private/single-use
/// links always get four words; a public link gets the shortest tier still under
/// its TTL band's occupancy ceiling (see [`yuiolink_core::words_for`]). It then
/// grows by one after every `COLLISION_GROW_AT` unique-name collisions, so a tier
/// that filled since the last occupancy refresh still resolves quickly.
pub async fn insert_link(
    pool: &SqlitePool,
    link: NewLink<'_>,
    occupancy: &Occupancy,
) -> Result<InsertedLink, sqlx::Error> {
    /// Grow the name by a word after this many collisions in a row.
    const COLLISION_GROW_AT: u32 = 8;

    let mut words = words_for(link.max_uses, link.private, link.ttl_seconds, occupancy);
    let mut collisions: u32 = 0;

    loop {
        let name = generate_name(words);
        let result: Result<String, sqlx::Error> = sqlx::query_scalar(
            "INSERT INTO links (name, kind, content, content_type, expires_at, max_uses, delete_token, words) \
             VALUES (?, ?, ?, ?, datetime('now', '+' || ? || ' seconds'), ?, ?, ?) \
             RETURNING expires_at",
        )
        .bind(&name)
        .bind(link.kind)
        .bind(link.content)
        .bind(link.content_type)
        .bind(link.ttl_seconds)
        .bind(link.max_uses)
        .bind(link.delete_token)
        .bind(words as i64)
        .fetch_one(pool)
        .await;

        match result {
            Ok(expires_at) => return Ok(InsertedLink { name, expires_at, words }),
            Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
                collisions += 1;
                if collisions.is_multiple_of(COLLISION_GROW_AT) {
                    words += 1;
                }
            }
            Err(e) => return Err(e),
        }
    }
}

/// Count live names grouped by word-tier, for the occupancy-driven public policy.
/// Names longer than four words (which only the collision valve ever produces) are
/// folded away — the policy never starts above four. Run periodically by the reaper.
pub async fn live_counts_by_words(pool: &SqlitePool) -> Result<Occupancy, sqlx::Error> {
    let sql = format!("SELECT words, COUNT(*) FROM links WHERE {LIVE_PREDICATE} GROUP BY words");
    let rows: Vec<(i64, i64)> = sqlx::query_as(&sql).fetch_all(pool).await?;
    let mut occ = EMPTY_OCCUPANCY;
    for (words, n) in rows {
        if (1..=occ.len() as i64).contains(&words) {
            occ[words as usize - 1] = n.max(0) as u64;
        }
    }
    Ok(occ)
}

/// Withdraw a link by name, but only when `token` matches the secret stored at
/// creation. This does NOT delete the row: it sets `withdrawn = 1`, which stops
/// the link resolving (it serves 410 Gone) while keeping the name reserved as a
/// tombstone until expiry — so a withdrawn name can never be re-registered and
/// silently repurposed within its stated life. Returns whether a row matched.
///
/// A NULL stored token never matches (`NULL = ?` is never true), so a tokenless
/// link cannot be withdrawn — fail closed. `name` matches case-insensitively
/// (the column is NOCASE); the token compares with the default binary collation
/// (exact).
pub async fn delete_link(pool: &SqlitePool, name: &str, token: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("UPDATE links SET withdrawn = 1 WHERE name = ? AND delete_token = ?")
        .bind(name)
        .bind(token)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete every expired row, freeing those names for reuse. Returns the count.
pub async fn reap_expired(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM links WHERE expires_at <= datetime('now')")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A fresh, isolated on-disk database (in-memory would give each pooled
    /// connection its own empty DB).
    async fn test_pool() -> SqlitePool {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("yuiolink-db-{}-{n}.db", std::process::id()));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        connect(path.to_str().unwrap()).await.expect("connect test db")
    }

    fn redirect(content: &str, max_uses: Option<i64>) -> NewLink<'_> {
        NewLink {
            kind: "redirect",
            content,
            content_type: None,
            ttl_seconds: 3600,
            max_uses,
            private: false,
            delete_token: Some("tok"),
        }
    }

    /// Force a link's expiry into the past, simulating the clock running out.
    async fn expire_now(pool: &SqlitePool, name: &str) {
        sqlx::query("UPDATE links SET expires_at = datetime('now', '-1 hour') WHERE name = ?")
            .bind(name)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn preview_reads_without_consuming() {
        let pool = test_pool().await;
        let l = insert_link(&pool, redirect("https://example.com", None), &EMPTY_OCCUPANCY).await.unwrap();
        // Reading twice must not move the hit counter.
        assert!(get_link_live(&pool, &l.name).await.unwrap().is_some());
        let d = get_link_live(&pool, &l.name).await.unwrap().unwrap();
        assert_eq!(d.hits, 0);
    }

    #[tokio::test]
    async fn one_time_consume_then_tombstone() {
        let pool = test_pool().await;
        let l = insert_link(&pool, redirect("https://example.com", Some(1)), &EMPTY_OCCUPANCY).await.unwrap();

        // First consume succeeds and counts the hit.
        let d = consume_link(&pool, &l.name).await.unwrap().unwrap();
        assert_eq!(d.hits, 1);

        // Now exhausted: not live, but still a reserved tombstone (410, not 404).
        assert!(get_link_live(&pool, &l.name).await.unwrap().is_none());
        assert!(get_link_any(&pool, &l.name).await.unwrap().is_some());
        // A second consume cannot resolve it.
        assert!(consume_link(&pool, &l.name).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn withdraw_tombstones_keeps_name_reserved() {
        let pool = test_pool().await;
        let l = insert_link(&pool, redirect("https://example.com", None), &EMPTY_OCCUPANCY).await.unwrap();

        // Wrong token does nothing (fail closed).
        assert!(!delete_link(&pool, &l.name, "nope").await.unwrap());
        assert!(get_link_live(&pool, &l.name).await.unwrap().is_some());

        // Correct token withdraws: stops resolving but keeps the row (tombstone).
        assert!(delete_link(&pool, &l.name, "tok").await.unwrap());
        assert!(get_link_live(&pool, &l.name).await.unwrap().is_none());
        assert!(get_link_any(&pool, &l.name).await.unwrap().is_some());
        // A withdrawn link cannot be consumed.
        assert!(consume_link(&pool, &l.name).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn expired_is_gone_to_both_readers_and_reaps() {
        let pool = test_pool().await;
        let l = insert_link(&pool, redirect("https://example.com", None), &EMPTY_OCCUPANCY).await.unwrap();
        expire_now(&pool, &l.name).await;

        // Expired reads as missing everywhere (the 404, not the 410, case).
        assert!(get_link_live(&pool, &l.name).await.unwrap().is_none());
        assert!(get_link_any(&pool, &l.name).await.unwrap().is_none());

        // The reaper frees the name (the only path that recycles it).
        assert_eq!(reap_expired(&pool).await.unwrap(), 1);
        assert_eq!(reap_expired(&pool).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn private_unlimited_link_gets_a_long_name() {
        let pool = test_pool().await;
        // A public unlimited link is one lowercase word (no case boundary).
        let public = insert_link(&pool, redirect("https://example.com/a", None), &EMPTY_OCCUPANCY).await.unwrap();
        assert!(!public.name.chars().any(|c| c.is_ascii_uppercase()), "{}", public.name);
        // A private unlimited link is four words, so alternating-case adds uppercase.
        let mut nl = redirect("https://example.com/b", None);
        nl.private = true;
        let private = insert_link(&pool, nl, &EMPTY_OCCUPANCY).await.unwrap();
        assert!(private.name.chars().any(|c| c.is_ascii_uppercase()), "{}", private.name);
    }

    #[tokio::test]
    async fn public_name_lengthens_when_the_short_tier_is_crowded() {
        let pool = test_pool().await;
        // Pretend the 1-word tier is ~50% full. A 7-day public link (40% ceiling)
        // must yield to two words; the recorded `words` reflects the real name.
        let cap1 = yuiolink_core::WORD_COUNT as u64;
        let crowded: Occupancy = [cap1 / 2, 0, 0, 0];
        let mut nl = redirect("https://example.com", None);
        nl.ttl_seconds = 604800; // 7 days
        let l = insert_link(&pool, nl, &crowded).await.unwrap();
        assert_eq!(l.words, 2, "name was {}", l.name);
        assert!(l.name.chars().any(|c| c.is_ascii_uppercase()), "{}", l.name);
    }
}
