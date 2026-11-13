# VoxyTube

VoxyTube is a standalone Rust service that receives YouTube upload notifications through WebSub and forwards normalized events to Voxy.

## Current status

Early development. The initial service provides:

- Axum HTTP server with `/healthz` and `/websub` endpoints.
- WebSub GET challenge handling.
- YouTube Atom notification parsing.
- PostgreSQL persistence with idempotency by YouTube video ID.
- Database migrations and graceful shutdown.

RSS reconciliation and delivery to Voxy are planned next. No YouTube scraping is planned.

## Requirements

- Rust stable (edition 2024)
- PostgreSQL 14+

## Run locally

1. Create a PostgreSQL database named `voxytube`.
2. Copy `.env.example` to `.env` and update `DATABASE_URL`.
3. Start the service:

```bash
cargo run
```

The service listens on `0.0.0.0:8080` by default.

## Endpoints

- `GET /healthz` — checks database connectivity.
- `GET /websub` — answers WebSub subscription verification challenges.
- `POST /websub` — receives and persists YouTube Atom notifications.

The WebSub callback must be reachable over public HTTPS. Put the service behind a reverse proxy such as Nginx or Cloudflare Tunnel.

## Configuration

See `.env.example`.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
