FROM rust:1.94.1-slim-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN cargo build --release --locked

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/orcastrate /usr/local/bin/orcastrate
COPY --chmod=755 action-entrypoint.sh /usr/local/bin/action-entrypoint.sh

ENTRYPOINT ["orcastrate"]
