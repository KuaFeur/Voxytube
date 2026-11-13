# Monitoring, troubleshooting and backups

## Health checks

Use:

```bash
curl -i https://youtube.example.com/healthz
curl -i https://youtube.example.com/readyz
```

Both routes execute `SELECT 1` against PostgreSQL. HTTP `200` with body `ok` indicates database connectivity. HTTP `503` indicates the database check failed.

## Channel state

After adding a channel, inspect:

```bash
curl -s https://youtube.example.com/api/v1/channels \
  -H "Authorization: Bearer $API_TOKEN"
```

A `pending` state means WebSub verification has not completed. An `active` state means a valid subscribe challenge was processed. Inspect `last_error` and container logs if the channel remains pending.

## Event delivery

Inspect the outbox:

```bash
curl -s https://youtube.example.com/api/v1/events \
  -H "Authorization: Bearer $API_TOKEN"
```

Review counts and the latest 100 events. Check `status`, `attempts`, `next_attempt_at` and `last_error`.

- If events remain pending, verify that `VOXY_WEBHOOK_URL` is configured and reachable from the container.
- If the receiver returns non-2xx responses, check its route, authentication and request parsing.
- If events become dead, correct the underlying problem and requeue them using the retry endpoint.
- If the webhook is intentionally disabled, events remain stored and are not delivered.

## Common problems

### Health check returns 503

PostgreSQL is not reachable by the application. Inspect container startup logs and verify that the persistent data volume is writable and not full.

### Channel remains pending

Confirm that `PUBLIC_CALLBACK_URL` is publicly reachable over HTTPS, ends exactly in `/websub`, and that the reverse proxy forwards GET requests and query parameters intact. The hub must be able to call the challenge endpoint.

### No new video events arrive

Check that the channel is active, that POST requests reach `/websub`, and that the reverse proxy does not modify the request body or strip `X-Hub-Signature`. RSS reconciliation is best-effort and cannot guarantee access to older feed entries.

### Webhook events are repeatedly retried

Confirm that the receiver responds with a 2xx status, that its bearer token matches `VOXY_WEBHOOK_TOKEN`, and that it accepts JSON. Implement idempotency handling before retrying dead events.

## Backups

The PostgreSQL cluster is stored in `/home/container/data/postgres`. Keep the Pelican data directory persistent.

For a raw filesystem backup, stop VoxyTube first and copy the data directory. Do not copy a live PostgreSQL data directory as if it were a consistent raw backup.

For a logical backup while the service is running:

```bash
gosu postgres pg_dump -Fc -d voxytube -f /tmp/voxytube.dump
cp /tmp/voxytube.dump /home/container/data/voxytube.dump
```

Keep an additional copy outside the container. A persistent volume is not a backup by itself.

To restore a custom-format dump, place it in the persistent data directory, set `POSTGRES_RESTORE_FILE` to its path, and start the server. The entrypoint restores the dump before starting Rust and records a marker to prevent the same dump from being restored on every restart. See [the repository operations guide](../OPERATIONS.md) for the full procedure.

## Security checklist

- Use a unique, high-entropy `API_TOKEN`.
- Keep webhook tokens and database credentials out of source control and logs.
- Expose only the HTTP application port publicly.
- Never expose PostgreSQL port `5432`.
- Use HTTPS for the public WebSub callback and preferably for remote webhook delivery.
- Keep independent backups outside the Pelican volume.
