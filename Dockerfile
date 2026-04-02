# syntax=docker/dockerfile:1.7

FROM --platform=$TARGETPLATFORM rust:1-slim-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --locked --release \
    && cp target/release/stunbeacon /tmp/stunbeacon

FROM --platform=$TARGETPLATFORM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /tmp/stunbeacon /usr/local/bin/stunbeacon

ENV LISTEN_ADDR=0.0.0.0:3000

EXPOSE 3000

ENTRYPOINT ["stunbeacon"]
