#!/bin/sh
# Rebuild game models from their sources (docs/model-pipeline.md):
#   sh scripts/models/build.sh SOURCES_DIR [KEY...]
# KEY is a folder under assets/models with a source.json (weapons/disc,
# armor/medium, structures/reefbreak-ship); with none, every one. Blender runs headless, two at a time;
# each model's log is in $TMPDIR/peakrunner-models/.
set -eu
cd "$(dirname "$0")/../.."
logs=${TMPDIR:-/tmp}/peakrunner-models
mkdir -p "$logs"
if [ "${1:-}" = "--one" ]; then
  src=$2 key=$3 log="$logs/$(echo "$3" | tr / -).log"
  spec="assets/models/$key/source.json"
  # A spec naming a "builder" is built by that script (a hard-surface model,
  # from no generated source); the rest by process.py from their sources.
  builder=$(sed -n 's/.*"builder": *"\([^"]*\)".*/\1/p' "$spec")
  if [ -n "$builder" ]; then
    set -- -b -P "$builder" -- "assets/models/$key/model.glb"
  else
    set -- -b -P scripts/models/process.py -- "$spec" "$src" "assets/models/$key/model.glb"
  fi
  if ${BLENDER:-blender} "$@" > "$log" 2>&1 && grep -q "^wrote " "$log"; then
    echo "built $key: $(grep -E '^[a-z_]+: [0-9]+ triangles|^wrote ' "$log" | sed 's/^wrote [^:]*: //; s/, bounds.*//' | tr '\n' ' ')"
  else
    echo "FAILED $key (see $log)"
  fi
  exit 0
fi
src=${1:?sources folder}; shift
if [ $# -eq 0 ]; then set -- $(cd assets/models && find . -name source.json | sed 's#/source.json##; s#^\./##' | sort); fi
for key in "$@"; do echo "$key"; done | xargs -P 2 -n 1 sh scripts/models/build.sh --one "$src"
