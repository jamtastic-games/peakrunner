#!/bin/sh
# Stage a signed updater feed from a prebuilt client, with the
# server binary beside it for Host game (6th argument). Does not deploy.
set -eu
cd "$(dirname "$0")/.."
platform=${1:?macos-arm64, macos-x64, windows-x64 or linux-x64}
binary=${2:?absolute path to the client binary}
key=${3:?absolute path to private Ed25519 PKCS8 signing key}
sequence=${4:?monotonically increasing release sequence}
notes=${5:?release notes file}
server=${6:-}
test -z "$server" || test -f "$server"
case "$platform" in macos-arm64|macos-x64|windows-x64|linux-x64) ;; *) exit 2 ;; esac
case "$sequence" in ''|*[!0-9]*) exit 2 ;; esac
version=$(sed -n 's/^pub const GAME_VERSION: &str = "\([^"]*\)";.*/\1/p' crates/protocol/src/lib.rs)
stage="out/launcher-releases/$version-$sequence/$platform"
test ! -e "$stage" || { echo "Release stage already exists" >&2; exit 1; }
mkdir -p "$stage/payload/game" "$stage/payload/map"
case "$platform" in
  windows-x64) cp "$binary" "$stage/payload/game/PeakRunner.exe" ;;
  *) cp "$binary" "$stage/payload/game/peakrunner" ;;
esac
# The launcher marks only the executable runnable; the game fixes the
# server's mode before starting it (src/host.rs).
if test -n "$server"; then
  case "$platform" in
    windows-x64) cp "$server" "$stage/payload/game/peakrunner-server.exe" ;;
    *) cp "$server" "$stage/payload/game/peakrunner-server" ;;
  esac
fi
# The default maps beside the game (game/packs), and Old Holler again in map/:
# launchers already installed require map/map.json and point the game at it.
# Identical files are one blob in the feed, so it costs no extra download.
sh scripts/stage-packs.sh "$stage/payload/game/packs"
cp "$stage"/payload/game/packs/maps/old-holler/* "$stage/payload/map/"
cp ../LICENSE "$stage/payload/game/LICENSE.txt"
cp ../COPYRIGHT.md "$stage/payload/game/COPYRIGHT.txt"
cp ../LICENSES/CC-BY-SA-4.0.txt "$stage/payload/game/CC-BY-SA-4.0.txt"
cp assets/package-docs/THIRD-PARTY-NOTICES.txt "$stage/payload/game/THIRD-PARTY-NOTICES.txt"
cargo run -p peakrunner-launcher --bin launcher-release -- publish "$key" "$platform" "$version" "$sequence" "$stage/payload" "$notes" "$stage/feed"
echo "Staged $stage/feed; upload blobs before the platform manifest. No deployment performed."
