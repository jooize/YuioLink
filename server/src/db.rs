//! SQLite access via a shared pool (created once, not per-request like the
//! original Go handlers).

use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

use yuiolink_core::{generate_name, words_for_ttl};

#[derive(sqlx::FromRow)]
pub struct LinkDetail {
    pub name: String,
    pub kind: String,
    pub content: String,
    // Selected so the row maps cleanly; minimal Text only renders plaintext, so
    // nothing reads this yet.
    #[allow(dead_code)]
    pub content_type: Option<String>,
    pub encrypted: bool,
    pub hits: i64,
    pub created_at: String,
    pub expires_at: String,
    pub max_uses: Option<i64>,
}

/// Columns selected for a [`LinkDetail`], in struct order.
const LINK_COLUMNS: &str =
    "name, kind, content, content_type, encrypted, hits, created_at, expires_at, max_uses";

/// A link still resolvable: not past `expires_at` and not over `max_uses`.
const LIVE_PREDICATE: &str =
    "expires_at > datetime('now') AND (max_uses IS NULL OR hits < max_uses)";

/// Fields needed to create a link; the name and `expires_at` are derived here.
pub struct NewLink<'a> {
    pub kind: &'a str,
    pub content: &'a str,
    pub content_type: Option<&'a str>,
    pub encrypted: bool,
    pub ttl_seconds: i64,
    pub max_uses: Option<i64>,
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

/// Read a link without consuming it — for previews and the REST API. Expired or
/// use-exhausted links read as `None`. Lookup is case-insensitive (the `name`
/// column is `COLLATE NOCASE`).
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
/// TTL and grows by one after every `COLLISION_GROW_AT` unique-name collisions,
/// so a crowded short namespace still resolves quickly.
pub async fn insert_link(pool: &SqlitePool, link: NewLink<'_>) -> Result<InsertedLink, sqlx::Error> {
    /// Grow the name by a word after this many collisions in a row.
    const COLLISION_GROW_AT: u32 = 8;

    let mut words = words_for_ttl(Duration::from_secs(link.ttl_seconds.max(0) as u64));
    let mut collisions: u32 = 0;

    loop {
        let name = generate_name(words);
        let result: Result<String, sqlx::Error> = sqlx::query_scalar(
            "INSERT INTO links (name, kind, content, content_type, encrypted, expires_at, max_uses) \
             VALUES (?, ?, ?, ?, ?, datetime('now', '+' || ? || ' seconds'), ?) \
             RETURNING expires_at",
        )
        .bind(&name)
        .bind(link.kind)
        .bind(link.content)
        .bind(link.content_type)
        .bind(link.encrypted)
        .bind(link.ttl_seconds)
        .bind(link.max_uses)
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

/// Delete every expired row, freeing those names for reuse. Returns the count.
pub async fn reap_expired(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM links WHERE expires_at <= datetime('now')")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
