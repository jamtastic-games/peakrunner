#!/bin/sh
# Build a double-clickable PeakRunner.app from the release binary.
set -eu
cd "$(dirname "$0")/.."

cargo build --release

APP="PeakRunner.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/peakrunner "$APP/Contents/MacOS/peakrunner"
cp assets/Info.plist "$APP/Contents/Info.plist"
chmod +x "$APP/Contents/MacOS/peakrunner"
# The default maps (docs/modding.md): the game reads them from Resources/packs.
sh scripts/stage-packs.sh "$APP/Contents/Resources/packs"
cp assets/icons/peakrunner.icns "$APP/Contents/Resources/AppIcon.icns"

sh scripts/sign-mac-app.sh "$APP"
echo "Built $(pwd)/$APP"
