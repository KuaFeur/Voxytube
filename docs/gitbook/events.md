# Events and Voxy webhook

VoxyTube stores normalized events in PostgreSQL before attempting webhook delivery. This durable outbox prevents a transient webhook outage from losing an already accepted event.

## Event payload

A video publication event has this shape:

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

- `event` identifies the event type.
- `video_id` is the YouTube video ID.
- `channel_id` identifies the publishing channel.
- `channel_title` may be null when unavailable.
- `title` is the video title.
- `url` is the canonical YouTube watch URL.
- `published_at` may be null when unavailable.
- `source` is `websub` or `rss`, depending on the ingestion path.

## Webhook request

VoxyTube sends an HTTP POST with a JSON body and these headers:

```http
Content-Type: application/json
Idempotency-Key: voxytube-<outbox-id>
Authorization: Bearer <VOXY_WEBHOOK_TOKEN>
```

The Authorization header is included only when `VOXY_WEBHOOK_TOKEN` is configured.

Any 2xx response marks the event as delivered. Non-2xx responses and transport errors are retried. A receiver should validate the bearer token when configured and deduplicate each idempotency key before triggering Discord messages or other side effects.

## Delivery and retries

Delivery is at least once. A timeout can happen after the receiver has processed the request, so duplicate requests are possible. Store processed idempotency keys durably on the receiver side.

The worker checks for pending events every few seconds. Failed attempts use exponential backoff capped at one hour. After 20 failed attempts, an event is marked `dead` and stops automatic delivery.

An event left in `dead` can be requeued with `POST /api/v1/events/{event_id}/retry`.

## Suggested receiver behavior

1. Authenticate the request if a webhook token is configured.
2. Read and validate the event type and required fields.
3. Atomically record the idempotency key if it has not already been processed.
4. Return a 2xx response after the event has been safely accepted.
5. Process Discord publication asynchronously if it may take significant time.

Do not return a 2xx response before the receiver has durably accepted responsibility for the event.
