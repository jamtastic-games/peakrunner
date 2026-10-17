# syntax=docker/dockerfile:1
FROM rust:1-bookworm@sha256:ae1a730a949f727611a5c684e1e26e5a9bb9885b34f65a442744ca8a61c86ca5 AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets/maps ./assets/maps
# The base game (modes and definitions) is compiled into the engine.
COPY assets/mods ./assets/mods
COPY crates ./crates
# BuildKit caches (Docker 23+ builds with BuildKit by default): the crate
# registry and the target directory persist between releases, so only
# changed crates recompile. The binary is copied out of the cached target.
RUN --mount=type=cache,id=peakrunner-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=peakrunner-cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=peakrunner-server-target,target=/src/target \
    cargo build --locked --release -p peakrunner-server --bin peakrunner-server \
 && cp target/release/peakrunner-server /peakrunner-server \
 && cargo run --locked --release -q -p peakrunner-core --example stage_packs -- assets/maps /packs

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171
COPY --from=build /peakrunner-server /usr/local/bin/peakrunner-server
# The default maps (docs/modding.md), staged compressed by the build.
COPY --from=build /packs /opt/peakrunner/packs
ENV PEAKRUNNER_PACK_DIR=/opt/peakrunner/packs
EXPOSE 7777/udp
USER nobody
CMD ["peakrunner-server"]
