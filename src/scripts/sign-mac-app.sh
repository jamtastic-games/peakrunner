#!/bin/sh
# Seal the finished bundle, not just its executable. No Developer ID is implied.
set -eu
app=${1:?path to completed .app bundle}
test -f "$app/Contents/Info.plist"
test -d "$app/Contents/MacOS"
# Other executables in the bundle (the server beside the game) first: Intel
# binaries leave the linker unsigned, and an unsigned one fails the seal.
main=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")
for f in "$app"/Contents/MacOS/*; do
  test "$(basename "$f")" = "$main" || codesign --force --sign - --timestamp=none "$f"
done
codesign --force --sign - --timestamp=none "$app"
codesign --verify --deep --strict --verbose=2 "$app"
