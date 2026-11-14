# VoxyTube operations

## First startup checklist

- Keep the Pelican server's `/home/container/data` directory persistent.
- Set `PUBLIC_CALLBACK_URL` to the public HTTPS callback ending in `/websub`.
- Set a unique `API_TOKEN` with at least 32 random characters.
- Set `WEBHOOK_URL` to the HTTP(S) endpoint that receives events. Set `WEBHOOK_TOKEN` if that endpoint requires Bearer authentication.
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

The PostgreSQL cluster is stored in `/home/container/data/postgres`. Stop the Pelican server before taking a raw directory-level backup. Restore a raw backup by replacing that directory while the server is stopped.

For a logical backup while the server is running, execute inside the container:

```bash
gosu postgres pg_dump -Fc -d voxytube -f /tmp/voxytube.dump
cp /tmp/voxytube.dump /home/container/data/voxytube.dump
```

To restore a custom-format dump, place it in the persistent data directory, set `POSTGRES_RESTORE_FILE=/home/container/data/voxytube.dump` in the Egg, and start the server. The entrypoint restores it before starting Rust, assigns restored objects to the application database user, and creates a marker so the same dump is not applied on every restart. To deliberately apply that same dump again, stop the server and remove the matching `.restore_done.*` marker first. Keep an additional copy outside the container; a persistent volume is not a backup by itself.

## Delivery semantics

VoxyTube persists a video and its outbox event in one PostgreSQL transaction. Delivery is at-least-once: a network timeout can occur after the receiver has accepted the request. The receiver should deduplicate using the `Idempotency-Key` header. Events that reach 20 failed attempts move to the `dead` state and require operator inspection.
