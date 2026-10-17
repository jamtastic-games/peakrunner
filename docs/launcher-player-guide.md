# PeakRunner Launcher

Extract the archive, then open PeakRunner Launcher.app (Mac),
PeakRunnerLauncher.exe (Windows), or peakrunner-launcher (Linux).

Choose Install game for your first download. Play starts PeakRunner; choose a
host from the game's multiplayer menu. Update game installs a new signed release.
Only changed files are downloaded, although a changed executable is downloaded
in full. Updates never replace an installation while its game is running.

Repair / verify restores damaged files from verified cache or the update service.
Rollback restores the preceding verified installation; an older game may not
match the live server. Internet is required for first installation and uncached
files. An already-installed game can still be started if the update service is
temporarily unavailable. Release notes and live hosts appear in the launcher.

macOS: Apple Silicon only. The app is ad-hoc signed but not Apple notarized.
You may need Privacy & Security > Open Anyway. Do not disable Gatekeeper globally.
Windows: x64, unsigned playtest. Tested compatibility is not a performance promise.
Linux: x64, glibc 2.36+, desktop OpenGL for the launcher and Vulkan for the game.
Runtime libraries include ALSA, udev, X11/Wayland and libxkbcommon-x11.

This version updates the game and maps, not the launcher itself. New launcher
versions are available at https://peakrunner.net/#downloads. Keep the standalone
game downloads as a recovery option. Install history and cache currently remain
on disk, so allow space for several installations. Logs are in game.log under:

- Mac: ~/Library/Application Support/PeakRunnerLauncher
- Windows: %LOCALAPPDATA%/PeakRunnerLauncher
- Linux: $XDG_DATA_HOME/PeakRunnerLauncher (default ~/.local/share/PeakRunnerLauncher)
