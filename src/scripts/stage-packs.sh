#!/bin/sh
# Stage the default maps into a package's packs/ folder (docs/modding.md):
# packs/maps/<key>/ with map.json and info.json as they are and the payloads
# zlib-compressed; every staged map is loaded again to prove it validates.
# The authored models go in packs/models/<key>/model.glb
# (docs/model-pipeline.md); their sources stay behind, and the structures'
# models ride in their maps' models.glb instead.
# Usage: sh scripts/stage-packs.sh <destination packs folder> (must not exist).
set -eu
cd "$(dirname "$0")/.."
dest=${1:?destination packs folder}
cargo run --locked --release -q -p peakrunner-core --example stage_packs -- assets/maps "$dest"
(cd assets/models && find . -name model.glb -not -path './structures/*' | sed 's#^\./##') | while read -r f; do
  mkdir -p "$dest/models/$(dirname "$f")"
  cp "assets/models/$f" "$dest/models/$f"
done
