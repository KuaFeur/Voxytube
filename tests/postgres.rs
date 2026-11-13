use sqlx::{postgres::PgPoolOptions, Row};
use uuid::Uuid;

#[tokio::test]
async fn migrations_and_outbox_are_idempotent() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point to the CI PostgreSQL service");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to PostgreSQL");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("apply VoxyTube migrations");

    let video_id = format!("v1test{}", Uuid::new_v4().simple());
    let channel_id = "UC1234567890123456789012";

    let inserted = sqlx::query(
        "INSERT INTO youtube_videos (video_id, channel_id, title) VALUES ($1,$2,$3) ON CONFLICT (video_id) DO NOTHING"
    )
    .bind(&video_id)
    .bind(channel_id)
    .bind("VoxyTube database integration test")
    .execute(&pool)
    .await
    .expect("insert video");
    assert_eq!(inserted.rows_affected(), 1);

    let payload = serde_json::json!({
        "event": "youtube.video.published",
        "video_id": video_id,
        "channel_id": channel_id,
        "title": "VoxyTube database integration test"
    });
    sqlx::query("INSERT INTO event_outbox (video_id, payload) VALUES ($1,$2)")
        .bind(&video_id)
        .bind(payload)
        .execute(&pool)
        .await
        .expect("enqueue event");

    let duplicate = sqlx::query(
        "INSERT INTO youtube_videos (video_id, channel_id, title) VALUES ($1,$2,$3) ON CONFLICT (video_id) DO NOTHING"
    )
    .bind(&video_id)
    .bind(channel_id)
    .bind("duplicate")
    .execute(&pool)
    .await
    .expect("deduplicate video");
    assert_eq!(duplicate.rows_affected(), 0);

    let outbox_count: i64 = sqlx::query("SELECT COUNT(*) AS count FROM event_outbox WHERE video_id=$1")
        .bind(&video_id)
        .fetch_one(&pool)
        .await
        .expect("read outbox")
        .try_get("count")
        .expect("count column");
    assert_eq!(outbox_count, 1);

    sqlx::query("UPDATE event_outbox SET status='dead', attempts=20 WHERE video_id=$1")
        .bind(&video_id)
        .execute(&pool)
        .await
        .expect("mark event dead");
    let retried = sqlx::query("UPDATE event_outbox SET status='pending', attempts=0, next_attempt_at=NOW(), locked_at=NULL, last_error=NULL WHERE video_id=$1 AND status='dead'")
        .bind(&video_id)
        .execute(&pool)
        .await
        .expect("requeue dead event");
    assert_eq!(retried.rows_affected(), 1);

    let channel_id = format!("UC{}", Uuid::new_v4().simple());
    let topic = format!("https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}");
    sqlx::query("INSERT INTO websub_subscriptions (channel_id, topic_url, callback_url, verify_token) VALUES ($1,$2,$3,$4)")
        .bind(&channel_id)
        .bind(&topic)
        .bind("https://example.test/websub")
        .bind(Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .expect("create subscription");
    let unsubscribing = sqlx::query("UPDATE websub_subscriptions SET state='unsubscribing' WHERE channel_id=$1")
        .bind(&channel_id)
        .execute(&pool)
        .await
        .expect("allow retryable unsubscribe state");
    assert_eq!(unsubscribing.rows_affected(), 1);
    let invalid_state = sqlx::query("UPDATE websub_subscriptions SET state='invalid' WHERE channel_id=$1")
        .bind(&channel_id)
        .execute(&pool)
        .await;
    assert!(invalid_state.is_err(), "subscription state constraint must reject unknown states");

    sqlx::query("DELETE FROM websub_subscriptions WHERE channel_id=$1")
        .bind(&channel_id)
        .execute(&pool)
        .await
        .expect("cleanup subscription");
    sqlx::query("DELETE FROM youtube_videos WHERE video_id=$1")
        .bind(video_id)
        .execute(&pool)
        .await
        .expect("cleanup test data");
}
