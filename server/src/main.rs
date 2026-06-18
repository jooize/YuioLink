mod config;
mod db;
mod error;
mod views;
mod web;

use axum::Router;
use axum::routing::{get, post};
use std::sync::Arc;
use tokio::net::TcpListener;
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
        "starting YuioLink server"
    );

    let pool = db::connect(&config.db_path).await?;

    let state = AppState {
        pool,
        base_url: Arc::from(config.base_url.as_str()),
    };

    let app = Router::new()
        .route("/", get(web::index).post(web::js_required))
        .route("/static/app.css", get(web::app_css))
        .route("/static/crypto.js", get(web::crypto_js))
        .route("/static/app.js", get(web::app_js))
        .route("/static/redirect.js", get(web::redirect_js))
        .route("/api/v1/links", post(web::api_create_link))
        .route("/:name", get(web::resolve))
        .fallback(fallback)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = TcpListener::bind(&config.bind).await?;
    tracing::info!(addr = %config.bind, "listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn fallback() -> error::AppError {
    error::AppError::NotFound
}
