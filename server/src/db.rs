//! SQLite access via a shared pool (created once, not per-request like the
//! original Go handlers).

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

use yuiolink_core::{DEFAULT_NAME_LEN, generate_name};

#[derive(sqlx::FromRow)]
pub struct LinkDetail {
    pub name: String,
    pub kind: String,
    pub content: String,
    // Read but unused until paste viewing lands.
    #[allow(dead_code)]
    pub content_type: Option<String>,
    pub encrypted: bool,
    pub hits: i64,
    pub created_at: String,
}

pub async fn connect(db_path: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub async fn get_link(pool: &SqlitePool, name: &str) -> Result<Option<LinkDetail>, sqlx::Error> {
    sqlx::query_as::<_, LinkDetail>(
        "SELECT name, kind, content, content_type, encrypted, hits, created_at FROM links WHERE name = ?",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
}

pub async fn bump_hits(pool: &SqlitePool, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE links SET hits = hits + 1 WHERE name = ?")
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}

/// Insert a row under a freshly generated name, retrying on the (vanishingly
/// rare) unique-name collision.
pub async fn insert_unique(
    pool: &SqlitePool,
    kind: &str,
    content: &str,
    content_type: Option<&str>,
    encrypted: bool,
) -> Result<String, sqlx::Error> {
    loop {
        let name = generate_name(DEFAULT_NAME_LEN);
        let result = sqlx::query(
            "INSERT INTO links (name, kind, content, content_type, encrypted) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&name)
        .bind(kind)
        .bind(content)
        .bind(content_type)
        .bind(encrypted)
        .execute(pool)
        .await;

        match result {
            Ok(_) => return Ok(name),
            Err(sqlx::Error::Database(db)) if db.is_unique_violation() => continue,
            Err(e) => return Err(e),
        }
    }
}
