# PeakRunner

A fast team shooter about skiing and jetpacks: Capture the Flag, Capture &
Hold, Team Deathmatch, Rabbit, Team Rabbit, Football and duels on original
maps. Written in Rust: a native client (also a browser build for offline
play), an authoritative match server, a server directory and a launcher that
keeps installs updated.

This repository holds the source of each release. Downloads, the server list
and the player guide are at **[peakrunner.net](https://peakrunner.net)**.

## Play

Download the launcher (it installs and updates the game) or a standalone
build for macOS, Windows or Linux from
[peakrunner.net](https://peakrunner.net/#downloads). Choose **Find match** to
play online, or pick a mode and map to play offline with bots.

| Key | Action | Key | Action |
|---|---|---|---|
| WASD, mouse | move, look | Left click | fire (Football: pass) |
| Space (hold) | jump / ski | Right click | jet |
| 1 2 3 4 | disc, chaingun, grenade launcher, rifle | G / N | grenade / mine |
| E | use a station, zoom a rifle | Q (hold) | repair tool |
| B | deploy a pack | R | third-person view |
| T / Y | chat / team chat | V | map vote |
| J | switch team | Ctrl+K | respawn |
| M | full map | Tab | scoreboard |
| F3 | frame rate | Esc | menu |

Keys can be changed in **Controls** (main or pause menu).

## Host a server

Download the server for your platform and run it in a terminal (on Windows,
double-click it). It asks a few questions, saves them, tells you whether
players can reach it, and prints an invite link to share. Details, ports and
options: [docs/multiplayer.md](docs/multiplayer.md).

## Build from source

Everything builds from `src/` with Rust 1.95 or newer (the lockfile pins
every dependency):

```sh
cd src
cargo run --locked --release -p peakrunner --bin peakrunner                 # the game
cargo run --locked --release -p peakrunner-server --bin peakrunner-server   # a server
```

The checks to run before sending a change:

```sh
cargo test --workspace --lib
cargo check --workspace --all-targets
cargo check --target wasm32-unknown-unknown -p peakrunner --lib
node scripts/check-app-boundaries.mjs
npm --prefix site run check
```

Windows and Linux builds and packaging: [docs/client-builds.md](docs/client-builds.md).

## Maps, models and mods

- **Maps** are built from their sources in `src/maps/` by the Python builders
  in `src/scripts/` (Python 3 with `pip install -r scripts/requirements.txt`);
  the built packs are in `src/assets/maps/`. See
  [docs/map-pipeline.md](docs/map-pipeline.md) and each map's own doc.
- **Models** are in `src/assets/models/`; the hard-surface ones rebuild from
  their scripts in `src/scripts/models/` with Blender
  ([docs/model-pipeline.md](docs/model-pipeline.md)).
- **Mods and maps of your own** need no code: [docs/making-content.md](docs/making-content.md),
  the format reference [docs/mod-format.md](docs/mod-format.md), and the
  examples in `src/examples/mods/`.

## Where things are

| Path | Contents |
|---|---|
| `src/` | the Cargo workspace (client in `src/src`, crates in `src/crates`), maps, scripts, website |
| `src/crates/` | `core` (the shared simulation), `server`, `protocol`, `net`, `discovery`, `directory`, `launcher`, `script` |
| `docs/` | how things work: modes, maps, systems, formats |

## License

Free software: the code is under the GNU GPL version 3 ([LICENSE](LICENSE)),
the art and maps under CC BY-SA 4.0
([LICENSES/CC-BY-SA-4.0.txt](LICENSES/CC-BY-SA-4.0.txt)), and the PeakRunner
name and logo are reserved. [COPYRIGHT.md](COPYRIGHT.md) says which files are
which, and lets the maps and mods you make stay yours.
