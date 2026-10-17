#!/bin/sh
set -eu
cd "$(dirname "$0")"
if curl --silent --fail http://127.0.0.1:8080/ >/dev/null 2>&1; then exit 0; fi
mkdir -p out
cd site
nohup npm run dev >../out/site-preview.log 2>&1 &
