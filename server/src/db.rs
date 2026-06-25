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

/// The result of creating a link: its generated name and computed expiry.
pub struct InsertedLink {
    pub name: String,
    pub expires_at: String,
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

/// Insert a link under a freshly generated name. The word count starts from the
/// link's use-type (a public link gets one word; a limited link gets four for
/// entropy) and grows by one after every `COLLISION_GROW_AT` unique-name
/// collisions, so a crowded namespace still resolves quickly.
pub async fn insert_link(pool: &SqlitePool, link: NewLink<'_>) -> Result<InsertedLink, sqlx::Error> {
    /// Grow the name by a word after this many collisions in a row.
    const COLLISION_GROW_AT: u32 = 8;

    let mut words = words_for(link.max_uses, link.private);
    let mut collisions: u32 = 0;

    loop {
        let name = generate_name(words);
        let result: Result<String, sqlx::Error> = sqlx::query_scalar(
            "INSERT INTO links (name, kind, content, content_type, expires_at, max_uses, delete_token) \
             VALUES (?, ?, ?, ?, datetime('now', '+' || ? || ' seconds'), ?, ?) \
             RETURNING expires_at",
        )
        .bind(&name)
        .bind(link.kind)
        .bind(link.content)
        .bind(link.content_type)
        .bind(link.ttl_seconds)
        .bind(link.max_uses)
        .bind(link.delete_token)
        .fetch_one(pool)
        .await;

        match result {
            Ok(expires_at) => return Ok(InsertedLink { name, expires_at }),
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
        let l = insert_link(&pool, redirect("https://example.com", None)).await.unwrap();
        // Reading twice must not move the hit counter.
        assert!(get_link_live(&pool, &l.name).await.unwrap().is_some());
        let d = get_link_live(&pool, &l.name).await.unwrap().unwrap();
        assert_eq!(d.hits, 0);
    }

    #[tokio::test]
    async fn one_time_consume_then_tombstone() {
        let pool = test_pool().await;
        let l = insert_link(&pool, redirect("https://example.com", Some(1))).await.unwrap();

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
        let l = insert_link(&pool, redirect("https://example.com", None)).await.unwrap();

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
        let l = insert_link(&pool, redirect("https://example.com", None)).await.unwrap();
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
        let public = insert_link(&pool, redirect("https://example.com/a", None)).await.unwrap();
        assert!(!public.name.chars().any(|c| c.is_ascii_uppercase()), "{}", public.name);
        // A private unlimited link is four words, so alternating-case adds uppercase.
        let mut nl = redirect("https://example.com/b", None);
        nl.private = true;
        let private = insert_link(&pool, nl).await.unwrap();
        assert!(private.name.chars().any(|c| c.is_ascii_uppercase()), "{}", private.name);
    }
}
