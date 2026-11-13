mod settings;
mod websub;

use axum::{
    Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("voxytube=info,tower_http=info")))
        .init();

    let config = settings::Settings::from_env()?;
    let db = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&db).await?;

    let state = AppState { db };
    let app = Router::new()
        .route("/healthz", get(health))
        .route("/websub", get(websub::challenge).post(websub::notification))
        .with_state(state)
        .layer(RequestBodyLimitLayer::new(256 * 1024))
        .layer(TraceLayer::new_for_http());

    let address = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    info!(%address, "VoxyTube HTTP server started");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn health(State(state): State<AppState>) -> Result<(StatusCode, &'static str), StatusCode> {
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map(|_| (StatusCode::OK, "ok"))
        .map_err(|err| {
            error!(error = %err, "database health check failed");
            StatusCode::SERVICE_UNAVAILABLE
        })
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("shutdown signal received");
}
