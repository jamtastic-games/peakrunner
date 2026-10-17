FROM rust:1-bookworm@sha256:ae1a730a949f727611a5c684e1e26e5a9bb9885b34f65a442744ca8a61c86ca5
RUN apt-get update && apt-get install -y --no-install-recommends libasound2-dev libudev-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-dev libxi-dev libxcursor-dev libxrandr-dev libvulkan1 mesa-vulkan-drivers xvfb xauth ca-certificates
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
RUN cargo build --locked --release -p peakrunner --bin peakrunner --example launch_smoke && cargo build --locked --release -p peakrunner-net --example public_smoke
