#!/bin/sh
# Package the small native launcher, independently of game payloads.
set -eu
cd "$(dirname "$0")/.."
platform=${1:?macos-arm64, macos-x64, windows-x64 or linux-x64}
binary=${2:?absolute path to compiled launcher}
revision=${3:?unique launcher release revision}
case "$platform" in macos-arm64|macos-x64|windows-x64|linux-x64) ;; *) exit 2 ;; esac
case "$revision" in ''|*[!a-zA-Z0-9.-]*) exit 2 ;; esac
out="out/launcher-packages/$revision/$platform"
test ! -e "$out" || { echo "Launcher package already exists" >&2; exit 1; }
mkdir -p "$out"
cp assets/package-docs/launcher-player-guide.md "$out/README.md"
cp ../LICENSE "$out/LICENSE.txt"
cp ../COPYRIGHT.md "$out/COPYRIGHT.txt"
cp ../LICENSES/CC-BY-SA-4.0.txt "$out/CC-BY-SA-4.0.txt"
cp assets/package-docs/THIRD-PARTY-NOTICES.txt "$out/THIRD-PARTY-NOTICES.txt"
case "$platform" in
 macos-arm64|macos-x64)
    app="$out/PeakRunner Launcher.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$binary" "$app/Contents/MacOS/peakrunner-launcher"
    cp assets/Info.plist "$app/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleExecutable peakrunner-launcher' "$app/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier dev.peakrunner.launcher' "$app/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleName PeakRunner Launcher' "$app/Contents/Info.plist"
    /usr/libexec/PlistBuddy -c 'Set :CFBundleDisplayName PeakRunner Launcher' "$app/Contents/Info.plist"
    cp assets/icons/launcher.icns "$app/Contents/Resources/AppIcon.icns"
    chmod +x "$app/Contents/MacOS/peakrunner-launcher"
    sh scripts/sign-mac-app.sh "$app"
    ;;
 windows-x64) cp "$binary" "$out/PeakRunnerLauncher.exe" ;;
 linux-x64) cp "$binary" "$out/peakrunner-launcher"; chmod +x "$out/peakrunner-launcher" ;;
esac
archive="PeakRunnerLauncher-$revision-$platform"
case "$platform" in
 linux-x64) COPYFILE_DISABLE=1 tar --no-xattrs -czf "$out/../$archive.tar.gz" -C "$out" . ;;
 *) (cd "$out" && zip -qr "../$archive.zip" .) ;;
esac
echo "Packaged $out"
