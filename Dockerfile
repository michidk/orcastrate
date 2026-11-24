FROM rust:1.100.0-slim-bookworm@sha256:bb875b0f8ce726053e259034848c471840e96bfe591fd52556178ae9a05e6126 AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN cargo build --release --locked

FROM debian:bookworm-slim@sha256:e67eeead736ff7bbe258ef088bff683635c561b1fcfefad0806c1df4ab49ce0e

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/orcastrate /usr/local/bin/orcastrate
COPY --chmod=755 action-entrypoint.sh /usr/local/bin/action-entrypoint.sh

ENTRYPOINT ["orcastrate"]
