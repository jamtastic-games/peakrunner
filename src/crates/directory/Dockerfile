FROM rust:1-bookworm@sha256:ae1a730a949f727611a5c684e1e26e5a9bb9885b34f65a442744ca8a61c86ca5 AS build
WORKDIR /src
# Cargo reads the workspace manifests, but only builds the directory dependency graph.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY crates ./crates
RUN cargo build --locked --release -p peakrunner-directory --bin peakrunner-directory

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171
COPY --from=build /src/target/release/peakrunner-directory /usr/local/bin/peakrunner-directory
EXPOSE 8080
USER nobody
CMD ["peakrunner-directory"]
