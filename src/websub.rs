use crate::{persist_video, AppState};
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha1::Sha1;
use sqlx::Row;
use tracing::{error, info, warn};

const ATOM_NS: &str = "http://www.w3.org/2005/Atom";
const YT_NS: &str = "http://www.youtube.com/xml/schemas/2015";

#[derive(Debug, Deserialize)]
pub struct Challenge {
    #[serde(rename = "hub.mode")]
    mode: Option<String>,
    #[serde(rename = "hub.challenge")]
    challenge: Option<String>,
    #[serde(rename = "hub.topic")]
    topic: Option<String>,
    #[serde(rename = "hub.verify_token")]
    verify_token: Option<String>,
    #[serde(rename = "hub.lease_seconds")]
    lease_seconds: Option<i64>,
}

pub async fn challenge(State(state): State<AppState>, Query(query): Query<Challenge>) -> impl IntoResponse {
    let (Some(mode), Some(challenge), Some(topic), Some(token)) =
        (query.mode.as_deref(), query.challenge, query.topic, query.verify_token.as_deref())
    else { return (StatusCode::BAD_REQUEST, "invalid WebSub challenge".to_owned()); };

    if !matches!(mode, "subscribe" | "unsubscribe") {
        return (StatusCode::BAD_REQUEST, "unsupported WebSub mode".to_owned());
    }
    let row = sqlx::query("SELECT verify_token, state FROM websub_subscriptions WHERE topic_url=$1")
        .bind(&topic).fetch_optional(&state.db).await;
    let Ok(Some(row)) = row else { return (StatusCode::FORBIDDEN, "unknown topic".into()); };
    let expected: Option<String> = row.try_get("verify_token").ok().flatten();
    let current_state: String = row.try_get("state").unwrap_or_default();
    if expected.as_deref() != Some(token) {
        return (StatusCode::FORBIDDEN, "verification token mismatch".into());
    }
    if (mode == "subscribe" && !matches!(current_state.as_str(), "pending" | "active"))
        || (mode == "unsubscribe" && current_state != "unsubscribing") {
        return (StatusCode::GONE, "subscription state no longer matches challenge".into());
    }
    if mode == "subscribe" {
        let lease = query.lease_seconds.unwrap_or(86400).clamp(60, 31_536_000);
        if let Err(e) = sqlx::query("UPDATE websub_subscriptions SET state='active', lease_expires_at=NOW()+($2 * INTERVAL '1 second'), last_error=NULL, updated_at=NOW() WHERE topic_url=$1 AND state IN ('pending','active')")
            .bind(&topic).bind(lease).execute(&state.db).await {
            error!(error=%e,"failed to activate WebSub subscription");
            return (StatusCode::SERVICE_UNAVAILABLE, "database error".into());
        }
    } else {
        if let Err(e) = sqlx::query("DELETE FROM websub_subscriptions WHERE topic_url=$1 AND state='unsubscribing'")
            .bind(&topic).execute(&state.db).await {
            error!(error=%e,"failed to remove WebSub subscription");
            return (StatusCode::SERVICE_UNAVAILABLE, "database error".into());
        }
    }
    info!(mode, "WebSub challenge verified");
    (StatusCode::OK, challenge)
}

pub async fn notification(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    info!(bytes = body.len(), "WebSub HTTP notification received");
    if body.is_empty() {
        warn!("WebSub notification has an empty body");
        return StatusCode::BAD_REQUEST;
    }
    let xml = match std::str::from_utf8(&body) { Ok(v) => v, Err(_) => return StatusCode::BAD_REQUEST };
    let videos = match parse_atom_entries(xml) {
        Ok(v) => v,
        Err(e) => { warn!(error=%e,"invalid YouTube Atom notification"); return StatusCode::BAD_REQUEST; }
    };
    if videos.is_empty() {
        info!("WebSub notification contains no video entries");
        return StatusCode::NO_CONTENT;
    }

    let channel_id = videos[0].channel_id.clone();
    if videos.iter().any(|video| video.channel_id != channel_id) {
        warn!("WebSub batch contains more than one channel");
        return StatusCode::BAD_REQUEST;
    }
    let row = sqlx::query("SELECT secret FROM websub_subscriptions WHERE channel_id=$1 AND state='active'")
        .bind(&channel_id).fetch_optional(&state.db).await;
    let Ok(Some(row)) = row else {
        warn!(channel_id, "notification for unsubscribed channel");
        return StatusCode::FORBIDDEN;
    };
    let secret: Option<String> = row.try_get("secret").ok().flatten();
    let Some(secret) = secret else {
        warn!(channel_id, "active subscription has no secret");
        return StatusCode::UNAUTHORIZED;
    };
    if !verify_signature(&headers, &body, &secret) {
        warn!(channel_id, "invalid WebSub HMAC signature");
        return StatusCode::UNAUTHORIZED;
    }
    for video in videos {
        info!(channel_id = %video.channel_id, video_id = %video.video_id, title = %video.title, "WebSub video notification accepted");
        if let Err(e) = persist_video(&state, video, "websub", true).await {
            error!(error=%e, "failed to persist WebSub event");
            return StatusCode::SERVICE_UNAVAILABLE;
        }
    }
    StatusCode::NO_CONTENT
}

fn verify_signature(headers: &HeaderMap, body: &[u8], secret: &str) -> bool {
    let Some(signature) = headers.get("x-hub-signature").and_then(|v| v.to_str().ok()) else { return false };
    let Some(hex_signature) = signature.strip_prefix("sha1=") else { return false };
    let Ok(expected) = hex::decode(hex_signature) else { return false };
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(secret.as_bytes()) else { return false };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

#[derive(Debug, Clone)]
pub struct VideoEntry {
    pub video_id: String,
    pub channel_id: String,
    pub channel_title: Option<String>,
    pub title: String,
    pub published_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

pub fn parse_atom_entries(xml: &str) -> anyhow::Result<Vec<VideoEntry>> {
    let document = roxmltree::Document::parse(xml)?;
    let mut entries = Vec::new();
    for entry in document.descendants().filter(|n| n.has_tag_name((ATOM_NS, "entry"))) {
        let text = |namespace: &str, name: &str| entry.descendants()
            .find(|n| n.has_tag_name((namespace, name)))
            .and_then(|n| n.text()).map(str::to_owned);
        let video_id = text(YT_NS, "videoId").ok_or_else(|| anyhow::anyhow!("entry missing yt:videoId"))?;
        let channel_id = text(YT_NS, "channelId").ok_or_else(|| anyhow::anyhow!("entry missing yt:channelId"))?;
        let title = text(ATOM_NS, "title").ok_or_else(|| anyhow::anyhow!("entry missing atom:title"))?;
        let date = |name: &str| text(ATOM_NS, name)
            .and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
            .map(|v| v.with_timezone(&Utc));
        entries.push(VideoEntry {
            video_id, channel_id, channel_title: text(YT_NS, "channelTitle"),
            title, published_at: date("published"), updated_at: date("updated"),
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_youtube_atom_entry() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:yt="http://www.youtube.com/xml/schemas/2015"><entry><yt:videoId>abc123</yt:videoId><yt:channelId>UC1234567890</yt:channelId><title>Test video</title><published>2026-11-13T10:00:00+00:00</published><updated>2026-11-13T10:00:00+00:00</updated><yt:channelTitle>Example channel</yt:channelTitle></entry></feed>"#;
        let entries = parse_atom_entries(xml).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].video_id, "abc123");
    }

    #[test]
    fn rejects_empty_or_invalid_xml() {
        assert!(parse_atom_entries("").is_err());
        assert!(parse_atom_entries("<feed>").is_err());
    }

    #[test]
    fn verifies_hmac_sha1_signature() {
        let body = b"hello";
        let secret = "test-secret";
        let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let signature = format!("sha1={}", hex::encode(mac.finalize().into_bytes()));
        let mut headers = HeaderMap::new();
        headers.insert("x-hub-signature", signature.parse().unwrap());
        assert!(verify_signature(&headers, body, secret));
        assert!(!verify_signature(&headers, b"changed", secret));
    }
}
