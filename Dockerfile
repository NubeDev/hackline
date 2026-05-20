# syntax=docker/dockerfile:1.7
#
# Multi-stage build for the hackline-gateway binary.
#
# The gateway is the only crate intended for cloud deployment. Agents
# and CLIs run on the user's own hardware. TLS is handled by Fly's
# edge proxy in this image — the embedded `tls` feature is off, so
# rustls/instant-acme aren't compiled in.

FROM rust:1.82-bookworm AS builder

WORKDIR /src

# Cache deps separately from sources. Copying the manifests first lets
# Docker reuse the dep-compile layer when only source files change.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY clients ./clients
COPY examples ./examples

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p hackline-gateway --bin serve \
 && cp target/release/serve /tmp/hackline-gateway-serve

FROM debian:bookworm-slim AS runtime

# `ca-certificates` for outbound HTTPS (e.g. ACME if enabled later via
# a separate image variant). `libssl3` covers reqwest's default
# native-tls backend that some workspace deps pull in transitively.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/*

# Persistent state lives under /data (mounted as a Fly volume).
RUN mkdir -p /data /etc/hackline
WORKDIR /etc/hackline

COPY --from=builder /tmp/hackline-gateway-serve /usr/local/bin/hackline-gateway
COPY deploy/fly/gateway.toml /etc/hackline/gateway.toml

# REST API. Tunnel TCP listeners are declared per-deployment in
# gateway.toml and must be added to fly.toml [[services]] to be
# reachable from the public internet.
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/hackline-gateway"]
CMD ["/etc/hackline/gateway.toml"]
