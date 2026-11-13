mod settings;
mod websub;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::Utc;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, FromRow};
use std::{net::SocketAddr, time::Duration};
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: settings::Settings,
    pub http: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoEvent {
    pub event: String,
    pub video_id: String,
    pub channel_id: String,
    pub channel_title: Option<String>,
    pub title: String,
    pub url: String,
    pub published_at: Option<chrono::DateTime<Utc>>,
    pub source: String,
}

#[derive(Debug, Deserialize)]
struct AddChannel {
    channel_id: String,
}

#[derive(Debug, Serialize, FromRow)]
struct Channel {
    channel_id: String,
    topic_url: String,
    state: String,
    lease_expires_at: Option<chrono::DateTime<Utc>>,
    created_at: chrono::DateTime<Utc>,
    last_error: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("voxytube=info,tower_http=info")))
        .init();

    let config = settings::Settings::from_env()?;
    let db = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .connect(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&db).await?;

    let http = reqwest::Client::builder()
        .user_agent(concat!("VoxyTube/", env!("CARGO_PKG_VERSION"), " (+https://github.com/KuaFeur/Voxytube)"))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(25))
        .build()?;

    let state = AppState { db, config, http };
    let app = Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(health))
        .route("/websub", get(websub::challenge).post(websub::notification))
        .route("/api/v1/channels", get(list_channels).post(add_channel))
        .route("/api/v1/channels/{channel_id}", delete(remove_channel))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/events/{event_id}/retry", post(retry_event))
        .with_state(state.clone())
        .layer(RequestBodyLimitLayer::new(256 * 1024))
        .layer(TraceLayer::new_for_http());

    tokio::spawn(subscription_worker(state.clone()));
    tokio::spawn(rss_worker(state.clone()));
    tokio::spawn(delivery_worker(state.clone()));

    let address = SocketAddr::from(([0, 0, 0, 0], state.config.port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    info!(%address, "VoxyTube HTTP server started");
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}

fn authorized(state: &AppState, headers: &axum::http::HeaderMap) -> bool {
    use subtle::ConstantTimeEq;
    let Some(value) = headers.get(axum::http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()) else { return false };
    let Some(token) = value.strip_prefix("Bearer ") else { return false };
    token.as_bytes().ct_eq(state.config.api_token.as_bytes()).into()
}

async fn health(State(state): State<AppState>) -> Result<(StatusCode, &'static str), StatusCode> {
    sqlx::query("SELECT 1").execute(&state.db).await
        .map(|_| (StatusCode::OK, "ok"))
        .map_err(|err| { error!(error = %err, "database health check failed"); StatusCode::SERVICE_UNAVAILABLE })
}

async fn add_channel(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(input): Json<AddChannel>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    if !authorized(&state, &headers) { return Err(StatusCode::UNAUTHORIZED); }
    let channel_id = input.channel_id.trim();
    if !channel_id.starts_with("UC") || channel_id.len() < 10 || channel_id.len() > 64
        || !channel_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut random = [0u8; 32];
    OsRng.fill_bytes(&mut random);
    let secret = hex::encode(random);
    let mut verify = [0u8; 24];
    OsRng.fill_bytes(&mut verify);
    let verify_token = hex::encode(verify);
    let topic = format!("https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}");

    sqlx::query(
        "INSERT INTO websub_subscriptions (channel_id, topic_url, callback_url, secret, verify_token, state, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,'pending',NOW(),NOW()) ON CONFLICT (channel_id) DO UPDATE SET topic_url=EXCLUDED.topic_url, callback_url=EXCLUDED.callback_url, secret=EXCLUDED.secret, verify_token=EXCLUDED.verify_token, state='pending', lease_expires_at=NULL, updated_at=NOW(), last_error=NULL"
    ).bind(channel_id).bind(&topic).bind(&state.config.callback_url).bind(&secret).bind(&verify_token)
     .execute(&state.db).await.map_err(|e| { error!(error=%e,"failed to store channel"); StatusCode::INTERNAL_SERVER_ERROR })?;

    request_subscription(&state, channel_id, "subscribe").await;
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({
        "channel_id": channel_id, "state": "pending",
        "topic_url": topic, "message": "Subscription requested; WebSub verification is asynchronous."
    }))))
}

async fn list_channels(State(state): State<AppState>, headers: axum::http::HeaderMap)
    -> Result<Json<Vec<Channel>>, StatusCode> {
    if !authorized(&state, &headers) { return Err(StatusCode::UNAUTHORIZED); }
    let rows = sqlx::query_as::<_, Channel>(
        "SELECT channel_id, topic_url, state, lease_expires_at, created_at, last_error FROM websub_subscriptions ORDER BY created_at"
    ).fetch_all(&state.db).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows))
}

async fn remove_channel(
    State(state): State<AppState>, headers: axum::http::HeaderMap,
    axum::extract::Path(channel_id): axum::extract::Path<String>,
) -> Result<StatusCode, StatusCode> {
    if !authorized(&state, &headers) { return Err(StatusCode::UNAUTHORIZED); }
    let result = sqlx::query("UPDATE websub_subscriptions SET state='unsubscribing', updated_at=NOW(), last_error=NULL WHERE channel_id=$1")
        .bind(&channel_id).execute(&state.db).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 { return Ok(StatusCode::NO_CONTENT); }
    request_subscription(&state, &channel_id, "unsubscribe").await;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Serialize, FromRow)]
struct EventRecord {
    id: i64,
    video_id: Option<String>,
    title: Option<String>,
    status: String,
    attempts: i32,
    next_attempt_at: chrono::DateTime<Utc>,
    last_error: Option<String>,
    created_at: chrono::DateTime<Utc>,
    delivered_at: Option<chrono::DateTime<Utc>>,
}

async fn list_events(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !authorized(&state, &headers) { return Err(StatusCode::UNAUTHORIZED); }
    let rows = sqlx::query_as::<_, EventRecord>(
        "SELECT id, payload->>'video_id' AS video_id, payload->>'title' AS title, status, attempts, next_attempt_at, last_error, created_at, delivered_at FROM event_outbox ORDER BY id DESC LIMIT 100"
    ).fetch_all(&state.db).await
        .map_err(|err| { error!(error=%err, "failed to query recent event records"); StatusCode::INTERNAL_SERVER_ERROR })?;
    let counts = sqlx::query("SELECT status, COUNT(*)::BIGINT AS count FROM event_outbox GROUP BY status")
        .fetch_all(&state.db).await
        .map_err(|err| { error!(error=%err, "failed to query event counts"); StatusCode::INTERNAL_SERVER_ERROR })?;
    let mut summary = serde_json::Map::new();
    for row in counts {
        let status: String = sqlx::Row::try_get(&row, "status").unwrap_or_default();
        let count: i64 = sqlx::Row::try_get(&row, "count").unwrap_or_default();
        summary.insert(status, serde_json::json!(count));
    }
    Ok(Json(serde_json::json!({"counts": summary, "recent": rows})))
}

async fn retry_event(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(event_id): axum::extract::Path<i64>,
) -> Result<StatusCode, StatusCode> {
    if !authorized(&state, &headers) { return Err(StatusCode::UNAUTHORIZED); }
    let result = sqlx::query("UPDATE event_outbox SET status='pending', attempts=0, next_attempt_at=NOW(), locked_at=NULL, last_error=NULL WHERE id=$1 AND status='dead'")
        .bind(event_id).execute(&state.db).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 { return Err(StatusCode::NOT_FOUND); }
    Ok(StatusCode::ACCEPTED)
}

async fn request_subscription(state: &AppState, channel_id: &str, mode: &str) {
    let row = sqlx::query("SELECT topic_url, secret, verify_token FROM websub_subscriptions WHERE channel_id=$1")
        .bind(channel_id).fetch_optional(&state.db).await;
    let Ok(Some(row)) = row else { return };
    let topic: String = sqlx::Row::try_get(&row, "topic_url").unwrap_or_default();
    let secret: String = sqlx::Row::try_get(&row, "secret").unwrap_or_default();
    let verify: String = sqlx::Row::try_get(&row, "verify_token").unwrap_or_default();
    let response = state.http.post(&state.config.hub_url).form(&[
        ("hub.callback", state.config.callback_url.as_str()),
        ("hub.mode", mode),
        ("hub.topic", topic.as_str()),
        ("hub.verify", "async"),
        ("hub.verify_token", verify.as_str()),
        ("hub.lease_seconds", "864000"),
        ("hub.secret", secret.as_str()),
    ]).send().await;
    match response {
        Ok(r) if r.status().is_success() => { let _ = sqlx::query("UPDATE websub_subscriptions SET updated_at=NOW(), last_error=NULL WHERE channel_id=$1").bind(channel_id).execute(&state.db).await; info!(channel_id, mode, "WebSub request accepted by hub"); },
        Ok(r) => {
            let msg = format!("hub returned HTTP {}", r.status());
            warn!(channel_id, %msg, "WebSub request failed");
            let _ = sqlx::query("UPDATE websub_subscriptions SET last_error=$2, updated_at=NOW() WHERE channel_id=$1")
                .bind(channel_id).bind(msg).execute(&state.db).await;
        }
        Err(e) => {
            warn!(channel_id, error=%e, "WebSub request failed");
            let _ = sqlx::query("UPDATE websub_subscriptions SET last_error=$2, updated_at=NOW() WHERE channel_id=$1")
                .bind(channel_id).bind(e.to_string()).execute(&state.db).await;
        }
    }
}

async fn subscription_worker(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        let rows = sqlx::query("SELECT channel_id, state FROM websub_subscriptions WHERE state IN ('pending','active','unsubscribing') AND (state='unsubscribing' OR lease_expires_at IS NULL OR lease_expires_at < NOW() + INTERVAL '12 hours') AND updated_at < NOW() - INTERVAL '4 minutes'")
            .fetch_all(&state.db).await;
        match rows {
            Ok(rows) => for row in rows {
                let id: String = sqlx::Row::try_get(&row, "channel_id").unwrap_or_default();
                let current_state: String = sqlx::Row::try_get(&row, "state").unwrap_or_default();
                let mode = if current_state == "unsubscribing" { "unsubscribe" } else { "subscribe" };
                request_subscription(&state, &id, mode).await;
            },
            Err(e) => error!(error=%e,"subscription renewal query failed"),
        }
    }
}

async fn rss_worker(state: AppState) {
    let interval_seconds = state.config.rss_interval_seconds.max(60);
    // Check eligibility more frequently than the per-channel polling interval.
    // A scheduler ticking at the same cadence as RSS_INTERVAL_SECONDS can miss
    // due rows when next_poll_at falls just after a scheduler tick.
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        interval.tick().await;
        let rows = sqlx::query("SELECT channel_id, topic_url, etag, last_modified, created_at FROM websub_subscriptions WHERE state IN ('active','pending') AND (next_poll_at IS NULL OR next_poll_at <= NOW())")
            .fetch_all(&state.db).await;
        let rows = match rows {
            Ok(rows) => rows,
            Err(e) => {
                error!(error=%e, "RSS scheduler query failed");
                continue;
            }
        };
        for row in rows {
            let channel_id: String = sqlx::Row::try_get(&row, "channel_id").unwrap_or_default();
            let topic: String = sqlx::Row::try_get(&row, "topic_url").unwrap_or_default();
            let etag: Option<String> = sqlx::Row::try_get(&row, "etag").ok().flatten();
            let modified: Option<String> = sqlx::Row::try_get(&row, "last_modified").ok().flatten();
            let created: chrono::DateTime<Utc> = sqlx::Row::try_get(&row, "created_at").unwrap_or_else(|_| Utc::now());
            let mut req = state.http.get(&topic);
            if let Some(v) = etag.as_deref() { req = req.header(reqwest::header::IF_NONE_MATCH, v); }
            if let Some(v) = modified.as_deref() { req = req.header(reqwest::header::IF_MODIFIED_SINCE, v); }
            match req.send().await {
                Ok(resp) if resp.status() == reqwest::StatusCode::NOT_MODIFIED => {
                    let _ = sqlx::query("UPDATE websub_subscriptions SET poll_failures=0, next_poll_at=NOW()+($2 * INTERVAL '1 second') WHERE channel_id=$1")
                        .bind(&channel_id).bind(interval_seconds as i64).execute(&state.db).await;
                }
                Ok(resp) if resp.status().is_success() => {
                    let new_etag = resp.headers().get(reqwest::header::ETAG).and_then(|v| v.to_str().ok()).map(str::to_owned);
                    let new_modified = resp.headers().get(reqwest::header::LAST_MODIFIED).and_then(|v| v.to_str().ok()).map(str::to_owned);
                    match resp.text().await {
                        Ok(xml) => match websub::parse_atom_entries(&xml) {
                            Ok(videos) => {
                                for video in videos {
                                    if video.channel_id != channel_id { continue; }
                                    let recent = video.published_at.map(|d| d >= created).unwrap_or(false);
                                    if let Err(e) = persist_video(&state, video, "rss", recent).await {
                                        error!(channel_id, error=%e, "RSS event persistence failed");
                                    }
                                }
                                let _ = sqlx::query("UPDATE websub_subscriptions SET etag=COALESCE($2,etag), last_modified=COALESCE($3,last_modified), poll_failures=0, last_error=NULL, next_poll_at=NOW()+($4 * INTERVAL '1 second') WHERE channel_id=$1")
                                    .bind(&channel_id).bind(new_etag).bind(new_modified).bind(interval_seconds as i64).execute(&state.db).await;
                            }
                            Err(e) => {
                                warn!(channel_id, error=%e, "RSS feed parsing failed");
                                let _ = sqlx::query("UPDATE websub_subscriptions SET poll_failures=LEAST(poll_failures+1,10), last_error=$2, next_poll_at=NOW()+LEAST(3600, (30 * power(2, LEAST(poll_failures,7)))::int) * INTERVAL '1 second' WHERE channel_id=$1")
                                    .bind(&channel_id).bind(e.to_string()).execute(&state.db).await;
                            },
                        },
                        Err(e) => {
                            warn!(channel_id, error=%e, "RSS response body failed");
                            let _ = sqlx::query("UPDATE websub_subscriptions SET poll_failures=LEAST(poll_failures+1,10), last_error=$2, next_poll_at=NOW()+LEAST(3600, (30 * power(2, LEAST(poll_failures,7)))::int) * INTERVAL '1 second' WHERE channel_id=$1")
                                .bind(&channel_id).bind(e.to_string()).execute(&state.db).await;
                        },
                    }
                }
                Ok(resp) => {
                    let status = resp.status();
                    let _ = sqlx::query("UPDATE websub_subscriptions SET poll_failures=LEAST(poll_failures+1,10), last_error=$2, next_poll_at=NOW()+LEAST(3600, (30 * power(2, LEAST(poll_failures,7)))::int) * INTERVAL '1 second' WHERE channel_id=$1")
                        .bind(&channel_id).bind(format!("RSS HTTP {status}")).execute(&state.db).await;
                }
                Err(e) => {
                    let _ = sqlx::query("UPDATE websub_subscriptions SET poll_failures=LEAST(poll_failures+1,10), last_error=$2, next_poll_at=NOW()+LEAST(3600, (30 * power(2, LEAST(poll_failures,7)))::int) * INTERVAL '1 second' WHERE channel_id=$1")
                        .bind(&channel_id).bind(e.to_string()).execute(&state.db).await;
                }
            }
        }
    }
}

async fn persist_video(state: &AppState, video: websub::VideoEntry, source: &str, enqueue: bool) -> anyhow::Result<()> {
    let mut tx = state.db.begin().await?;
    let inserted = sqlx::query("INSERT INTO youtube_videos (video_id,channel_id,channel_title,title,published_at,updated_at,first_seen_at) VALUES ($1,$2,$3,$4,$5,$6,NOW()) ON CONFLICT(video_id) DO NOTHING")
        .bind(&video.video_id).bind(&video.channel_id).bind(&video.channel_title)
        .bind(&video.title).bind(video.published_at.clone()).bind(video.updated_at.clone())
        .execute(&mut *tx).await?.rows_affected() > 0;
    if inserted && enqueue {
        let event = VideoEvent {
            event: "youtube.video.published".into(), video_id: video.video_id.clone(),
            channel_id: video.channel_id.clone(), channel_title: video.channel_title.clone(),
            title: video.title.clone(), url: format!("https://www.youtube.com/watch?v={}", video.video_id),
            published_at: video.published_at.clone(), source: source.into(),
        };
        sqlx::query("INSERT INTO event_outbox(video_id,payload) VALUES($1,$2)")
            .bind(&video.video_id).bind(serde_json::to_value(event)?).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn delivery_worker(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(3));
    loop {
        interval.tick().await;
        if state.config.voxy_webhook_url.is_empty() { continue; }
        let row = sqlx::query("WITH candidate AS (SELECT id FROM event_outbox WHERE (status='pending' OR (status='processing' AND locked_at < NOW() - INTERVAL '2 minutes')) AND next_attempt_at<=NOW() ORDER BY id FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE event_outbox SET status='processing', locked_at=NOW(), attempts=attempts+1 FROM candidate WHERE event_outbox.id=candidate.id RETURNING event_outbox.id,event_outbox.payload,event_outbox.attempts")
            .fetch_optional(&state.db).await;
        let Ok(Some(row)) = row else { continue };
        let id: i64 = sqlx::Row::try_get(&row, "id").unwrap_or_default();
        let payload: serde_json::Value = sqlx::Row::try_get(&row, "payload").unwrap_or_default();
        let attempts: i32 = sqlx::Row::try_get(&row, "attempts").unwrap_or(1);
        let mut request = state.http.post(&state.config.voxy_webhook_url)
            .header("Idempotency-Key", format!("voxytube-{id}")).json(&payload);
        if let Some(token) = &state.config.voxy_webhook_token {
            request = request.bearer_auth(token);
        }
        match request.send().await {
            Ok(resp) if resp.status().is_success() => {
                let _ = sqlx::query("UPDATE event_outbox SET status='delivered',delivered_at=NOW(),last_error=NULL WHERE id=$1")
                    .bind(id).execute(&state.db).await;
                info!(outbox_id=id,"event delivered to Voxy");
            }
            Ok(resp) => {
                let delay = (5_i64 * 2_i64.pow(attempts.clamp(1,10) as u32)).min(3600);
                let message = format!("Voxy endpoint returned HTTP {}", resp.status());
                let status = if attempts >= 20 { "dead" } else { "pending" };
                let _ = sqlx::query("UPDATE event_outbox SET status=$2,last_error=$3,next_attempt_at=NOW()+($4 * INTERVAL '1 second') WHERE id=$1")
                    .bind(id).bind(status).bind(message).bind(delay).execute(&state.db).await;
            }
            Err(e) => {
                let delay = (5_i64 * 2_i64.pow(attempts.clamp(1,10) as u32)).min(3600);
                let status = if attempts >= 20 { "dead" } else { "pending" };
                let _ = sqlx::query("UPDATE event_outbox SET status=$2,last_error=$3,next_attempt_at=NOW()+($4 * INTERVAL '1 second') WHERE id=$1")
                    .bind(id).bind(status).bind(e.to_string()).bind(delay).execute(&state.db).await;
            }
        }
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
        } else { let _ = tokio::signal::ctrl_c().await; }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
    info!("shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_youtube_channel_ids() {
        for id in ["UC1234567890", "UCabcdefghij_123"] {
            assert!(id.starts_with("UC") && id.len() >= 10 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        }
        assert!(!"not-a-channel".starts_with("UC"));
    }
}
