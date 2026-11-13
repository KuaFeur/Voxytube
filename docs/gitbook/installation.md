# Installation with Pelican

This guide describes the single-container Pelican deployment. The repository also contains the Egg at `deploy/pelican/egg-voxytube.json`.

## Requirements

- A Pelican server capable of running the published VoxyTube container image.
- An allocated TCP port for the HTTP API.
- A public HTTPS hostname that can reach that allocated port.
- Persistent storage for `/home/container/data`.

## 1. Import the Egg

Import `deploy/pelican/egg-voxytube.json` into Pelican, create a server from the VoxyTube Egg, and allocate its HTTP port.

The image is:

```text
ghcr.io/kuafeur/voxytube:latest
```

The application listens on `PORT` (default `8080`) inside the container. In Pelican, use the actual allocated port and configure your reverse proxy to forward HTTP traffic to it.

## 2. Configure the public callback

Set `PUBLIC_CALLBACK_URL` to the exact public HTTPS callback, including the path:

```text
https://youtube.example.com/websub
```

The path must be exactly `/websub`. The application rejects non-HTTPS URLs, query strings, fragments, embedded credentials, and any other callback path.

The reverse proxy must preserve the `/websub` path and forward requests to VoxyTube. The callback must be reachable from the public internet for YouTube's verification challenge and notifications.

## 3. Set credentials

Generate a strong management token:

```bash
openssl rand -hex 32
```

Set the output as `API_TOKEN`. It must contain at least 32 characters. Keep it private; it authorizes channel and event management.

If Voxy exposes a protected webhook receiver, set `VOXY_WEBHOOK_TOKEN` to the receiver's bearer token. Configure the same expected token on the receiving side.

## 4. Configure webhook delivery

Set `VOXY_WEBHOOK_URL` to the public or internal HTTP(S) endpoint that will receive event JSON.

You may leave it empty during initial setup. In that case, delivery is disabled and events remain in the persistent outbox until a destination is configured.

## 5. Start and verify

Start the Pelican server. The entrypoint initializes PostgreSQL and applies migrations on startup.

Check:

```bash
curl -i https://youtube.example.com/healthz
```

A healthy response is HTTP `200` with body `ok`. Then register a test channel using the [REST API](api.md).

## Persistent data

Keep `/home/container/data` on Pelican's persistent server volume. It contains the PostgreSQL cluster and generated database password. Removing this directory resets subscriptions, deduplication history and delivery history.

Never expose PostgreSQL port `5432` publicly.
