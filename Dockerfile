# syntax=docker/dockerfile:1
FROM --platform=$BUILDPLATFORM rust:1-bookworm AS builder
ARG TARGETARCH
WORKDIR /build

COPY Cargo.toml ./
COPY src ./src
COPY migrations ./migrations

# Compile Rust natively on the builder CPU. This avoids running rustc under QEMU
# when producing the arm64 image on an amd64 GitHub runner.
RUN if [ "$TARGETARCH" = "arm64" ]; then \
      apt-get update \
      && apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu \
      && rm -rf /var/lib/apt/lists/* \
      && rustup target add aarch64-unknown-linux-gnu \
      && CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
         AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar \
         CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
         cargo build --release --target aarch64-unknown-linux-gnu \
      && cp target/aarch64-unknown-linux-gnu/release/voxytube /build/voxytube; \
    else \
      cargo build --release \
      && cp target/release/voxytube /build/voxytube; \
    fi

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

COPY --from=builder /build/voxytube /usr/local/bin/voxytube
COPY docker/entrypoint.sh /usr/local/bin/voxytube-entrypoint
RUN chmod 0755 /usr/local/bin/voxytube /usr/local/bin/voxytube-entrypoint

WORKDIR /home/container
EXPOSE 8080
VOLUME ["/home/container/data"]
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/voxytube-entrypoint"]
