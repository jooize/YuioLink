mod config;
mod db;
mod error;
mod views;
mod web;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::{Method, header};
use axum::routing::{get, post};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use web::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "yuiolink_server=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::Config::from_env();
    tracing::info!(
        bind = %config.bind,
        base_url = %config.base_url,
        db = %config.db_path,
        max_ttl_secs = config.max_ttl_secs,
        reap_interval_secs = config.reap_interval_secs,
        "starting YuioLink server"
    );

    let pool = db::connect(&config.db_path).await?;

    let state = AppState {
        pool: pool.clone(),
        base_url: Arc::from(config.base_url.as_str()),
        max_ttl_secs: config.max_ttl_secs,
    };

    spawn_reaper(pool, config.reap_interval_secs);

    let app = Router::new()
        .route("/", get(web::index).post(web::js_required))
        .route("/static/app.css", get(web::app_css))
        .route("/static/crypto.js", get(web::crypto_js))
        .route("/static/app.js", get(web::app_js))
        .route("/static/redirect.js", get(web::redirect_js))
        .route("/static/text.js", get(web::text_js))
        .nest("/api/v1", api_routes())
        .route("/create", post(web::create_plain))
        .route("/:name", get(web::resolve))
        .fallback(fallback)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = TcpListener::bind(&config.bind).await?;
    tracing::info!(addr = %config.bind, "listening");
    axum::serve(listener, app).await?;
    Ok(())
}

/// The REST API, with open CORS so a trusted third-party client can run against
/// yuio.link: any origin, no credentials, GET/POST, content-type header only.
fn api_routes() -> Router<AppState> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE]);

    Router::new()
        .route("/links", post(web::api_create_link))
        .route("/links/:name", get(web::api_get_link))
        .layer(cors)
}

/// Periodically delete expired rows, recycling their names back into the
/// namespace. A failed sweep is logged and retried on the next tick.
fn spawn_reaper(pool: sqlx::SqlitePool, interval_secs: u64) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            match db::reap_expired(&pool).await {
                Ok(n) if n > 0 => tracing::info!(reaped = n, "recycled expired links"),
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "reaper sweep failed"),
            }
        }
    });
}

async fn fallback() -> error::AppError {
    error::AppError::NotFound
}
