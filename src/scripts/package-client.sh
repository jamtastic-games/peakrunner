#!/bin/sh
# Package a prebuilt client without overwriting previous releases. With a
# server binary (4th argument), the package carries it for Host game. The
# default maps go in packs/ (Contents/Resources/packs in the Mac app).
set -eu
cd "$(dirname "$0")/.."
platform=${1:?macos-arm64, macos-x64, windows-x64 or linux-x64}
binary=${2:?absolute path to compiled client}
version=$(sed -n 's/^pub const GAME_VERSION: &str = "\([^"]*\)";.*/\1/p' crates/protocol/src/lib.rs)
revision=${3:-}
server=${4:-}
case "$revision" in *[!a-zA-Z0-9-]*) echo "Invalid package revision" >&2; exit 2 ;; esac
package_version="$version${revision:+-$revision}"
case "$platform" in macos-arm64|macos-x64|windows-x64|linux-x64) ;; *) exit 2 ;; esac
test -f "$binary"
test -z "$server" || test -f "$server"
release_dir="out/releases/$package_version/$platform"
test ! -e "$release_dir" || { echo "Release already exists: $release_dir" >&2; exit 1; }
mkdir -p "$release_dir"
cp assets/package-docs/client-release-notes.md "$release_dir/README.md"
cp ../LICENSE "$release_dir/LICENSE.txt"
cp ../COPYRIGHT.md "$release_dir/COPYRIGHT.txt"
cp ../LICENSES/CC-BY-SA-4.0.txt "$release_dir/CC-BY-SA-4.0.txt"
cp assets/package-docs/THIRD-PARTY-NOTICES.txt "$release_dir/THIRD-PARTY-NOTICES.txt"
case "$platform" in
  macos-arm64|macos-x64)
    app="$release_dir/PeakRunner.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$binary" "$app/Contents/MacOS/peakrunner"
    if test -n "$server"; then cp "$server" "$app/Contents/MacOS/peakrunner-server"; chmod +x "$app/Contents/MacOS/peakrunner-server"; fi
    cp assets/Info.plist "$app/Contents/Info.plist"
    cp assets/icons/peakrunner.icns "$app/Contents/Resources/AppIcon.icns"
    chmod +x "$app/Contents/MacOS/peakrunner"
    sh scripts/stage-packs.sh "$app/Contents/Resources/packs"
    sh scripts/sign-mac-app.sh "$app"
    ;;
  windows-x64)
    cp "$binary" "$release_dir/PeakRunner.exe"
    if test -n "$server"; then cp "$server" "$release_dir/peakrunner-server.exe"; fi
    sh scripts/stage-packs.sh "$release_dir/packs"
    ;;
  linux-x64)
    cp "$binary" "$release_dir/peakrunner"; chmod +x "$release_dir/peakrunner"
    if test -n "$server"; then cp "$server" "$release_dir/peakrunner-server"; chmod +x "$release_dir/peakrunner-server"; fi
    sh scripts/stage-packs.sh "$release_dir/packs"
    ;;
esac
archive="PeakRunner-$package_version-$platform"
case "$platform" in
  linux-x64) COPYFILE_DISABLE=1 tar --no-xattrs -czf "$release_dir/../$archive.tar.gz" -C "$release_dir" . ;;
  *) (cd "$release_dir" && zip -qr "../$archive.zip" .) ;;
esac
echo "Packaged $release_dir"
