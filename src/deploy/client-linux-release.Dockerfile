# syntax=docker/dockerfile:1
# Release recipe for Linux x86-64 clients: the standalone build and the
# launcher-managed (external-map) build, exported from an artifacts-only
# stage (`--target artifacts --output type=local,dest=...`). Same base and
# packages as client-linux.Dockerfile.
FROM rust:1-bookworm@sha256:ae1a730a949f727611a5c684e1e26e5a9bb9885b34f65a442744ca8a61c86ca5 AS base
RUN apt-get update && apt-get install -y --no-install-recommends libasound2-dev libudev-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-dev libxi-dev libxcursor-dev libxrandr-dev ca-certificates
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY examples ./examples
COPY crates ./crates
COPY assets/maps ./assets/maps
# The base game (modes and definitions) is compiled into the engine.
COPY assets/mods ./assets/mods
# The Library's pictures are compiled into the client (src/library.rs).
COPY assets/library ./assets/library
# The window icons are compiled into the client and launcher (assets/icons).
COPY assets/icons ./assets/icons
# The build host has 72 cores; the old cap of 4 jobs left most of them idle.
ARG JOBS=48
ENV CARGO_BUILD_JOBS=${JOBS}

FROM base AS build
# Registry and target caches persist between releases (see the server
# Dockerfile); the binaries are copied out of the cached target.
RUN --mount=type=cache,id=peakrunner-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=peakrunner-cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=peakrunner-client-target,target=/src/target \
    cargo build --locked --release -p peakrunner --bin peakrunner && cp target/release/peakrunner /peakrunner-standalone \
 && cargo build --locked --release -p peakrunner --bin peakrunner --features external-map && cp target/release/peakrunner /peakrunner-managed
FROM scratch AS artifacts
COPY --from=build /peakrunner-standalone /peakrunner-managed /

# The launcher alone (`--target launcher-artifacts`), for a launcher release
# (docs/launcher.md), from the same caches.
FROM base AS launcher
RUN --mount=type=cache,id=peakrunner-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=peakrunner-cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=peakrunner-client-target,target=/src/target \
    cargo build --locked --release -p peakrunner-launcher --bin peakrunner-launcher && cp target/release/peakrunner-launcher /peakrunner-launcher
FROM scratch AS launcher-artifacts
COPY --from=launcher /peakrunner-launcher /
