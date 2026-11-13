# syntax=docker/dockerfile:1
FROM rust:1-bookworm AS builder
WORKDIR /build

COPY Cargo.toml ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
ENV DEBIAN_FRONTEND=noninteractive \
    PORT=8080 \
    RUST_LOG=voxytube=info,tower_http=info \
    VOXYTUBE_DATA_DIR=/home/container/data

RUN apt-get update \
    && apt-get install -y --no-install-recommends postgresql postgresql-client gosu ca-certificates bash curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system voxytube \
    && useradd --system --gid voxytube --home-dir /home/container --no-create-home voxytube \
    && mkdir -p /home/container/data /opt/voxytube \
    && chown -R voxytube:voxytube /home/container

COPY --from=builder /build/target/release/voxytube /usr/local/bin/voxytube
COPY docker/entrypoint.sh /usr/local/bin/voxytube-entrypoint
RUN chmod 0755 /usr/local/bin/voxytube /usr/local/bin/voxytube-entrypoint

WORKDIR /home/container
EXPOSE 8080
VOLUME ["/home/container/data"]
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/voxytube-entrypoint"]
