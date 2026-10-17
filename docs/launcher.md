# PeakRunner Launcher

The separate native launcher installs and starts the game, checks for updates,
displays signed release notes and live directory hosts, repairs files, and can
restore the preceding installation. No administrator privileges are required.
Updates are explicit, not automatic during a match. The running game holds a lock
even if its launcher closes. Standalone clients remain supported.

## Release status

The current release and launcher are listed at peakrunner.net.
Launcher `0.1.0-r3` (2026-10-10) is the first
with Intel Macs, the cleaner window and the banner; `0.1.0-r2` was uploaded
but never advertised (the banner was asked for first). Launcher `0.1.0-r4`
(2026-10-15) is r3 with its own icon
(`assets/icons/launcher.*`, `docs/client-builds.md` "Icons"); a launcher
doesn't update itself, so installed ones keep theirs until replaced.
Launcher r1 rejects
zero-byte files, so managed packages omit only empty `ambient.f32`; the game
accepts absence only when the signed map manifest records the empty SHA-256.
Standalone/server packs retain those files. No key rotation or launcher schema
change is required. Publication signatures/hashes and fresh Mac installation
passed. The older status and verification below document the previous releases.

Launcher `0.1.0-r1`, game `.20260919.4` and signed feed sequence `2026091907`
are published on peakrunner.net. This client-only visual release remains
compatible with `.20260919.3`; Springdale Central was subsequently updated to
`.20260919.4`. Gameplay compatibility is unchanged. Older sections below
document the initial launcher rollout.
Standalone downloads remain available. Previous `.2` archives and image pins
are retained for coordinated rollback, not advertised as compatible clients.
The launcher itself currently requires a new download to upgrade; v1 updates the
game and maps, not itself. Apple Developer ID signing/notarization is still needed
for a normal Gatekeeper first-launch experience; Ed25519 update signatures and
local ad-hoc code signing are not substitutes for Apple notarization.

## Architecture and trust

`crates/launcher` has no game-core, server or protocol dependency. It uses native
egui/Glow on Mac/Linux and egui/wgpu on Windows (DX12-capable, including software
adapters). OpenGL-only Windows failed on the test VM's 1.1 display driver, so do
not revert Windows to Glow. Platforms: Apple Silicon macOS, Intel macOS (`macos-x64`, its own
feed and launcher build), Windows x64, Linux x64. Linux requires
a working desktop OpenGL stack. Builds alone do not establish VM/runtime support.

The fixed HTTPS feed is `https://peakrunner.net/updates/v1`. Each platform's
`latest.json` contains an Ed25519-signed raw JSON payload. The launcher verifies
the signature against `crates/launcher/release-public-key.hex` before accepting
any paths, sizes, hashes or release notes. Files live at `blobs/<sha256>`.
No redirects or arbitrary download URLs are accepted. Paths, counts, sizes,
symlinks and Windows reparse points are restricted. Every file is hashed before
Play. Automatic sequence downgrades are refused, including after manual rollback.

The private signing key is kept off the repository on the maintainer's machine,
mode 0600. Back it up in a secure secret store outside this repository.
Never upload it to the website, server, Git or release artifacts. Replacing the
public key requires distributing a new trusted launcher. Losing/compromising the
private key requires key rotation; there is no online key-rotation scheme in v1.

Updates reuse verified files/cache entries and download only changed **whole
files**, not binary patches. Map-only releases do not need to download the game
executable. New files are staged in a fresh generation, verified, then selected
using an immutable selection record; the old install survives interrupted work.
Repair restores from verified cache or the signed feed. Rollback verifies the
previous installation, which may no longer match live servers. Cache, previous
generations, failed stages and logs are retained; automatic garbage collection is
not implemented yet. Budget disk space for multiple full installations.

Default data locations: macOS `~/Library/Application Support/PeakRunnerLauncher`,
Windows `%LOCALAPPDATA%/PeakRunnerLauncher`, Linux
`${XDG_DATA_HOME:-~/.local/share}/PeakRunnerLauncher`. An absolute
`PEAKRUNNER_LAUNCHER_DATA_DIR` selects an isolated store for QA/portable use without
changing the pinned public key or feed. Game output goes to `game.log` there.

## The window

A banner across the top (Ashveil's skull tower at dusk, captured in the
game: `crates/launcher/assets/banner.png`) carries the name, the platform and
links to peakrunner.net, the YouTube channel and r/peakrunner. One card says what state the game is in (ready to install, an update
available, ready to play, running) with the one button that does the next
thing: Install, Update or Play. Check for updates, Repair and Restore previous
sit beside it as quiet text buttons; restoring asks first. Below, two cards:
the newest release's notes and the servers online (map and mode, players).
A footer line explains updates (hovering shows the game folder) and links to
help. `QA_LAUNCHER_STATE=installed|update` (with the `smoke` example) shows
the installed and one-behind looks for captures.

## Build and stage (no deployment)

1. `cargo test -p peakrunner-launcher --lib`
2. Build launcher: `cargo build --release -p peakrunner-launcher --bin peakrunner-launcher`.
3. Build managed game: `cargo build --release -p peakrunner --bin peakrunner --features external-map`.
   Copy the result before building a normal standalone client in the same target
   directory. Managed clients intentionally require their external map pack.
4. Run `sh scripts/stage-launcher-release.sh PLATFORM ABSOLUTE_GAME_BINARY PRIVATE_KEY_PATH SEQUENCE NOTES_FILE [ABSOLUTE_SERVER_BINARY]`.
   With the server binary, the feed carries `game/peakrunner-server[.exe]` for Host game; the launcher
   marks only the game executable runnable, and the game sets the server's mode before starting it.
   Use increasing sequence numbers and review notes. The publisher refuses a key
   that differs from the launcher's pinned public key and refuses existing output.
5. Package with `sh scripts/package-launcher.sh PLATFORM ABSOLUTE_LAUNCHER_BINARY UNIQUE_REVISION`.
   Cross-build per platform using the existing client toolchains. Mac packaging
   signs the complete app only after assembling it.

Publisher helpers: `launcher-release public-key KEY_PATH` prints only the public
key. `keygen NEW_KEY_PATH` creates a new private key without overwriting one; do
not rotate the production key casually. Offline release QA uses
`launcher-release install-local MANIFEST BLOB_DIRECTORY ABSOLUTE_STORE_DIRECTORY`;
this still requires the pinned signature and native platform and follows the same
transactional installer. It does not bypass verification.

## Deployment gate

Runtime-test each launcher and its external-map game, plus offline Play, Repair,
Rollback and interrupted update. Deploy the compatible match server first, then
publish all signed blobs, and **last** atomically publish each platform manifest.
Serve manifests with `Cache-Control: no-store` and SHA-addressed blobs with long
immutable caching (routes are staged in `site/Caddyfile`). Prepare the feed at
`/data/peakrunner/updates/v1`; the production site mounts it read-only.
Rebuild/pin the site image for route changes; changing
local source does not deploy it. Confirm
public signature/hash verification before adding launcher links to Downloads.
Keep standalone downloads as recovery options. The launcher reads hosts directly
from `https://dir.peakrunner.net/servers`; it does not tunnel gameplay traffic.

## Local verification, 2026-09-19

- Seven updater regressions cover signed-manifest rejection, traversal, symlink
  escape, changed-file reuse, failed downloads, repair, rollback, locking and
  staging-name uniqueness under concurrent tests.
- Workspace library tests and dependency-boundary checks passed. The first
  sandboxed server test attempt could not bind sockets; the network-enabled rerun
  passed. Normal WebAssembly client checks still pass (existing Lobby warning).
- macOS native launcher screenshots were inspected. The live directory returned
  Springdale Central; the unconfigured public update feed correctly returned 404.
- A signed external-map macOS client was installed and launched from an isolated
  store; its independent game lock was held while running and released on exit.
- Windows cross-target checks/builds are available, but Windows and Linux launcher
  runtime testing and Linux packaging remain release gates, not verified claims.
- A repeat signed install with unchanged payload reused all eight cached files.
  Signing keys were confirmed ignored by Git. Nothing was deployed.

## Published verification, 2026-09-19 (continuation)

- Mac, Windows x64 and Linux x64 launcher/standalone archives are published; all
  six public download hashes matched `site/public/release.json`.
- All three public platform manifests and all their blobs passed the pinned-key
  verifier (`launcher-release check-public PLATFORM`). Manifests return no-store.
  `install-latest ABSOLUTE_STORE` supports an isolated end-to-end public-feed check.
  A fresh public Mac install launched successfully and held/released its game lock.
- Linux's seven updater tests passed. Real launcher rendering, signed external-map
  installation and actual managed-game process locking passed under Xvfb/Mesa.
  A local-match screenshot was inspected. No sound device or interactive gameplay
  certification is implied. Missing XDG_RUNTIME_DIR/ALSA device warnings were confined
  to the headless test environment.
- Windows 11 ARM rendered the x64 launcher and actual external-map Old Holler game.
  The initial OpenGL backend failed on its OpenGL 1.1 driver; Windows now uses wgpu.
  The signed installer and child-process lock test passed. A PowerShell wrapper
  lost its child's exit code; its handle-retention fix was followed by UTM/session
  instability, so a clean full-script rerun remains unverified. Session-0 graphics
  launch was unsuccessful and is not supported QA. Native x64 hardware, audio,
  controls and extended matches still need testing. Keep Windows experimental.
- Server `.3` passed a real two-client 10-second encrypted WAN test: max input ack
  gap 9 ticks, max snapshot gap 101 ms, 196/198 snapshots. No players were present
  at the restart. Site/server healthy; directory and tunnel were not recreated.
- Desktop/mobile website checks passed; six download cards, live `.3` host details,
  no browser errors or horizontal overflow. See `screenshots/launcher-public-*`.

The Linux launcher alone builds from the release recipe's caches on the build
host: `docker build -f deploy/client-linux-release.Dockerfile --target
launcher-artifacts --output type=local,dest=DIR .` (the release's
`--target artifacts` is unchanged).

Linux builds: use `deploy/launcher-linux.Dockerfile --target artifacts --output
type=local,dest=...` to export only binaries. The initial full compiler-image export
was slow; the artifacts target reused its completed cache. Exact Linux/server
source archive: `/data/peakrunner/releases/20260919-launcher1/source.tar.gz`.
The Windows renderer correction is in the later launcher source snapshot, not that
initial Linux/server archive. See the release provenance record.

Each release's rollback records are kept on the production host under
`/opt/peakrunner/backups/<tag>/` and `/data/peakrunner/public/backups/<tag>/`
(signed manifests included). A gameplay rollback requires
matching client/download/feed changes, not only restoring the server image.
Previously accepted feed sequences cannot be automatically downgraded: sign a
higher-sequence recovery release. Never discard the private key. Downloads
older than the live release are withdrawn; recover forward with a newer signed
sequence.
