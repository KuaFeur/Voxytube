use crate::AppState;
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct Challenge {
    #[serde(rename = "hub.mode")]
    mode: Option<String>,
    #[serde(rename = "hub.challenge")]
    challenge: Option<String>,
    #[serde(rename = "hub.topic")]
    topic: Option<String>,
}

pub async fn challenge(Query(query): Query<Challenge>) -> impl IntoResponse {
    match (query.mode.as_deref(), query.challenge) {
        (Some("subscribe" | "unsubscribe"), Some(challenge)) => {
            info!(mode = ?query.mode, topic = ?query.topic, "WebSub subscription challenge received");
            (StatusCode::OK, challenge)
        }
        _ => (StatusCode::BAD_REQUEST, "invalid WebSub challenge".to_owned()),
    }
}

pub async fn notification(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    if body.is_empty() {
        return StatusCode::BAD_REQUEST;
    }

    let xml = match std::str::from_utf8(&body) {
        Ok(xml) => xml,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let videos = match parse_atom_entries(xml) {
        Ok(videos) => videos,
        Err(err) => {
            warn!(error = %err, "invalid YouTube Atom notification");
            return StatusCode::BAD_REQUEST;
        }
    };

    let delivery_id = Uuid::new_v4();
    let signature_present = headers.contains_key("x-hub-signature");
    info!(%delivery_id, count = videos.len(), signature_present, "WebSub delivery received");

    // Persist before acknowledging the hub. The unique video ID makes retries idempotent.
    for video in videos {
        let result = sqlx::query(
            r#"INSERT INTO youtube_videos
               (video_id, channel_id, channel_title, title, published_at, updated_at, first_seen_at)
               VALUES ($1, $2, $3, $4, $5, $6, NOW())
               ON CONFLICT (video_id) DO NOTHING"#,
        )
        .bind(&video.video_id)
        .bind(&video.channel_id)
        .bind(&video.channel_title)
        .bind(&video.title)
        .bind(video.published_at)
        .bind(video.updated_at)
        .execute(&state.db)
        .await;

        if let Err(err) = result {
            error!(video_id = %video.video_id, error = %err, "failed to persist YouTube notification");
            return StatusCode::SERVICE_UNAVAILABLE;
        }
    }

    StatusCode::NO_CONTENT
}

#[derive(Debug)]
struct VideoEntry {
    video_id: String,
    channel_id: String,
    channel_title: Option<String>,
    title: String,
    published_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
}

fn parse_atom_entries(xml: &str) -> anyhow::Result<Vec<VideoEntry>> {
    let document = roxmltree::Document::parse(xml)?;
    let mut entries = Vec::new();

    for entry in document.descendants().filter(|node| node.has_tag_name(("http://www.w3.org/2005/Atom", "entry"))) {
        let text = |namespace: &str, name: &str| {
            entry.descendants()
                .find(|node| node.has_tag_name((namespace, name)))
                .and_then(|node| node.text())
                .map(str::to_owned)
        };

        let video_id = text("http://www.youtube.com/xml/schemas/2015", "videoId")
            .ok_or_else(|| anyhow::anyhow!("entry is missing yt:videoId"))?;
        let channel_id = text("http://www.youtube.com/xml/schemas/2015", "channelId")
            .ok_or_else(|| anyhow::anyhow!("entry is missing yt:channelId"))?;
        let title = text("http://www.w3.org/2005/Atom", "title")
            .ok_or_else(|| anyhow::anyhow!("entry is missing atom:title"))?;

        let parse_date = |name: &str| -> Option<DateTime<Utc>> {
            text("http://www.w3.org/2005/Atom", name)
                .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
                .map(|value| value.with_timezone(&Utc))
        };

        entries.push(VideoEntry {
            video_id,
            channel_id,
            channel_title: text("http://www.youtube.com/xml/schemas/2015", "channelTitle"),
            title,
            published_at: parse_date("published"),
            updated_at: parse_date("updated"),
        });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_youtube_atom_entry() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:yt="http://www.youtube.com/xml/schemas/2015">
          <entry>
            <yt:videoId>abc123</yt:videoId>
            <yt:channelId>UC123</yt:channelId>
            <title>Test video</title>
            <published>2026-11-13T10:00:00+00:00</published>
            <updated>2026-11-13T10:00:00+00:00</updated>
            <yt:channelTitle>Example channel</yt:channelTitle>
          </entry>
        </feed>"#;

        let entries = parse_atom_entries(xml).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].video_id, "abc123");
        assert_eq!(entries[0].title, "Test video");
    }
}
