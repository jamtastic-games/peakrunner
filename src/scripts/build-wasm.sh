#!/bin/sh
# Compile the WebAssembly target. Uses trunk when it is installed so the
# result is a page you can open; otherwise it only checks the crate builds.
set -eu
cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown

if command -v trunk >/dev/null 2>&1; then
  trunk build --release
  echo "Web build is in dist/"
else
  cargo build --release --target wasm32-unknown-unknown
  echo "wasm crate compiled. Install trunk (cargo install trunk) to bundle index.html."
fi
