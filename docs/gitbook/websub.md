# YouTube WebSub

WebSub is VoxyTube's primary notification path. YouTube sends subscription verification challenges and Atom feed notifications to the public callback configured in `PUBLIC_CALLBACK_URL`.

## Callback URL

The callback must be a public HTTPS URL ending exactly in `/websub`, for example:

```text
https://youtube.example.com/websub
```

Your reverse proxy must forward both GET and POST requests to VoxyTube without changing the path or request body.

## Subscription lifecycle

When a channel is added, VoxyTube creates a pending subscription and sends a subscribe request to the configured hub. The request includes the Atom topic URL, callback URL, asynchronous verification mode, a verification token, a lease request and a per-subscription secret.

The hub later calls `GET /websub` with WebSub challenge parameters. VoxyTube checks the topic, mode, verification token and current subscription state. A valid subscribe challenge changes the channel state to `active` and records the lease expiry. A valid unsubscribe challenge deletes the subscription row.

The subscription worker checks periodically for pending subscriptions, unsubscribe requests and leases nearing expiry, then requests the appropriate operation again when necessary.

## Incoming notifications

YouTube sends Atom XML to `POST /websub`. VoxyTube:

1. Rejects an empty or invalid UTF-8 body.
2. Parses Atom entries and extracts YouTube video and channel IDs.
3. Requires all entries in a notification batch to belong to the same channel.
4. Requires that channel to have an active subscription.
5. Validates the `X-Hub-Signature` header using HMAC-SHA1 and the subscription secret.
6. Persists eligible videos and their outbox events.

The signature header must use the `sha1=<hex-digest>` format. Do not attempt to call the callback manually with unsigned XML as a substitute for a real hub notification.

## Callback responses

| Condition | Response |
| --- | --- |
| Valid subscribe/unsubscribe challenge | `200 OK`, response body is the challenge string |
| Missing or invalid challenge parameters | `400 Bad Request` |
| Unknown topic or wrong verification token | `403 Forbidden` |
| Subscription state no longer matches challenge | `410 Gone` |
| Database error while processing challenge | `503 Service Unavailable` |
| Valid notification persisted | `204 No Content` |
| Empty or invalid XML notification | `400 Bad Request` |
| Notification for a channel without an active subscription | `403 Forbidden` |
| Missing or invalid HMAC signature | `401 Unauthorized` |
| Database persistence failure | `503 Service Unavailable` |

## RSS reconciliation

A background worker polls active and pending channel feeds at the configured interval. It uses ETag and Last-Modified conditional requests when available and backs off after errors.

RSS is a best-effort fallback, not a guaranteed historical archive. It can only see entries YouTube still exposes in the feed. WebSub remains the primary near-real-time mechanism.
