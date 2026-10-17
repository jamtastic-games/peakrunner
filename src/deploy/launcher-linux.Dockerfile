# BUILD_BASE may point at a recorded client-build image to reuse Rust artifacts.
# Default remains a reproducible clean Debian 12 toolchain build.
ARG BUILD_BASE=rust:1-bookworm@sha256:ae1a730a949f727611a5c684e1e26e5a9bb9885b34f65a442744ca8a61c86ca5
FROM ${BUILD_BASE} AS build
RUN apt-get update && apt-get install -y --no-install-recommends libasound2-dev libudev-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-dev libxi-dev libxcursor-dev libxrandr-dev libvulkan1 mesa-vulkan-drivers libegl1 libgl1 libgl1-mesa-dri xvfb xauth ca-certificates
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY examples ./examples
COPY crates ./crates
COPY assets/maps ./assets/maps
COPY assets/mods ./assets/mods
# The Library's pictures are compiled into the client (src/library.rs).
COPY assets/library ./assets/library
# The window icons are compiled into the client and launcher (assets/icons).
COPY assets/icons ./assets/icons
ENV CARGO_BUILD_JOBS=4
RUN cargo build --locked --release -p peakrunner-launcher --bins --examples && cargo test --locked -p peakrunner-launcher --lib
RUN cargo build --locked --release -p peakrunner --bin peakrunner --example launch_smoke --features external-map && mkdir -p /artifacts/managed && cp target/release/peakrunner /artifacts/managed/peakrunner && cp target/release/examples/launch_smoke /artifacts/managed/launch_smoke
RUN cargo build --locked --release -p peakrunner --bin peakrunner && mkdir -p /artifacts/standalone && cp target/release/peakrunner /artifacts/standalone/peakrunner

# Export only deliverables; avoid exporting the large compiler/debug cache.
FROM scratch AS artifacts
COPY --from=build /artifacts/ /
COPY --from=build /src/target/release/peakrunner-launcher /launcher/peakrunner-launcher
COPY --from=build /src/target/release/launcher-release /launcher/launcher-release
COPY --from=build /src/target/release/examples/smoke /qa/launcher-smoke
COPY --from=build /src/target/release/examples/play_smoke /qa/play-smoke
