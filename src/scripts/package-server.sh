#!/bin/sh
# Package a prebuilt match server without overwriting previous releases.
# Publishes nothing: the archive stays under out/releases/.
set -eu
cd "$(dirname "$0")/.."
platform=${1:?macos-arm64, macos-x64, windows-x64 or linux-x64}
binary=${2:?absolute path to compiled peakrunner-server}
version=$(sed -n 's/^pub const GAME_VERSION: &str = "\([^"]*\)";.*/\1/p' crates/protocol/src/lib.rs)
revision=${3:-}
case "$revision" in *[!a-zA-Z0-9-]*) echo "Invalid package revision" >&2; exit 2 ;; esac
package_version="$version${revision:+-$revision}"
case "$platform" in macos-arm64|macos-x64|windows-x64|linux-x64) ;; *) exit 2 ;; esac
test -f "$binary"
release_dir="out/releases/$package_version/server-$platform"
test ! -e "$release_dir" || { echo "Release already exists: $release_dir" >&2; exit 1; }
mkdir -p "$release_dir"
cp assets/package-docs/server-readme.md "$release_dir/README.md"
cp ../LICENSE "$release_dir/LICENSE.txt"
cp ../COPYRIGHT.md "$release_dir/COPYRIGHT.txt"
cp ../LICENSES/CC-BY-SA-4.0.txt "$release_dir/CC-BY-SA-4.0.txt"
cp assets/package-docs/THIRD-PARTY-NOTICES.txt "$release_dir/THIRD-PARTY-NOTICES.txt"
case "$platform" in
  windows-x64) cp "$binary" "$release_dir/peakrunner-server.exe" ;;
  *) cp "$binary" "$release_dir/peakrunner-server"; chmod +x "$release_dir/peakrunner-server" ;;
esac
# The default maps, beside the executable (docs/modding.md).
sh scripts/stage-packs.sh "$release_dir/packs"
# Example mods to copy into content/mods/ (docs/making-content.md).
mkdir -p "$release_dir/examples"
cp -R examples/mods "$release_dir/examples/mods"
# Apple Silicon runs only signed code; an ad-hoc signature is not Developer ID
# (Intel Macs get the same, so both open the same way).
case "$platform" in macos-*) codesign --force --sign - "$release_dir/peakrunner-server" ;; esac
archive="PeakRunner-Server-$package_version-$platform"
case "$platform" in
  linux-x64) COPYFILE_DISABLE=1 tar --no-xattrs -czf "$release_dir/../$archive.tar.gz" -C "$release_dir" . ;;
  *) (cd "$release_dir" && zip -qr "../$archive.zip" .) ;;
esac
echo "Packaged $release_dir"
