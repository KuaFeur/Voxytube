# VoxyTube operations

## First startup checklist

- Keep the Pelican server's `/home/container/data` directory persistent.
- Set `PUBLIC_CALLBACK_URL` to the public HTTPS callback ending in `/websub`.
- Set a unique `API_TOKEN` with at least 32 random characters.
- Set `VOXY_WEBHOOK_URL` to the Voxy receiver endpoint. Set `VOXY_WEBHOOK_TOKEN` if that endpoint requires bearer authentication.
- Route the public callback URL to the allocated VoxyTube HTTP port through HTTPS.
- Do not expose PostgreSQL port 5432.

The first start initializes PostgreSQL and applies SQL migrations automatically. If `POSTGRES_PASSWORD` is empty, a random password is stored in `/home/container/data/.postgres_password`.

## Add a YouTube channel

```bash
curl -X POST "https://your-host.example/api/v1/channels" \
  -H "Authorization: Bearer $API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"channel_id":"UCxxxxxxxxxxxxxxxxxxxxxx"}'
```

A `202 Accepted` means the subscription request was sent. It is not proof that YouTube has verified it. Poll `GET /api/v1/channels` until the channel state is `active`.

## Backup and restore

The entire `/home/container/data` directory contains the PostgreSQL cluster and generated password. Stop the Pelican server before making a raw directory-level backup.

For a logical backup while the server is running, execute inside the container:

```bash
gosu postgres pg_dump -Fc -d voxytube -f /tmp/voxytube.dump
cp /tmp/voxytube.dump /home/container/data/voxytube.dump
```

To restore, stop VoxyTube's Rust process, ensure PostgreSQL is running, then use:

```bash
gosu postgres pg_restore --clean --if-exists -d voxytube /home/container/data/voxytube.dump
```

Keep an additional copy outside the container. A persistent volume is not a backup by itself.

## Delivery semantics

VoxyTube persists a video and its outbox event in one PostgreSQL transaction. Delivery is at-least-once: a network timeout can occur after the receiver has accepted the request. The receiver should deduplicate using the `Idempotency-Key` header. Events that reach 20 failed attempts move to the `dead` state and require operator inspection.
