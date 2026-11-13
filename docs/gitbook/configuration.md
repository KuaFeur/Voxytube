# Configuration reference

Environment variables are read at process startup. Restart VoxyTube after changing them.

| Variable | Required | Default | Description |
| --- | --- | --- | --- |
| `PUBLIC_CALLBACK_URL` | Yes | — | Public HTTPS WebSub callback. Must end exactly in `/websub`. |
| `API_TOKEN` | Yes | — | Bearer token for management endpoints; minimum 32 characters. |
| `VOXY_WEBHOOK_URL` | No | Empty | HTTP(S) endpoint receiving normalized video events. Empty disables delivery. |
| `VOXY_WEBHOOK_TOKEN` | No | Empty | Optional bearer token sent to the webhook receiver. |
| `PORT` | No | `8080` | HTTP listen port, from 1 to 65535. |
| `DATABASE_URL` | Yes at runtime | Set by container entrypoint | PostgreSQL connection string. Local development must provide it. |
| `DATABASE_MAX_CONNECTIONS` | No | `10` | PostgreSQL pool size, from 1 to 100. |
| `RSS_INTERVAL_SECONDS` | No | `300` | RSS reconciliation interval, from 60 to 86400 seconds. |
| `WEBSUB_HUB_URL` | No | `https://pubsubhubbub.appspot.com/subscribe` | HTTPS WebSub hub endpoint. |
| `RUST_LOG` | No | `voxytube=info,tower_http=info` in the container | Rust tracing filter. |
| `POSTGRES_DB` | No | `voxytube` in the container | Embedded PostgreSQL database name. |
| `POSTGRES_USER` | No | `voxytube` in the container | Embedded PostgreSQL application user. |
| `POSTGRES_PASSWORD` | No | Generated and persisted if empty | Embedded PostgreSQL password. |
| `POSTGRES_RESTORE_FILE` | No | Empty | Optional custom-format `pg_dump` file restored once on startup. |

## Validation rules

- `PUBLIC_CALLBACK_URL` must be a valid HTTPS URL whose path is exactly `/websub`; query, fragment and user credentials are rejected.
- `API_TOKEN` must be at least 32 characters.
- `VOXY_WEBHOOK_URL`, when set, must be a valid HTTP or HTTPS URL without embedded credentials.
- `WEBSUB_HUB_URL` must be HTTPS and must not contain embedded credentials.
- `RSS_INTERVAL_SECONDS` must be between 60 and 86400.
- `DATABASE_MAX_CONNECTIONS` must be between 1 and 100.

## Example

```dotenv
PUBLIC_CALLBACK_URL=https://youtube.example.com/websub
API_TOKEN=replace-with-a-random-secret-of-at-least-32-characters
VOXY_WEBHOOK_URL=https://bot.example.com/internal/voxytube
VOXY_WEBHOOK_TOKEN=replace-with-the-receiver-token
PORT=8080
RSS_INTERVAL_SECONDS=300
WEBSUB_HUB_URL=https://pubsubhubbub.appspot.com/subscribe
RUST_LOG=voxytube=info,tower_http=info
```

Do not commit real tokens, database passwords, or production webhook credentials to source control.
