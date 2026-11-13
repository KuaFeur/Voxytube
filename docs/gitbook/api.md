# REST API

The management API is rooted at `/api/v1`. All management routes require:

```http
Authorization: Bearer YOUR_API_TOKEN
```

The token is configured with `API_TOKEN`. The public health and WebSub callback routes do not use this management bearer token.

## Add a channel

`POST /api/v1/channels`

Request:

```http
Content-Type: application/json
Authorization: Bearer YOUR_API_TOKEN
```

```json
{
  "channel_id": "UCxxxxxxxxxxxxxxxxxxxxxx"
}
```

Example:

```bash
curl -X POST "https://youtube.example.com/api/v1/channels" \
  -H "Authorization: Bearer $API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"channel_id":"UCxxxxxxxxxxxxxxxxxxxxxx"}'
```

A successful request returns `202 Accepted` with the channel ID, initial `pending` state, topic URL and a message explaining that verification is asynchronous. This response means the subscription request was accepted for processing; it does **not** mean YouTube has verified the subscription.

Channel IDs must start with `UC`, be 10–64 characters long, and contain only ASCII letters, digits, underscores or hyphens.

## List channels

`GET /api/v1/channels`

Returns an array containing:

- `channel_id`
- `topic_url`
- `state`
- `lease_expires_at`
- `created_at`
- `last_error`

A channel becomes `active` after a valid WebSub verification challenge is received.

## Remove a channel

`DELETE /api/v1/channels/{channel_id}`

Starts an asynchronous unsubscribe and returns `202 Accepted` when a matching channel exists. If the channel does not exist, it returns `204 No Content`. The row is removed after the hub's unsubscribe challenge is verified.

## List event deliveries

`GET /api/v1/events`

Returns an object with:

- `counts`: number of outbox records grouped by status.
- `recent`: up to the latest 100 event records, including ID, video ID, title, status, attempt count, next attempt time, last error, creation time and delivery time.

## Retry a dead event

`POST /api/v1/events/{event_id}/retry`

Requeues an event only when its current status is `dead`, resets its attempt counter and schedules it immediately. Returns `202 Accepted`; returns `404 Not Found` when no dead event with that ID exists.

## Health checks

- `GET /healthz`
- `GET /readyz`

Both execute a database query. They return HTTP `200` with body `ok` when PostgreSQL is reachable, otherwise HTTP `503`.

## Common status codes

| Status | Meaning |
| --- | --- |
| `200` | Request completed; used by health checks and channel listing. |
| `202` | Asynchronous operation accepted. |
| `204` | Successful empty response, including unknown channel deletion. |
| `400` | Invalid request or malformed channel input. |
| `401` | Missing or invalid management bearer token, or invalid WebSub signature. |
| `403` | WebSub topic is unknown/inactive or its verification token is incorrect. |
| `404` | Requested dead event does not exist. |
| `503` | Database unavailable or WebSub notification could not be persisted. |

Responses for some internal failures may be intentionally generic. Check application logs for diagnostic details.
