# Architecture and concepts

VoxyTube is a standalone Rust HTTP service. Its purpose is to receive YouTube upload notifications, normalize them into a stable JSON event, persist them, and deliver them to a configured webhook.

It does not connect to Discord and does not implement Discord commands. A Discord bot such as Voxy consumes the webhook and decides where and how to publish notifications.

## Data flow

1. An operator registers a YouTube channel through the authenticated management API.
2. VoxyTube asks the configured WebSub hub to subscribe to that channel's Atom feed.
3. YouTube verifies the public callback with a WebSub challenge.
4. YouTube sends Atom notifications to `POST /websub`. VoxyTube validates the HMAC signature and parses the feed entries.
5. A periodic RSS worker reconciles feeds as a fallback.
6. Video IDs are deduplicated in PostgreSQL. A new eligible video and its outbox event are stored in the same database transaction.
7. The delivery worker sends the event JSON to the configured webhook and retries failures.

## Components

- **Rust / Axum**: HTTP API, WebSub callback and background workers.
- **PostgreSQL**: subscriptions, video deduplication and durable event outbox.
- **WebSub**: primary near-real-time notification mechanism.
- **RSS reconciliation**: fallback polling with conditional requests (ETag and Last-Modified) and backoff.
- **Webhook delivery**: at-least-once delivery with an idempotency key.

The container image bundles the Rust service and PostgreSQL. PostgreSQL is intended to remain private inside the container; only the HTTP port should be exposed.

## Explicit non-goals

VoxyTube does not use YouTube page scraping, yt-dlp, or the YouTube Data API. It does not require a YouTube API key.

## Delivery guarantee

Webhook delivery is **at least once**, not exactly once. A receiver can accept a request while the response is lost, causing VoxyTube to retry. Receivers must deduplicate using the `Idempotency-Key` header before performing non-idempotent side effects.
