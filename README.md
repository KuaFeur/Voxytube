<!--
  VoxyTube — YouTube event gateway
  Documentation: https://voxytube.gitbook.io
-->

<div align="center">

# VoxyTube

### A reliable, self-hosted YouTube event gateway.

Receive YouTube upload notifications and forward clean, normalized events to any HTTP webhook — without tying your workflow to a specific platform.

[**Documentation**](https://voxytube.gitbook.io) · [GitHub](https://github.com/KuaFeur/Voxytube) · [Report an issue](https://github.com/KuaFeur/Voxytube/issues)

<br />

[![CI](https://github.com/KuaFeur/Voxytube/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/KuaFeur/Voxytube/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/KuaFeur/Voxytube)](LICENSE)
[![Rust](https://img.shields.io/badge/built%20with-Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Docker](https://img.shields.io/badge/container-GHCR-2496ED?logo=docker&logoColor=white)](https://github.com/KuaFeur/Voxytube/pkgs/container/voxytube)

</div>

---

## What is VoxyTube?

VoxyTube is a lightweight service that turns YouTube channel uploads into reliable HTTP events.

It uses **YouTube WebSub** for near-real-time notifications, with RSS reconciliation as a fallback. Events are stored in PostgreSQL before delivery, so temporary destination outages do not immediately lose notifications.

VoxyTube is destination-agnostic: send events to your own application, automation platform, community integration, or any service that accepts HTTP requests.

> **Start here:** the full installation, configuration, API and operations guides are available in the **[VoxyTube documentation](https://voxytube.gitbook.io)**.

## Highlights

| | Capability | Details |
|:--:|---|---|
| ⚡ | **Near-real-time notifications** | WebSub subscriptions receive upload notifications from YouTube. |
| 🔁 | **RSS reconciliation** | Periodic feed checks help recover entries missed by push delivery. |
| 💾 | **Durable event storage** | PostgreSQL-backed deduplication and transactional outbox. |
| 🪝 | **Any HTTP destination** | Deliver normalized JSON to any compatible webhook endpoint. |
| 🔐 | **Protected management API** | Bearer-token authentication for channel and event management. |
| ♻️ | **Retry handling** | Failed deliveries are retried; exhausted events can be inspected and requeued. |
| 📦 | **Simple deployment** | A single container image includes the service and PostgreSQL. |

### How it works

\`\`\`mermaid
flowchart LR
    YT[YouTube] -->|WebSub notifications| WS[VoxyTube]
    YT -.->|RSS reconciliation| WS
    WS --> DB[(PostgreSQL)]
    DB --> OUT[Durable event outbox]
    OUT -->|HTTP POST + Idempotency-Key| DEST[Any HTTP endpoint]
\`\`\`

1. Add a YouTube channel through the management API.
2. VoxyTube subscribes to the channel's WebSub feed.
3. Incoming notifications are validated, deduplicated and persisted.
4. The delivery worker sends a normalized JSON event to your configured endpoint.
5. Failed deliveries are retried automatically.

## Get started

### Deployment with Pelican / Pterodactyl

The recommended deployment uses the published container image and the included Egg.

1. Download and import [\`egg-voxytube.json\`](deploy/pelican/egg-voxytube.json) into your panel.
2. Create a server from the imported Egg and allocate its application port (default: \`8080\`).
3. Configure the environment variables below.
4. Start the server and keep \`/home/container/data\` on persistent storage.
5. Expose the callback URL over HTTPS and route it to the allocated container port.

**Container image**

\`\`\`text
ghcr.io/kuafeur/voxytube:latest
\`\`\`

**Required configuration**

| Variable | Purpose |
|---|---|
| \`PUBLIC_CALLBACK_URL\` | Public HTTPS callback URL, ending in \`/websub\`. |
| \`API_TOKEN\` | Management API Bearer token; use at least 32 random characters. |
| \`WEBHOOK_URL\` | HTTP(S) destination that receives normalized event JSON. |
| \`WEBHOOK_TOKEN\` | Optional Bearer token sent to the destination. |

**Optional configuration**

| Variable | Default / purpose |
|---|---|
| \`PORT\` | HTTP listen port; defaults to \`8080\`. |
| \`POSTGRES_DB\` | Embedded PostgreSQL database name; defaults to \`voxytube\`. |
| \`POSTGRES_USER\` | Embedded PostgreSQL user; defaults to \`voxytube\`. |
| \`POSTGRES_PASSWORD\` | If omitted, a random password is generated and persisted. |
| \`POSTGRES_RESTORE_FILE\` | Optional custom-format PostgreSQL dump to restore on startup. |
| \`DATABASE_MAX_CONNECTIONS\` | Maximum database connections; defaults to \`10\`. |
| \`RSS_INTERVAL_SECONDS\` | RSS reconciliation interval; defaults to \`300\`. |
| \`WEBSUB_HUB_URL\` | WebSub hub URL. |
| \`RUST_LOG\` | Rust tracing filter. |

Generate a management token with:

\`\`\`bash
openssl rand -hex 32
\`\`\`

The container's PostgreSQL instance is internal. **Do not expose port \`5432\` publicly.** Keep \`/home/container/data\` persistent: it contains the database and delivery history.

For other deployment methods, reverse-proxy configuration, backups and restore procedures, see the **[documentation](https://voxytube.gitbook.io)**.

## Management API

All management routes require:

\`\`\`http
Authorization: Bearer <API_TOKEN>
\`\`\`

| Method | Endpoint | Description |
|---|---|---|
| \`POST\` | \`/api/v1/channels\` | Add a channel and request a WebSub subscription. |
| \`GET\` | \`/api/v1/channels\` | List configured channels and subscription states. |
| \`DELETE\` | \`/api/v1/channels/{channel_id}\` | Request an asynchronous unsubscribe. |
| \`GET\` | \`/api/v1/events\` | View event counts and the latest deliveries. |
| \`POST\` | \`/api/v1/events/{id}/retry\` | Requeue a dead event. |
| \`GET\` | \`/healthz\` | Check database connectivity. |
| \`GET\` | \`/readyz\` | Check service readiness. |

The WebSub callback is public and is available at \`GET /websub\` and \`POST /websub\`.

### Add a channel

\`\`\`bash
curl -X POST "https://your-host.example/api/v1/channels" \\
  -H "Authorization: Bearer $API_TOKEN" \\
  -H "Content-Type: application/json" \\
  -d '{"channel_id":"UCxxxxxxxxxxxxxxxxxxxxxx"}'
\`\`\`

A \`202 Accepted\` response means the subscription request was submitted. It does not mean YouTube has verified it yet. Check \`GET /api/v1/channels\` until the channel state becomes \`active\`.

## Event format

VoxyTube sends a normalized JSON payload to the configured destination:

\`\`\`json
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
\`\`\`

Each delivery includes an \`Idempotency-Key\` header:

\`\`\`http
Idempotency-Key: voxytube-<outbox-id>
\`\`\`

### Delivery guarantees

Delivery is **at-least-once**, not exactly-once. A network timeout can happen after the destination has already processed a request. Your receiving endpoint should therefore store and honor the \`Idempotency-Key\` to prevent duplicate side effects.

Events are persisted before delivery. If \`WEBHOOK_URL\` is empty, delivery is disabled and events remain in the outbox until a destination is configured.

## Technical details

- **Language:** Rust (edition 2024)
- **HTTP:** Axum
- **Database:** PostgreSQL
- **Notifications:** YouTube WebSub with RSS reconciliation
- **Deployment:** Docker / Pelican / Pterodactyl

VoxyTube does not use YouTube page scraping, \`yt-dlp\`, or a YouTube Data API key.

## Development

Requirements: Rust stable and PostgreSQL 14 or newer.

\`\`\`bash
git clone https://github.com/KuaFeur/Voxytube.git
cd Voxytube
cp .env.example .env
\`\`\`

Set \`DATABASE_URL\`, \`PUBLIC_CALLBACK_URL\` and \`API_TOKEN\` in \`.env\`, then run:

\`\`\`bash
cargo run
\`\`\`

Run the test suite and static checks:

\`\`\`bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -A warnings
\`\`\`

## Documentation

The GitBook site is the main reference for setup and operations:

<div align="center">

### [Read the VoxyTube documentation →](https://voxytube.gitbook.io)

Installation · Configuration · API · Troubleshooting · Backups

</div>

Additional repository resources:

- [Operations guide](docs/OPERATIONS.md)
- [Pelican Egg](deploy/pelican/egg-voxytube.json)
- [Releases](https://github.com/KuaFeur/Voxytube/releases)
- [Issues and bug reports](https://github.com/KuaFeur/Voxytube/issues)

## Project status

VoxyTube is under active development and validation. Do not treat the current V1 implementation as production-stable until CI, supported container architectures and end-to-end WebSub/RSS/outbox delivery have all been validated. YouTube and WebSub behavior can change independently of this project.

## License

VoxyTube is licensed under the [Apache License 2.0](LICENSE).
