# Native client builds

Client, match server and directory remain separate applications. The directory
does not need rebuilding for name/chat protocol changes. Deploy the matching
match server and client downloads together; old gameplay protocols are rejected.

## Build and package

On Apple Silicon:

```sh
cargo build --locked --release -p peakrunner --bin peakrunner
cargo build --locked --release -p peakrunner-server --bin peakrunner-server
sh scripts/package-client.sh macos-arm64 "$PWD/target/release/peakrunner" "" "$PWD/target/release/peakrunner-server"
```

The optional fourth argument is the server binary: client packages carry it
for **Host game** (`src/host.rs`), in `PeakRunner.app/Contents/MacOS/`
(sealed with the app), as `peakrunner-server.exe` next to `PeakRunner.exe`, or
as `peakrunner-server` next to `peakrunner`. Without it the Host screen says
the server app is missing. It roughly doubles a client download (about 36 to
68 MB compressed).

Intel Macs (`macos-x64`) cross-build on Apple Silicon with the
`x86_64-apple-darwin` target (`rustup target add x86_64-apple-darwin`; the
Xcode SDK has both architectures):

```sh
cargo build --locked --release --target x86_64-apple-darwin -p peakrunner --bin peakrunner
cargo build --locked --release --target x86_64-apple-darwin -p peakrunner-server --bin peakrunner-server
sh scripts/package-client.sh macos-x64 "$PWD/target/x86_64-apple-darwin/release/peakrunner" "" "$PWD/target/x86_64-apple-darwin/release/peakrunner-server"
```

The app is the same bundle as Apple Silicon's (macOS 12 or later). The Intel
linker leaves binaries unsigned, so `scripts/sign-mac-app.sh` ad-hoc signs the
bundled server before sealing the app. The Intel game, server and launcher
have run under Rosetta on Apple Silicon (a local match, the launcher against
the live feed), not yet on an Intel Mac.

Windows x86-64 cross-build from macOS uses the installed MinGW-w64 linker in
`.cargo/config.toml` and the `x86_64-pc-windows-gnu` Rust target:

```sh
cargo build --locked --release --target x86_64-pc-windows-gnu -p peakrunner --bin peakrunner
sh scripts/package-client.sh windows-x64 "$PWD/target/x86_64-pc-windows-gnu/release/peakrunner.exe" "" "$PWD/target/x86_64-pc-windows-gnu/release/peakrunner-server.exe"
```

Linux x86-64 builds on the build host (any Linux/amd64 Docker host) using the
Debian 12 recipe:

```sh
docker build -f deploy/client-linux.Dockerfile -t peakrunner/client-build:local .
```

Extract `/src/target/release/peakrunner` from the resulting image, then pass its
absolute path to `sh scripts/package-client.sh linux-x64`.

The match server builds for the same three targets
(`-p peakrunner-server --bin peakrunner-server`; on Linux, the server
Dockerfile's build stage or the client recipe with that package) and packages
with `sh scripts/package-server.sh <platform> <absolute binary path>` into
`out/releases/<version>/server-<platform>/` plus a
`PeakRunner-Server-…` archive with `assets/package-docs/server-readme.md`.
The Windows server cross-builds as a console program (so the setup questions
work when double-clicked); it has not been run on Windows. The packaging script
refuses to overwrite release directories, embeds no credentials and does not
publish anything. Packages live under the ignored `out/releases/<GAME_VERSION>/`.

## Icons

The apps' icons are the website's mark (`site/public/favicon.svg`'s two
slanted bars) on a dark tile: the game's plain, the Launcher's with an amber
download badge, the Server's with a rack badge. `scripts/build-icons.py`
draws them all into `assets/icons/` (Pillow and macOS's `iconutil`; it
refuses to overwrite, and rebuilds are byte-identical):

```sh
python3 scripts/build-icons.py [OUTPUT]
```

- **Mac bundles:** `package-client.sh`, `bundle-mac.sh` and
  `package-launcher.sh` copy `peakrunner.icns` or `launcher.icns` in as
  `Contents/Resources/AppIcon.icns` (`assets/Info.plist`'s `CFBundleIconFile`).
- **While they run:** the game and the Launcher set their window icon
  (`with_icon` in `src/main.rs` and `crates/launcher/src/main.rs`): on a Mac
  the dock's (`*-mac.png`, Apple's grid), which a launcher's game needs most,
  since it runs outside any app bundle; elsewhere the window's and taskbar's
  (`*.png`). Without one the toolkit shows its own logo. Every Linux client
  Dockerfile copies `assets/icons` for them.
- **Windows executables:** each package's `build.rs` (the game's, the
  Launcher's and the Server's) compiles its `.ico` with MinGW's
  `x86_64-w64-mingw32-windres` (`WINDRES` overrides it) for `windows-gnu`
  targets only, and links it into every binary of the package; elsewhere it
  does nothing.

## Verification

- `cargo test --workspace --lib` runs shared policy, chat-input, team privacy,
  transport and server tests. Socket tests need local networking permission.
- `cargo check --workspace --all-targets` checks all native targets/examples.
- `cargo check --target wasm32-unknown-unknown -p peakrunner --lib` checks web
  compatibility, but is not a browser release.
- `examples/launch_smoke.rs` delegates both application logic and rendering,
  captures the actual native window, then closes only its own test window.
  Set `QA_CAPTURE_PATH`; optionally `PEAKRUNNER_JOIN`, `QA_PAUSE=1` or
  `QA_CHAT=team` / `QA_CHAT=public` for a local test match.
- Linux launch tests can run under Xvfb and Mesa software Vulkan in an isolated
  container. Use `docker run --init` so Xvfb startup signals reach its wrapper.
  This proves rendering compatibility, not gameplay performance. The current
  Linux build passed this check after adding the X11 keyboard library; sound
  devices were absent, so audio and interactive Linux gameplay remain untested.
- Windows build and DLL inspection are not substitutes for Windows runtime QA.
  Use a Windows machine or VM with working accelerated graphics for release QA.

Mac packaging now ad-hoc signs the completed bundle (including its resource seal)
and verifies it with `codesign --verify --deep --strict`. This fixes the invalid
bundle signature; it does not provide Developer ID signing or Apple notarization.
Verify again after extracting the ZIP. No system-wide Gatekeeper bypass is needed.
An optional third package-script argument creates a new packaging revision without
overwriting a published archive or changing the gameplay version.

The playtest packages are published on peakrunner.net with a
matching server. The packaging commands alone do not change public
downloads or the live server. No VM is needed for compilation; native
Windows testing is still required before claiming Windows runtime support.

## Cached release builds on the build host

The server (`crates/server/Dockerfile`) and Linux client
(`deploy/client-linux-release.Dockerfile`) builds use BuildKit cache mounts
(`peakrunner-cargo-registry`, `peakrunner-cargo-git`, and a target cache per
recipe). Docker 23+ uses BuildKit by default, so a plain `docker build` on
the build host reuses downloaded crates and compiled dependencies from the previous
release; only the workspace crates recompile (a `git archive` gives every
file the commit time, so all workspace crates count as changed). The images
and exported binaries are unchanged in content. To start clean, remove the
caches with `docker builder prune --filter type=exec.cachemount` (this
removes only BuildKit cache mounts, never images).
