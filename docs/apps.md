# Focused applications

PeakRunner stays in one Rust workspace, but each application has its own package,
entry point, production dependency graph, and release artifact.

| App | Package / source | Job | Release artifact |
| --- | --- | --- | --- |
| Client | `peakrunner`, `src/` | Rendering, audio, menus, input, prediction, offline play | `peakrunner` / `PeakRunner.app` |
| Match server | `peakrunner-server`, `crates/server/` | Authoritative matches and encrypted UDP | `peakrunner-server`; server-only Docker image |
| Directory | `peakrunner-directory`, `crates/directory/` | HTTPS-facing listings and status polling | `peakrunner-directory`; directory-only Docker image |

Shared libraries are deliberately narrower than the applications:

- `peakrunner-core`: simulation, match rules, terrain and material sampling.
  Used by the client and server only; no networking, graphics or audio runtime.
- `peakrunner-protocol`: gameplay messages and bounded snapshot/input codecs.
  Used by the client networking library and server, never by the directory.
- `peakrunner-discovery`: listing/status types, generic framing, verified TLS/QUIC
  control and discovery clients. No gameplay dependency or game-server listener.
- `peakrunner-net`: client sessions, server browsing, and client transports only.

The directory does **not** import the gameplay core, gameplay protocol, client,
or server. It asks the configured server for status and publishes a bounded,
expiring listing. It never simulates players or receives snapshots. The shared
discovery types preserve the existing `peakrunner-4` wire format and ALPN.

## Build and test independently

```sh
cargo build --locked --release -p peakrunner --bin peakrunner
cargo build --locked --release -p peakrunner-server --bin peakrunner-server
cargo build --locked --release -p peakrunner-directory --bin peakrunner-directory
node scripts/check-app-boundaries.mjs
cargo test --workspace --lib
cargo test -p peakrunner-server eight_clients_sustain -- --ignored
```

Server tests intentionally use the client and directory as **dev-dependencies**
for real integration checks. They are not linked into production server builds.
The dependency-boundary check excludes dev-dependencies and fails if app
responsibilities become coupled again.

## Hosting

The client never runs a match in-process. Its **Host game** screen
(`src/host.rs`) starts the separate `peakrunner-server` shipped beside it
(inside `PeakRunner.app/Contents/MacOS/`, next to `PeakRunner.exe`, or next to
`peakrunner`): it writes that server's own `server.json` in
`PeakRunner/host/` beside the client settings, passes a fresh host key
(`PEAKRUNNER_HOST_KEY`) and any match password through the environment,
joins once the server prints its ready line with its key's fingerprint
(pinned, over the private pipe), and shows the server's messages and invite
link. The host key lets that client start the match with one team empty
(`ClientMsg::StartMatch`, marker `host1`). The server exits when its stdin
closes, so quitting or crashing the client stops a server it started; a
dedicated host runs `peakrunner-server` on its own.

- Public encrypted host: `peakrunner-server`. Run it at a terminal and it asks
  a few questions, saves `server.json` and generates its own key (no domain or
  certificate needed; see [Host a server](multiplayer.md#host-a-server)).
  Deployments configure it by environment instead: `PEAKRUNNER_MATCH_MAP`,
  `PEAKRUNNER_MATCH_PASSWORD`, `PEAKRUNNER_UDP_BIND`, `PEAKRUNNER_TLS_CERT`,
  and `PEAKRUNNER_TLS_KEY`, which also override `server.json`. Its container
  image: `crates/server/Dockerfile`.
- Public directory: `peakrunner-directory`, configured by
  `PEAKRUNNER_HTTP_BIND`, `PEAKRUNNER_STATUS_URL`, `PEAKRUNNER_PUBLIC_URL`,
  and `PEAKRUNNER_TRUST_CLOUDFLARE`. Its container image:
  `crates/directory/Dockerfile`.
- Trusted LAN host: `cargo run -p peakrunner-server --bin peakrunner-lan-server --
  --bind 127.0.0.1 --port 7781 --map old-holler`. Never expose plaintext LAN mode
  to the internet. Clients use **Find match → Join directly**.
- Optional LAN directory: `peakrunner-lan-directory` in the directory package.
- Legacy WebSocket host: `peakrunner-websocket-server` in the server package.
  Kept for compatibility tests; not used by the public UDP deployment.

The old multi-purpose `peakrunner-public` and `peakrunner-quic` executables are
replaced by the dedicated directory and server apps. In particular, the name
`peakrunner-server` now means the encrypted public host; old LAN commands must
use `peakrunner-lan-server` explicitly. Deployments must update commands and
health checks together with their image pins. Older compatible clients need no
protocol upgrade.

Both Dockerfiles use the workspace root as build context. The directory build
does not need the terrain asset; each runtime image contains only its own app.
There is no shared runtime container or requirement to deploy the two together.
