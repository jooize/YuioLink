mod card;
mod config;
mod db;
mod emoji;
mod error;
mod joining;
mod legal;
mod licenses;
mod phone;
mod qr;
mod ratelimit;
mod security;
mod token;
mod urlview;
mod variation;
mod views;
mod web;

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use web::AppState;

const USAGE: &str = "\
usage: yuiolink-server             serve; settings come from YUIOLINK_* variables
       yuiolink-server --license   print the license texts of everything it includes
       yuiolink-server --help      print this
";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Settings come from YUIOLINK_* variables (see config.rs); the command line
    // has two options, so anything else on it is refused rather than ignored.
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => {}
        [flag] if flag == "--license" || flag == "--licenses" => {
            // A reader that stops early (`| head`) is not an error.
            return match std::io::stdout().write_all(licenses::text().as_bytes()) {
                Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => Err(e.into()),
                _ => Ok(()),
            };
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print!("{USAGE}");
            return Ok(());
        }
        _ => {
            eprint!("{USAGE}");
            std::process::exit(2);
        }
    }

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

    // Live name count per word-tier. The create path reads it to give a public
    // link the shortest name still available; the reaper refreshes it each sweep.
    // Seed it once before serving so the first creates see real occupancy.
    let occupancy: Arc<[AtomicU64; 4]> = Arc::new(std::array::from_fn(|_| AtomicU64::new(0)));
    if let Ok(counts) = db::live_counts_by_words(&pool).await {
        for (slot, n) in occupancy.iter().zip(counts) {
            slot.store(n, Ordering::Relaxed);
        }
    }

    let state = AppState {
        pool: pool.clone(),
        base_url: Arc::from(config.base_url.as_str()),
        max_ttl_secs: config.max_ttl_secs,
        secret: config.secret.clone(),
        occupancy: occupancy.clone(),
        limiter: Arc::new(ratelimit::RateLimiter::new()),
    };

    spawn_reaper(pool, config.reap_interval_secs, occupancy);

    let app = web::router(state).layer(TraceLayer::new_for_http());

    let listener = TcpListener::bind(&config.bind).await?;
    tracing::info!(addr = %config.bind, "listening");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Periodically delete expired rows, recycling their names back into the
/// namespace, then refresh the per-tier occupancy the create path reads. A failed
/// sweep is logged and retried on the next tick.
fn spawn_reaper(pool: sqlx::SqlitePool, interval_secs: u64, occupancy: Arc<[AtomicU64; 4]>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            match db::reap_expired(&pool).await {
                Ok(n) if n > 0 => tracing::info!(reaped = n, "recycled expired links"),
                Ok(_) => {}
                Err(e) => tracing::error!(error = %e, "reaper sweep failed"),
            }
            match db::live_counts_by_words(&pool).await {
                Ok(counts) => {
                    for (slot, n) in occupancy.iter().zip(counts) {
                        slot.store(n, Ordering::Relaxed);
                    }
                }
                Err(e) => tracing::error!(error = %e, "occupancy refresh failed"),
            }
        }
    });
}
