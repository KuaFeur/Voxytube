CREATE TABLE IF NOT EXISTS youtube_videos (
    video_id       TEXT PRIMARY KEY,
    channel_id     TEXT NOT NULL,
    channel_title  TEXT,
    title          TEXT NOT NULL,
    published_at   TIMESTAMPTZ,
    updated_at     TIMESTAMPTZ,
    first_seen_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_youtube_videos_channel_published
    ON youtube_videos (channel_id, published_at DESC);

CREATE TABLE IF NOT EXISTS websub_subscriptions (
    channel_id       TEXT PRIMARY KEY,
    topic_url        TEXT NOT NULL,
    callback_url     TEXT NOT NULL,
    secret           TEXT,
    lease_expires_at TIMESTAMPTZ,
    state            TEXT NOT NULL DEFAULT 'pending'
                     CHECK (state IN ('pending', 'active', 'expired', 'failed')),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id          UUID PRIMARY KEY,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    item_count  INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL DEFAULT 'received'
);
