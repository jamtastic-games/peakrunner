#!/bin/sh
# Build the browser version into OUTPUT (refuses an existing directory):
# peakrunner.js + peakrunner_bg.wasm, served from /downloads/web/<version>/.
# Needs the wasm32-unknown-unknown target and wasm-bindgen-cli matching the
# lockfile's wasm-bindgen (cargo install --locked wasm-bindgen-cli --version V).
set -eu
out=${1:?usage: build-web.sh OUTPUT_DIR}
test ! -e "$out" || { echo "refusing to overwrite $out" >&2; exit 1; }
want=$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/"/,"",$3); print $3}' Cargo.lock)
have=$(wasm-bindgen --version | awk '{print $2}')
test "$want" = "$have" || { echo "wasm-bindgen CLI $have does not match lockfile $want" >&2; exit 1; }
cargo build --locked --release --target wasm32-unknown-unknown -p peakrunner --bin peakrunner
wasm-bindgen --target web --no-typescript --out-dir "$out" target/wasm32-unknown-unknown/release/peakrunner.wasm
(cd "$out" && shasum -a 256 peakrunner.js peakrunner_bg.wasm)
