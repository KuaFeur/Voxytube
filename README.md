# VoxyTube

VoxyTube is an independent Rust service that receives YouTube upload notifications and delivers normalized events to a configured Voxy webhook. It does not connect to Discord itself.

## V1 scope

- WebSub subscriptions with verification challenges, lease renewal, and HMAC-SHA1 validation.
- RSS reconciliation using YouTube channel feeds, conditional requests (ETag / Last-Modified), and bounded retry backoff.
- PostgreSQL-backed deduplication and durable event outbox.
- Authenticated channel management API.
- Retried event delivery with an idempotency key.
- One Pelican/Pterodactyl container containing the Rust binary and PostgreSQL.

No YouTube page scraping, yt-dlp, or YouTube Data API key is used.

## One-container deployment

The image contains VoxyTube (Rust) and PostgreSQL. PostgreSQL binds only to container loopback; it is not a separately exposed service. Persistent state lives in `/home/container/data`.

1. Import [the Pelican Egg](deploy/pelican/egg-voxytube.json).
2. Create a server from the Egg and allocate its HTTP port (8080 by default).
3. Set `PUBLIC_CALLBACK_URL` to the public HTTPS URL ending in `/websub`.
4. Set `API_TOKEN` to a random value of at least 32 characters.
5. Set `VOXY_WEBHOOK_URL` to the endpoint that Voxy will expose to receive event JSON. Optionally set `VOXY_WEBHOOK_TOKEN`.
6. Start the server and keep `/home/container/data` persistent.
7. Route the public HTTPS callback to the allocated HTTP port using Nginx or Cloudflare.

The image is built and published to `ghcr.io/kuafeur/voxytube:latest` by GitHub Actions. PostgreSQL credentials are generated on first boot if no password is supplied. Never expose port 5432 publicly.

## Management API

All management endpoints require `Authorization: Bearer <API_TOKEN>`.

- `POST /api/v1/channels` — JSON body: `{"channel_id":"UC..."}`; starts an asynchronous WebSub subscription.
- `GET /api/v1/channels` — lists configured channels and their subscription state.
- `DELETE /api/v1/channels/{channel_id}` — requests an asynchronous unsubscribe; the hub removes the subscription after its verification challenge.
- `GET /healthz` and `GET /readyz` — database health checks.
- `GET /websub` and `POST /websub` — public WebSub callback.

Example:

```bash
curl -X POST http://127.0.0.1:8080/api/v1/channels \
  -H "Authorization: Bearer $API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"channel_id":"UCxxxxxxxxxxxxxxxxxxxxxx"}'
```

## Event contract

A successful delivery is a JSON object:

```json
{
  "event": "youtube.video.published",
  "video_id": "VIDEO_ID",
  "channel_id": "CHANNEL_ID",
  "channel_title": "Channel name",
  "title": "Video title",
  "url": "https://www.youtube.com/watch?v=VIDEO_ID",
  "published_at": "2026-11-13T10:00:00Z",
  "source": "websub"
}
```

VoxyTube sends `Idempotency-Key: voxytube-<outbox-id>`. The receiving endpoint should honor this key to avoid duplicate side effects if a network timeout occurs after the receiver accepted a request.

## Local development

Use Rust stable (edition 2024) and PostgreSQL 14+. Create a database, copy `.env.example` to `.env`, set `DATABASE_URL`, `PUBLIC_CALLBACK_URL` and `API_TOKEN`, then run `cargo run`.

## Stability notice

The V1 implementation is under active validation. A release must not be considered stable until CI builds and tests pass, the container is exercised on supported architectures, and an end-to-end WebSub/RSS/outbox test has been completed. YouTube and hub delivery behavior can change independently of VoxyTube.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
