# VoxyTube

VoxyTube is an independent Rust service that receives YouTube upload notifications through WebSub and is designed to forward normalized events to Voxy. It does not connect to Discord itself.

## Deployment: one Pelican/Pterodactyl container

The official container bundles everything required to run the service:

- VoxyTube, compiled in Rust.
- PostgreSQL, running locally inside the same container.
- An entrypoint that initializes PostgreSQL on first boot, starts both processes, and shuts them down cleanly.
- Persistent database storage under `/home/container/data`.

**No external PostgreSQL server, Redis instance, or separate database container is required.** PostgreSQL listens only on the container loopback interface and is not exposed as a game-server allocation. Only the VoxyTube HTTP port needs to be allocated and exposed through HTTPS for YouTube WebSub callbacks.

The PostgreSQL password is generated automatically and persisted in the data volume when `POSTGRES_PASSWORD` is left empty. Set that variable only if you want to provide your own password.

### Install with Pelican / Pterodactyl

1. Import [`deploy/pelican/egg-voxytube.json`](deploy/pelican/egg-voxytube.json) into the panel.
2. Create a server using the VoxyTube Egg.
3. Allocate the HTTP port (8080 by default).
4. Start the server and keep its data volume persistent.
5. Configure a public HTTPS callback URL pointing to the allocated HTTP port through your reverse proxy.

The image is published to `ghcr.io/kuafeur/voxytube:latest` by the GitHub Actions container workflow on pushes to `main`. Version tags are also published.

### Build the container yourself

```bash
docker build -t voxytube:local .
docker run -d \
  --name voxytube \
  -p 8080:8080 \
  -v voxytube-data:/home/container/data \
  voxytube:local
```

Do not publish port 5432. It is intentionally bound to localhost inside the container.

## Local development

For development outside the bundled container, install Rust stable (edition 2024) and PostgreSQL 14 or newer.

1. Create a PostgreSQL database named `voxytube`.
2. Copy `.env.example` to `.env` and set `DATABASE_URL`.
3. Run:

```bash
cargo run
```

The HTTP server listens on `0.0.0.0:8080` by default.

## HTTP endpoints currently implemented

- `GET /healthz` — checks database connectivity.
- `GET /websub` — responds to WebSub subscription verification challenges.
- `POST /websub` — parses and persists YouTube Atom notifications.

The WebSub callback must be reachable over public HTTPS. Put the service behind a reverse proxy such as Nginx or Cloudflare.

## Project status

VoxyTube is still under active development. The current repository contains the initial WebSub receiver and PostgreSQL persistence layer, plus the all-in-one container packaging. RSS reconciliation, subscription lifecycle management, authenticated channel management, durable event delivery to Voxy, and full security/compatibility tests are not yet complete. The container packaging must also pass its first CI build before it should be considered ready for deployment.

No YouTube page scraping or YouTube Data API integration is planned.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
