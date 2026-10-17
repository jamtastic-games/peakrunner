# Authoritative public and private matches

## Public deployment

The native client defaults to `https://dir.peakrunner.net/servers` and
`quic://play.peakrunner.net:7777`. These use validated public CA certificates;
there is no insecure certificate bypass or plaintext fallback. Community
servers without a CA certificate are authenticated by a pinned key instead
(see [Host a server](#host-a-server)); that is also verified TLS, not a bypass. Gameplay uses
QUIC datagrams over direct UDP with TLS 1.3; reliable streams carry admission
and status only. HTTPS directory responses are bounded, redirects are disabled,
and public listings must use QUIC. The official servers use a public CA
certificate for `play.peakrunner.net`; WAN, loss/jitter and 28-client checks
passed. Longer human multi-machine playtests are still needed before
competitive-readiness claims.

The production host runs the `directory` behind the Cloudflare tunnel and the
official `peakrunner-server`s with only their UDP ports published (7777–7780),
behind a game-port flood filter. Cloudflare handles HTTPS discovery, not
gameplay. Gameplay encryption runs from the native client to the server; the
local authoritative backend uses trusted loopback TCP. The server operator can
read game state. The official servers' deployment files and runbooks are
kept with their own operations, outside this repository.

The directory lists its configured backend, any further official servers in
`PEAKRUNNER_OFFICIAL_URLS` (comma-separated `quic://` URLs, polled like the
backend, so the Football and Bots matches can be listed without depending on
announce), and servers that announce themselves with `POST /announce`. An
announced server is listed only when its host (a name, or an IP literal) is
the address the request came from and a TLS 1.3 status query there succeeds;
see `crates/directory/src/lib.rs` for the caps: 256 announced servers in
all, 16 from one community address, and the official host's address up to
the whole listing (`per_address`; raised from 32 and 4 on 2026-10-14 so the
listing holds 100 servers and more; games before then refuse a listing of
more than 128 in `servers` or `vouched`, the new ones 512). CA-certified servers go in the
listing's `servers`; servers with their own key go in `vouched` with the key's
fingerprint (below). A new server's verification runs
at most one at a time per address (IPv6 per /64) and four at once; a listed
server's refresh (DNS only, 1 s timeout) has its own pool, so a flood of new
announces can't make listed servers lapse. Stale results expire.

The QUIC gateway uses address validation, caps connections (48 at once, of
which at most 32 are player sessions), player sessions per address (IPv6 per
/64; `PEAKRUNNER_SESSIONS_PER_IP`, 1–28, default 4, with two more connections
per address for a status query and a pending join; loopback is exempt so local
multi-client tests work), admission bursts (18 per address, replenishing one
per three seconds), input packet size (256 bytes), input rate, write duration
and idle duration. Raise `PEAKRUNNER_SESSIONS_PER_IP` for a LAN party behind
one NAT. A refused join gets a `Reject` with the reason ("server is full", "too
many players from your address", a wrong password, a full match) before the
connection closes.
Inputs redundantly include three numbered frames and acknowledge the newest
snapshot the client has decoded. Snapshots are quantized delta snapshots
(`delta1`, below), fragmented to the connection's current maximum datagram
size (1060–1452 bytes), bounded to 12 fragments (12.6 KB), with at most four
incomplete assemblies. Transmit queues are
16 KiB for snapshots and 1 KiB for inputs to avoid seconds of stale buffered state.
Only one snapshot is pending for transmission; intermediate snapshots are skipped
under congestion so fragments from multiple ticks do not crowd each other out.
Old, duplicate and incomplete snapshots never block a newer complete snapshot.
Zero-RTT is disabled. The old WSS implementation is not the public gameplay path.

The directory and gateway use health checks; Docker restarts exited processes.
An unhealthy-but-running container needs operator attention (Compose alone does
not automatically restart it). Logs record joins, departures and phase changes
without passwords. Reconnect is available in the disconnect screen; it joins as
a new session, not a restored identity. Credentials stay outside Git.

## What is implemented

- Up to 28 players (`PEAKRUNNER_MAX_PLAYERS`, default 8), stable
  connection-assigned IDs (display names may repeat).
- Auto-balanced teams; one server-selected map and shared 60 Hz physics.
- Disc, chaingun, and grenade simulation, damage, health, energy, cooldowns,
  blast impulses, deaths, respawns, flags, captures, scores, and round clock.
- Warmup until both teams exist, a ten-second countdown before every round
  (see "Round countdown" below), first to three captures or eight minutes,
  ten-second intermission, automatic restart. Late joining is supported.
  Disconnects drop carried flags and remove owned projectiles. Losing an entire
  team aborts the round into warmup with a fresh countdown on rejoin.
- Client-side movement prediction and input replay against acknowledged snapshots.
  Other players are drawn 100 ms behind the client's estimate of the server
  clock, between the two snapshots around that time (`src/online.rs`); if no
  newer snapshot has arrived they coast on their last velocity for up to
  100 ms, and a respawn, teleport, new round or map change snaps instead of
  sliding. In Football they are drawn ahead instead, coasted on from the
  newest snapshot to the moment of your own predicted body, a missed turn
  sliding over (`docs/football.md` "Body checks"). Projectile rendering is extrapolated between snapshots; clients do
  not resolve authoritative hits or captures. Snapshot indices (flag and ball
  carriers, projectile and point owners) and numbers are checked before a
  snapshot is applied: bad references are dropped and non-finite players
  refuse the snapshot, so a broken server can't crash a client.
- 20 Hz snapshots (deltas on QUIC), a Tab scoreboard, direct joining, optional directory,
  optional match password, and visible disconnect/rejection states.
- Focus loss and menus send neutral controls. The match never pauses for a client.

## Trust boundary

The server accepts a numbered control frame (movement axes, view angles, held
actions, weapon slot, and while fire is held the server time of the view the
client was drawing; in Football always, there the snapshot its drawing coasted
on from and how far; see laser lag compensation below). There are no position, velocity, health, damage, score,
or client-delta-time messages; team changes are requests the server may refuse
(`teams1`, below). Server time advances independently
of packet count. Inputs must be finite and in range with increasing sequence
numbers and bounded jumps; UDP loss does not require a disconnect. Rate, frame,
connection, and queue limits reject abusive peers.

Sockets are nonblocking with incremental framing; partial lines survive polls.
The game host takes at most 44 connected/pending peers, 28 player slots and
8 spectators.
Each player's commands go through a small jitter buffer
(`crates/server/src/input_buffer.rs`): one command is simulated per 60 Hz tick,
in order, because the client predicts exactly one step per command. A tick with
nothing queued repeats the held input (one-shot presses removed); a standing
backlog is trimmed one command at a time when the queue never drained for a
second, and a burst beyond 8 queued is cut at once. Presses in trimmed commands
(throw, buy, deploy, suicide, and jump/fire/interact edges) carry into the next
simulated command. TCP peers have 4 outgoing frames per socket and 8 KiB client
lines (Hello carries the compatibility marker). A 32-message burst allowance
absorbs jitter, while a sustained 120/s limit rejects floods. On QUIC, chat,
rename and vote messages allow a burst of 8 and then 2 a second; the excess is
dropped, and only a flood beyond 20 a second past a 100-message allowance
disconnects.
Handshakes expire after 3 seconds;
missing inputs become neutral after 250 ms and disconnect after 5 seconds.
Snapshots cannot exceed 256 KiB. Slow consumers disconnect instead of building
unbounded queues. Directory listings are bounded and require an owner token
for updates/removal; the advertised IP must match the registering peer.

**Legacy LAN hosting is not encrypted.** Its passwords and directory lease
tokens travel over plaintext TCP. That transport is suitable only within
a trusted network, preferably a private encrypted VPN with restricted members.
Only specific loopback, RFC1918, VPN shared-address-space, or IPv6 ULA binds are
accepted. That is an accidental-exposure guard, not protection against port
forwarding, proxies, malicious VPN members, packet interception, or DDoS.
Do not use a password you use anywhere else. The server operator is trusted.

## Host a server

### From the game

**Multiplayer → Host game** on the main menu starts the PeakRunner Server app bundled with
the game and puts you in its match. Pick a server name, a mode and maps (click
them in play order), player and bot counts, an optional password (never
saved) and the UDP port. **Duel** makes a duel server (`docs/duels.md`): its
maps are the ones duels are drawn on, and its bots spar with blades in the
lobby's tower. **Announce publicly** is checked by default and lists
the server under Find match; unchecked, the server isn't listed, asks the
server list once for your public address and shows an invite link to share.
Alone, the match waits in warmup: press Esc, then **Start match now** (only
the host sees it; the server checks the host's key). Others can join at any
time; the host's start holds across map changes until everyone has left.
**Stop server** on the Host screen, or quitting the game, stops it. Players on
the internet need UDP port forwarding to your computer; the server tries UPnP
and its messages on the Host screen say whether that worked. Settings are
kept in `PeakRunner/host/server.json`; the log of the last run is
`server.log` beside it.

### The server app

Anyone can host with just the `peakrunner-server` binary: no domain,
certificate, environment variables or settings file.

```text
$ ./peakrunner-server
PeakRunner server setup. Press Enter to accept the answer in [brackets].

Server name [PeakRunner Community]: Garage CTF

Game mode:
  1) Capture the Flag
  2) Capture & Hold
  3) Team Deathmatch
  4) Football (stadium maps)
  5) Capture the Flag, then Capture & Hold, on every map
  6) Rabbit
  7) Team Rabbit
Mode [1]:

Maps for Capture the Flag:
  1) Old Holler
  2) Tower Complex
  ...
Maps (numbers like 1,3, or all) [all]: 1,2,3

Max players (2-28) [16]: 12
Bots: fill the match with bots up to how many players? (no, or 2-12) [no]:
Time of day each round (default, random, map, dawn, day, dusk, night) [default]: random
Weather each round (default, random, clear, overcast, rain, storm, snow, fog) [default]: rain
Password players must enter (none for an open server) [none]:
UDP port [7777]:
Announce to the public server list? (yes/no) [yes]:
Location shown in the list, like Denver, US (none to leave out) [none]: Denver, US
Ask your router to open the port automatically (UPnP)? (yes/no) [yes]:

  Name:        Garage CTF
  Rotation:    Capture the Flag: Old Holler, Tower Complex, Cairnhold
  ...
Start with these settings? (Y/n) [Y]:
```

- **Time of day and weather:** `time_of_day` and `weather` in `server.json`
  (`PEAKRUNNER_TIME_OF_DAY`, `PEAKRUNNER_WEATHER`), the setup questions, or
  the Host screen's **Time of day** and **Weather** rows; offline, the menu's
  **Sky**. Each is `default` (as the mode does: the map's own look, while
  Team Deathmatch rolls its own), `random` (again each round), or one value:
  `map` (the map's own lighting), `dawn`, `day`, `dusk`, `night`; `clear`,
  `overcast`, `rain`, `storm`, `snow`, `fog`. They change only how the map
  looks (lighting, sky, fog, rain or snow); they never bring Team
  Deathmatch's gameplay twists to other modes. Code: `conditions::Forecast`,
  `World::roll_conditions`.
- **Rotation order:** `rotation_order` in `server.json`
  (`PEAKRUNNER_ROTATION_ORDER`): `in_order` (the default) plays the rotation
  as listed; `random` plays the same list in a new random order each time
  through, never starting a lap with the match just played. Votes still list
  the entries in their configured order. The server draws the order itself
  (`Rotation::shuffle`); the maps and modes come only from the rotation.
- **Config:** answers go to `server.json` in the per-user config directory
  (macOS `~/Library/Application Support/PeakRunner/server/`, Windows
  `%APPDATA%\PeakRunner\server\`, elsewhere `$XDG_CONFIG_HOME` or
  `~/.config/PeakRunner/server/`), or `--config-dir DIR` /
  `PEAKRUNNER_SERVER_DIR`. Later runs print "Using …/server.json; run with
  --setup to change it" and start. The file may hold the match password in
  plain text: it is the host's own config, written owner-only (0600 on Unix;
  on Windows it relies on the per-user profile folder), and
  `PEAKRUNNER_MATCH_PASSWORD` overrides it. The game client still never
  stores passwords. It is validated on load and a bad file is an error,
  never replaced. Code: `crates/server/src/setup.rs`.
- **No prompts without a person:** questions appear only when stdin and
  stdout are terminals, there is no `server.json`, and the environment
  doesn't already configure the match (`PEAKRUNNER_TLS_CERT`/`KEY`,
  `PEAKRUNNER_MATCH_ROTATION`, `PEAKRUNNER_MATCH_MAP`,
  `PEAKRUNNER_ADVERTISE_URL`). Docker and service managers never prompt.
  Environment variables override `server.json` field by field; with no file
  the server reads only the environment, exactly as before (health listener
  on `0.0.0.0:8080`, announce only with both `PEAKRUNNER_DIRECTORY_URL` and
  `PEAKRUNNER_ADVERTISE_URL`). With a file the health listener is on
  loopback only, TCP at the game port's number (`127.0.0.1:<port>`; a taken
  port is logged and skipped), unless `PEAKRUNNER_HEALTH_BIND` says
  otherwise; `--healthcheck` reads whichever the config gives.
- **Identity:** `PEAKRUNNER_TLS_CERT`/`PEAKRUNNER_TLS_KEY`, or the Docker
  mount `/run/peakrunner/`, keep working and take precedence (a configured
  but missing certificate is still an error). Otherwise the server generates
  a key and self-signed certificate on first run (`server-key.pem`,
  `server-cert.pem`, owner-only, next to `server.json`) and keeps them, so
  its fingerprint (SHA-256 of the certificate's DER) is stable. Code:
  `crates/server/src/identity.rs`.
- **Being found:** with announce on, the server asks the directory for its
  public address (`GET /whoami`, IPv4 first) unless `public_address` is set
  in `server.json`, announces `quic://<ip>:<port>` every 20 s, and says in
  plain words whether the directory reached it; if not, it prints UDP
  port-forwarding steps. It warns when the router's UPnP address differs
  from the directory's view (double NAT / carrier-grade NAT). Code:
  `crates/server/src/reach.rs`.
- **Unlisted servers** (announce off) contact no directory by default: they
  print the LAN invite link and "Run with --publicip to also show your public
  invite link (asks the server list for your address once)". With
  `--publicip` they log the request first, make one `GET /whoami` (4 s
  timeout per attempt), and print the public invite link (with a note that
  UDP `<port>` must reach the computer, and the UPnP status) and the LAN
  link. If that fails they fall back to the router's UPnP address when it is
  public, else to the LAN link with instructions. `public_address` in
  `server.json` makes the lookup unnecessary. Environment-only servers never
  make it.
- **UPnP:** optional (`upnp` in `server.json`), via `igd-next`: an hour
  lease on UDP `<port>` renewed every 30 minutes, removed at shutdown.
  Routers without UPnP just get the manual instructions.
- **Invite links:** the server prints `peakrunner://join/<host>:<port>#sha256=<hex>`
  (a LAN one, and a public one once the address is known). Players paste
  it into **Find match → Join by address… → Join**; `quic://host:port#sha256=<hex>` and `PEAKRUNNER_JOIN` work the
  same. A link without a fingerprint is refused: no trust on first use. The
  client keeps a pasted fingerprint in its settings (it isn't a secret; other
  fragments still aren't stored). The OS does not open `peakrunner://` links
  yet; they are pasted.
- **Packaging:** the server builds for the client's targets (Apple Silicon,
  Windows x64 cross-build, Linux x64 on the build host); `scripts/package-server.sh
  <platform> <binary>` makes an archive with `assets/package-docs/server-readme.md`
  under the ignored `out/releases/`. Nothing is published by it.

### Directory-vouched keys

A client authenticates a server one of two ways
(`peakrunner_discovery::quic::Trust`):

- **WebPKI:** a public CA certificate for the URL's host, as before. The
  official servers and anyone with a domain and certificate.
- **Pinned:** exactly one end-entity certificate, compared by SHA-256 of its
  DER bytes (`PinnedCert`). rustls still verifies the TLS 1.3
  CertificateVerify signature against that certificate with its standard
  `verify_tls13_signature`, so the peer must hold the private key. Names,
  chains and dates don't apply to a pin; the pin is the identity. This is not
  disabling verification: an unknown key is always refused, and the pin
  comes from a channel the player already trusts: the directory's
  CA-verified HTTPS listing, or an invite link from the host.

The directory's first probe of an announced server (`identify_status_at`)
accepts a CA certificate valid for the URL's host (listed in `servers`) or
records the fingerprint of any other certificate whose handshake signature
verified (listed in `vouched`), and only after the status query validates.
Every later poll pins that identity, so a server whose key changes stops
answering them; once its status is stale (12 s) its next announce is
re-verified with a fresh probe, which records the new key. A new key never
replaces a listed one without that probe. All earlier checks stay: announces
only for the requester's own public address (an IP literal must equal it,
IPv6 exactly), probes only to that address on ports ≥ 1024, per-address
rate limits and caps, the verification pools, the reserved "springdale"
name prefix, TTL and refresh. Announce replies carry an `AnnounceReply` body
(`listed`, `unreachable`, `wrong_address`, …) explaining the unchanged status
codes.

What the directory vouches for: "whoever controls this IP address and port
(as seen from the directory) holds this key and runs a PeakRunner status
endpoint". Not who the operator is, and not that the game is fair.

**Website:** `site/public/app.js` lists `vouched` servers in a separate
"Community servers" group under the official ones, and their details show
the invite link to copy. Entries
without a well-formed `quic://` host and lowercase 64-hex `sha256` are
skipped; all server-supplied text is set with `textContent`. For a local
preview against a test directory: `PORT=18180
PEAKRUNNER_DIRECTORY=http://127.0.0.1:<port> npm --prefix site run dev`
(defaults: port 8080, the live directory).

**Compatibility:** released clients (up to `.20260926.5`) and the released
website read only `servers`, which still holds exactly the CA-certified servers in
the old format; they ignore `vouched` (their listing types don't deny unknown
fields; `released_clients_see_exactly_the_ca_certified_servers` checks a
copy of them). New clients show both, vouched ones marked "community host".
`game_protocol()` is unchanged: this is transport trust, not gameplay.

**Browser build:** the WebAssembly client has no online play (no QUIC, no
listing), so it neither needs nor does pinning.

## Maps a server needs (`content1`, source only)

A server's status lists every map its rotation (and so every vote) can reach:
key, menu name, fingerprint (the SHA-256 of `map.json`) and download size
(`MatchStatus::content`, at most 32 maps so a status with a full roster stays
under 16 KiB). Joining, the game:

1. asks for the status (over QUIC, or on the same LAN TCP connection before
   `Hello`);
2. binds each key to the map with exactly that fingerprint, installed or
   already downloaded;
3. for anything missing, asks the player ("This server needs 1 map you don't
   have: River Fort (24.0 MB)", Download and join / Cancel). The preference
   `auto_download_mb` (client.json, default 0) skips the question for smaller
   downloads;
4. downloads over a separate QUIC connection to the same port, trusted exactly
   as the join (`ClientMsg::Fetch`, one file per stream, large receive
   windows): `map.json` first, which must hash to the fingerprint, then the
   payloads it lists (only known payload names) and `info.json`, never more
   bytes than the list gave the map but for an `info.json` of up to 16 KiB
   (a file past that is refused before it is read, so a server can't slip a
   big download past the question with a small size; a server lists a map
   under two keys at the size of the files it sends for both, the first
   key's). A mod's files must make the very listing it was fetched by. The
   whole map is then validated as the game loads maps before it is
   registered;
5. says `Hello` with `content_digest` of the list; the server refuses a
   mismatch with a message that says to join again from the game.

Downloads live in a per-user cache (macOS `~/Library/Application Support/PeakRunner/downloads`,
Windows `%LOCALAPPDATA%\PeakRunner\downloads`, Linux `~/.local/share/PeakRunner/downloads`),
at most 2 GB, and 400 maps and 100 mods besides what the server being joined lists (the oldest dropped first, never
one that server lists under its key; a list naming a map or mod twice is refused), never listed in menus and never answering a
key outside a session with the server that named them. A download connection holds
no player seat, is capped at 1,024 files, 2 GiB and 15 minutes, and can fetch only files of the
maps and mods in that server's own list. At most six download connections run at
once (more are told the server is busy): a download holds one of the server's 48
connection slots for as long as its client keeps asking, so a few addresses asking
slowly could otherwise have kept players and the directory's status probes out.
Map files go out from disk in 64 KiB pieces, each within 30 s (a client that stops
reading is let go), never whole in memory. LAN servers can't send maps: a LAN join that lacks
one ends with the missing names. Test: `crates/server/tests/content_download.rs`
(the real server app; decline, approve and download, join from the cache).

A server finds the maps and mods it offers in its content folders:
`PEAKRUNNER_PACK_DIR`, then `content/` beside its `server.json` (the setup
wizard creates it; the Host game's server uses the game's own `content/`),
then the installed `packs/`. `peakrunner-server --check` checks that folder
before players see it. The official servers mount `/data/peakrunner/content`
(see their own runbooks). Authoring:
`docs/making-content.md`.

## Legacy LAN and headless hosting

Hosting now belongs to the separate server app. Clients use **Find match → Join
by address…**; closing the client does not stop the server. The directory is a third
independent app with no simulation dependency. See [app boundaries](apps.md).

For a trusted-LAN headless host:

Build from the `peakrunner` directory:

```sh
cargo build --release -p peakrunner-server -p peakrunner-directory
target/release/peakrunner-lan-server --bind 127.0.0.1 --port 7781 --name "North Spine" --map old-holler
```

Replace the loopback bind with the host's specific private LAN/VPN address to
allow friends to connect. The server is headless: its dependency tree contains
no GPU, windowing, or audio libraries. Linux/Windows binaries should be built
on those targets; this pass does not certify an untested OS build.

Set `PEAKRUNNER_MATCH_PASSWORD` in the server process environment for an optional
invite password. Players enter that password in Find match. Keep it out of
source control and do not expose the service directly to the internet.

Optional directory, on the same private network:

```sh
target/release/peakrunner-lan-directory 127.0.0.1:7780
target/release/peakrunner-lan-server --bind 127.0.0.1 --name "North Spine" --map old-holler --directory 127.0.0.1:7780
```

Change both addresses for a LAN/VPN setup. `--advertise` may specify the server
IP explicitly; it must match the connection's source IP seen by the directory.
Listings heartbeat every two seconds and expire after eight seconds. There is
no automatic deployment or assumption that a previously used host is running.

Native clients optionally accept `PEAKRUNNER_JOIN=quic://play.peakrunner.net:7777`
(or a private `IP:PORT`), `PEAKRUNNER_NAME`,
and `PEAKRUNNER_MATCH_PASSWORD` at launch for direct session entry.

The separate server and directory Dockerfiles use the `peakrunner` workspace
as build context. The directory's HTTP origin belongs on the isolated tunnel
network. Only the encrypted server's UDP port is published.

## Verification

```sh
cargo test --workspace
cargo test -p peakrunner-server eight_clients_sustain -- --ignored --nocapture
node scripts/check-app-boundaries.mjs
cargo test --release -p peakrunner --lib -- --include-ignored
cargo check --target wasm32-unknown-unknown
```

Tests cover shared snapshots, duplicate display names, all eight slots,
password rejection, malformed/oversized traffic, listing ownership, partial
frames, authoritative airborne movement, all three weapons' damage and respawn,
flag drops/captures, late joining, round restart, and delayed input reconciliation.
Existing movement/collision regressions remain in the shared core.

Local verification for this implementation: 62 tests passed including GPU
captures and the 12-second eight-client load test (about 60 server ticks/s;
largest observed snapshot about 24 KiB). Two native game windows were checked
for shared matches, live movement, the scoreboard, UI hosting, guest joining,
and host-disconnect handling. The native release build and WASM compile passed.
This is a short local check, not a long-running or cross-machine certification.

## Remaining before public competitive hosting

1. Verified player accounts and persistent reconnect identities if requested.
   TLS verifies the service, not the player. Public sessions are anonymous.
2. Longer latency/loss testing on real networks. Public gameplay now uses
   quantized delta snapshots over QUIC datagrams, not TCP/WSS. Only the laser rifle is lag
   compensated (below); projectiles are simulated forward on the server. Short
   smoke tests are not competitive certification.
3. Operator tools: kick/ban, reconnect identity, team management, readiness,
   map rotation, structured match logs, and durable results if requested.
4. Longer multi-machine soak tests and profiling on low-end clients/servers.
   Local eight-client tests are not evidence of internet or cross-OS performance.

Server authority prevents clients from directly inventing movement or damage.
It does not prevent aim assistance, collusion, information cheats, or a dishonest
server operator. No anti-cheat or exploit-proof claim is made.

## Map votes (`vote1`) and empty servers (2026-09-26)

- **V** calls a vote for one of the server's rotation entries (the snapshot's
  `vote_choices`; the current match is greyed out) or answers the running
  one. Rules live in `crates/core/src/vote.rs`: one vote at a time, 60 s,
  passes early on a majority of players, fails early when half say no, else
  yes must beat no; 2-minute cooldown per caller; 20 s after a passed vote and 5 minutes after a failed one before the next (the panel shows the wait). A
  passed vote switches at once (into a new round's 10 s countdown on the new
  map) and moves the rotation cursor to that entry.
  Clients send `CallVote`/`Vote`; the QUIC relay forwards both. The banner
  pulses amber under the score; a whistle marks the start. Tests:
  `vote::tests`, `a_called_vote_that_passes_switches_the_map_and_mode`.
- **Empty servers keep their current map** (they no longer reset to the
  first rotation entry); the rotation continues from there.
- **Map-change fix:** players are rebuilt as on join after a rotation (before,
  the new match's remote placeholders ignored all input until a rejoin).
- **Inventory online:** the inventory screen's station use and purchases are
  sent while it's open (they were dropped with movement input).

## Server bots (2026-09-26)

`PEAKRUNNER_BOTS=N` (or `GameHost::with_bots`) keeps a match at N players with
bots, N/2 a side. Bots carry net IDs from `BOT_ID_BASE`; `sim::human(p)` tells
people apart for admission, team balance and votes. A joining person takes a
bot's slot (their team's first); leaving brings a bot back; map changes re-add
bots. Bots fly their own wishes on the server (their network input slot is
ignored). Compose services `bots` ("Boticus Maximus", UDP 7779) and, since
2026-10-14, `football-bots` ("Springdale Football Bots", UDP 7781; the
people-only "Springdale Football" stays on 7778), 8 a side on Boticus
Maximus and 6 a side on Springdale Football Bots (12 bots since 2026-10-14). In
Football bots play their own jobs and go by football names (`docs/football.md`
"Bots"). Test: `a_bot_server_plays_by_itself_and_people_take_bots_places`.

Bots count as players in the server lists: the status's `players` counts
everyone playing (at most `max_players`: balance can seat one bot past it,
and directories drop a status over its capacity), `MatchStatus::bots` says
how many of them are bots, and the roster lists them with `"bot": true`. The
directory passes the count on (`ServerAdvert::bots`, left out at zero, which
released clients ignore). Find match calls a server full only when people
fill it, and hovering a count with bots splits it; the website marks bots in
the roster as the scoreboard does ("· bot"). The official release script
counts only people, so it needs a directory that passes `bots` on. The player counts below stay
people only (bots send no `Hello`). Test:
`a_bot_server_lists_its_bots_as_players_and_still_takes_people`.

## Duel servers (`duel1`, 2026-10-14, branch `duel/arenas`)

A rotation of `duel` entries (`[{"playlist":"duel"}]`, or the setup wizard's
"Duels") makes a duel server: a lobby everyone joins, and an arena per
accepted challenge, each its own match on the same server and clock, so
duels play side by side. People challenge each other, or a bot, from the
duel board (the map-vote key), and watch duels from it; the server moves
everyone (`crates/server/src/duels.rs`). `PEAKRUNNER_MAX_PLAYERS` counts the
lobby, the duelists and those away watching; `PEAKRUNNER_BOTS` is how many
sparring bots fight with blades in the lobby's tower.
The lobby is Tenfold Tower (`docs/tenfold.md`), ten floors of the blade side
game (`docs/blade.md`); the status reads `Tenfold Tower - DUEL`. Everything
else: `docs/duels.md`.

## Player counts (2026-10-11)

The website shows how many people have played on the official servers, in
all and today (UTC). A joining player's game sends an anonymous code
(`ClientMsg::Hello::player`, marker `players1`; see
`docs/client-preferences.md`). A server with `PEAKRUNNER_STATS_DIR` set (the
official ones: `/data/peakrunner/stats`, set up by their own runbooks) keeps only a salted SHA-256 of it (`peakrunner-server` `stats.rs`), by
day and in total, counting a player once they play (a watcher once they take
a place). The host's servers share the folder: each writes `seen-<name>.json`
and then all of them combined as `public/stats.json` (`{total, today, day,
updated}`), on joins and every minute; the site serves that folder at
`/stats/` and its home page reads it (`app.js` `played()`). The salt stays in
the folder, never in `public/`. Unset (community and Host game servers), the
server counts nothing; a folder it can't use is logged and play goes on.

## Server locations (2026-09-26)

`PEAKRUNNER_LOCATION` ("New York, US"; `GameHost::with_location`) is sent in
the match status and copied by the directory into each listing's `location`
(`peakrunner_discovery::valid_location`: up to 40 letters, digits, spaces,
commas, periods, hyphens). Additive: older directories and clients ignore the
field. The website cards and details and the in-game server list show it.

## Networking fixes (2026-09-27, source only)

No wire or compatibility change: `game_protocol()` is byte-identical, and old
clients work with new servers and the reverse.

- **Input jitter buffer** (server): one command per tick instead of the newest
  queued one. In a model of the input path against the real sim
  (`input_buffer::tests`), 7 ms frame and 15 ms network jitter used to leave
  21% of commands unsimulated and 42% of snapshots correcting the local player
  by more than 5 cm; now 100% are simulated and 0% correct by more than 5 cm,
  for about 4 ms more input-to-acknowledgement latency.
- **Remote player interpolation** (client), described above.
- **In-process relay link** (server): the QUIC relays and the game thread share
  channels (`crates/server/src/link.rs`) instead of loopback TCP JSON. Each
  snapshot view (Ember, Glacier, no team; team chat is still filtered in
  `Match::snapshot_for`) is encoded once into datagram fragments with the
  relays' RTTs, and every peer with that view gets the same bytes. The relays
  and the client's network thread wake on messages instead of 2 ms timers.
  Loopback TCP remains for LAN hosting and tests.
- **Snapshots always go out:** if projectiles alone overflow the compressed cap,
  the oldest half is dropped until it fits (`packets::chunks_fitting`).
- **Status replies** may be up to 16 KiB (`quic::STATUS_LIMIT`; other control
  messages keep 4 KiB); a 28-player roster with long names is about 3.7 KB.
  Deploy the directory first: an older directory can't read a reply over 4 KiB.
- **One vote per address** (`vote.rs`): sessions from one address (IPv6 per
  /64) share one ballot, count once among the voters and share the call
  cooldown, which a reconnect no longer resets. LAN (TCP) players count alone.
- New settings: `PEAKRUNNER_SESSIONS_PER_IP` (match server) and
  `PEAKRUNNER_OFFICIAL_URLS` (directory). Neither is set in the deploy files yet.

## Round countdown (2026-09-27, source only)

No wire or compatibility change: `game_protocol()` is byte-identical; the
snapshot already carried `phase` and `phase_left`.

- **Every round start counts down `ROUND_COUNTDOWN` (10 s)** on every map and
  mode (`crates/core/src/matchplay.rs`): the first start (warmup to both
  teams present), after each intermission (with or without a rotation to the
  next map) and after a passed vote's map switch. Football's first start keeps
  the classic 20 s warmup (`football::WARMUP_SECONDS`); its later rounds count
  down 10 s like everything else.
- **One teleport, at the start.** `Match::begin_countdown` restarts the round
  (flags, points, equipment, scores, clock) and respawns everyone when the
  countdown begins; a rotation (`rotate_to_mode`) respawns on the new map and
  goes straight into the countdown. Play begins on the tick the countdown
  reaches zero, from where everyone stands. The round number changes at the
  countdown's start, so clients reset prediction at the teleport and carry it
  straight on into play (no rubber band at the start).
- **Frozen.** During the countdown inputs are zeroed (look still turns), and
  `World::countdown` holds every player still, bots included: only movement
  settling and body contact run. No firing, throws, purchases, deploys,
  suicides, captures, turret fire or scoring; the match clock waits. A team
  emptying during the countdown falls back to warmup, and the next opponent
  starts a full countdown again.
- **HUD** (`src/countdown_hud.rs`): a large centred 10 … 1, then a brief GO,
  under "ROUND STARTS IN" and over the map and mode. Online it follows
  `phase_left`, run down smoothly between snapshots by the client's server
  clock estimate (`Online::countdown_left`); a click each second and the start
  cue on GO (existing synthesized cues).
- **Offline** matches from the menu count down the same 10 s
  (`World::start_counted_match`), players and bots held still, clock stopped.
- Older clients on a new server see the old "MATCH STARTS IN n" message for
  10 s; new clients on an older server show its 3 s countdown.
- Tests: `matchplay::countdown_tests` (every entry into play, spawns once,
  frozen inputs and bots, no scoring, team emptying, Football's first warmup,
  offline), `online::tests::the_countdown_runs_down_smoothly_between_snapshots`,
  `play_starts_from_the_countdown_without_a_rubber_band`,
  `countdown_hud::tests`, and the ignored `rotation_cycles_connected_clients`
  (each new map starts in Countdown).

## Laser lag compensation (`lagcomp1`, source only)

The laser rifle hits instantly, so it used to be tested against the server's
current positions: a shooter had to lead a moving target by the time it takes
their shot to reach the server plus the 100 ms that other players are drawn
behind (about 220 ms on a 100 ms round trip, over 4 m for a 20 m/s skier).

- **Client** (`src/online.rs`): each command that holds fire carries `view`,
  the server time the other players on screen were drawn at
  (`clock - INTERP_DELAY`), in sixteenths of a tick. In Football every
  command carries one (`claimed_view`): the newest snapshot's time, with
  `ahead`, how far past it the others were coasted (`Online::claim`). Other
  commands send 0.
- **Server** (`crates/core/src/lagcomp.rs`, `World::lag`): `Match::step`
  records every player's position and velocity at the end of each tick,
  keeping 16 ticks, and passes this tick's claims to the sim (people only:
  bots send none). A laser shot is tested against the other players where
  they were at the claimed time, interpolated between the two ticks around
  it. The shooter's eye and aim, the map, deployables, equipment, mines and
  damage all stay as they are now.
- **Clamped, never trusted:** a claim in the future counts as the present, and
  one older than `MAX_REWIND_TICKS` (15 ticks, 250 ms) counts as 250 ms. A
  `view` above `MAX_VIEW` fails `Command::valid` (the input packet is
  refused). The client names a time only, never a victim or a hit; cooldown,
  energy and damage are unchanged. A victim who died, respawned or teleported
  since the claimed time (a step farther than their speed allows, a different
  player in the slot) is tested where they are now.
- **Why 250 ms:** the 100 ms drawing delay, a round trip of up to about
  120 ms and a tick or two of input buffering. Players on slower connections
  lead by the excess only. A longer window would let a target be hit longer
  after ducking behind cover.
- **Caveats:** the victim is shot where the shooter saw them, so they can be
  hit up to 250 ms after reaching cover ("shot around a corner"). The shooter
  itself is not rewound. A carried trigger press (input buffer trim) keeps its
  own view. Projectiles (disc, chaingun, grenades, mortar, railgun slug) are
  unchanged: they fly forward on the server and need leading.
- **Football body checks on bots** (`ball6`, since 2026-10-15;
  `bump::seen_hit`, `docs/football.md` "Body checks"): a person and a bot who
  didn't touch on the server hit if the person's body now touched the bot
  where their game drew it: its state at the claimed snapshot, from the same
  history and under the same clamp, coasted on by the claimed `ahead` to no
  later than now and no earlier than 100 ms before it (`MAX_BEHIND_TICKS`;
  `MAX_AHEAD` bounds a valid command) as the client coasts it
  (`football::coast`); judged by the speeds drawn, once per pair in 0.3 s.
  Hits on people are judged where they are: Football draws them ahead, about
  where they really are. Test
  `quic::tests::a_bot_is_tackled_where_the_chasers_game_drew_it_over_a_100_ms_round_trip`
  (real QUIC, 50 ms added each way: a bot cutting sideways at 30 m/s, the
  chaser put on the line it would have run straight on; tackled in the lap
  whose commands claim their drawing, never in the lap without).
- **Tests:** `sim::lagcomp::tests` (history bounds and eviction, clamping,
  interpolation and respawn fallback, a rewound shot hits where the same aim
  without a view misses, bots claim nothing, malformed views refused),
  `online::tests::the_view_sent_with_a_shot_is_the_time_remote_players_are_drawn_at`,
  the protocol wire test, and
  `quic::tests::a_laser_aimed_where_the_target_is_drawn_lands_over_a_100_ms_round_trip`
  (real QUIC, 50 ms added each way: the server rewound 13 ticks and the shot
  landed; the same aim without a view missed).


## Team switching and spectators (`teams1`, 2026-09-27, source only)

Wire change: `game_protocol()` gains `teams1`, so clients and servers must
update together. Rules live in `crates/core/src/teams.rs` (`Match::switch_team`,
`spectate`, `unspectate`, `join_spectator`); the server owns every decision.

- **Switching teams** (J by default, rebindable; or **Switch team** in the
  match menu; `ClientMsg::SwitchTeam`). Allowed when team sizes stay at most
  one apart afterwards, the same rule auto-balance keeps. On a bot server a
  team's size counts its bots (each side is topped up to `PEAKRUNNER_BOTS / 2`),
  so a person may always take a bot's place on the other side and a bot
  crosses back. Refused in free-for-all, and in Football while a ball is in
  play (allowed in warmup, countdown, intermission and between a score and
  the next ball). A carried flag or ball drops; the player respawns on the new
  team with no death, frag or score counted; the feed says "X switched to
  Glacier". Auto-balance uses the same rules, so it never undoes a legal
  switch.
- **Bot servers keep friends together.** Auto-balance there evens team
  sizes (people plus bots), not people: two people who play on one side
  (2 people + 2 bots against 4 bots) stay together and are never balanced
  apart. People may be uneven among themselves. Servers without bots balance
  people as before (at most one apart).
- **Cooldown:** 10 s (`SWITCH_COOLDOWN`) after a switch or after becoming a
  spectator, before switching or playing again. It is kept per player and per
  address group (IPv6 by /64, the vote groups), so reconnecting doesn't reset
  it and people sharing one address share it. LAN (TCP) players count alone.
- **Spectators** join with `Hello { spectate: true }` (**Join as a spectator**
  in Find match, or `PEAKRUNNER_SPECTATE=1` with `PEAKRUNNER_JOIN`) or become
  one with **Spectate** in the match menu (`ClientMsg::Spectate { on: true }`).
  They take no player slot: the snapshot lists them in `spectators`, apart
  from the 28 slots. `PEAKRUNNER_MAX_SPECTATORS` (0–8, default 8; also read
  by the LAN server; `GameHost::with_spectators`) caps them. They count
  against the per-address session limit (`PEAKRUNNER_SESSIONS_PER_IP`) like
  players; the QUIC gateway allows 28 + 8 sessions. Spectators receive the
  no-team snapshot view (no team chat, filtered in `Match::snapshot_for`, and
  nothing players don't also get), may use public chat (rate-limited as
  players are; Y opens public chat for them), can't vote and don't count as
  voters. The team key (or **Join a team**, `Spectate { on: false }`) takes a
  free slot on the side `join` picks.
- **Full matches:** joining a full match as a player seats you as a spectator
  while a spectator place is free, with the notice "Match is full: you're
  spectating, and you'll join a team when a slot opens". Only when the
  spectator places are full too is the join refused ("match is full (no
  spectator places free either)"). Such spectators, and any spectator who
  presses Join while the match is full, are *queued* (`Spectator.queued` in
  snapshots; the HUD says they'll join when a slot opens). Each tick a freed
  slot goes to the longest-waiting queued spectator, on the team the server
  picks, with the notice "A slot opened: you joined Ember". Spectators who
  chose to watch aren't queued and never join by themselves; pressing
  Spectate while waiting leaves the queue.
  Spectators still send neutral inputs to keep the session alive; the server
  ignores them. Everyone else leaving resets the match as before, but
  spectators stay connected, and they stay through map changes.
- **Refusals** arrive as `ServerMsg::Notice { message }` (shown in amber for
  five seconds) and never end the session. A voluntary switch shows
  "SWITCHED TEAMS — NOW PLAYING FOR …" instead of the auto-balance message.
- **Spectator camera** (`src/spectate.rs`): free camera (WASD along the view,
  Space up, Shift down, Ctrl faster; kept out of walls and over the terrain),
  or follow a player: click for the next player, right-click for the
  previous; R swaps over-the-shoulder and orbit (mouse turns around them);
  Space returns to the free camera there. The HUD shows who is watched and
  these keys; Tab lists spectators under the teams. QA:
  `QA_LOCAL=1 QA_BOTS=8 PEAKRUNNER_SPECTATE=1 QA_SPECTATE_FOLLOW=shoulder|orbit`.
- Tests: `teams::tests` (balance, bot swap, friends staying together on a
  bot server, cooldown through a reconnect, flag drop, nothing counted,
  Football, spectators' slots, votes and chat, queue order),
  `a_spectator_watches_chats_and_joins_and_a_player_switches_teams` and
  `a_full_match_seats_a_waiting_spectator_who_joins_when_a_slot_frees` (QUIC),
  `lan_spectators_are_capped_switch_teams_and_join_through_the_client_session`
  and `eight_real_slots_and_password_gate` (TCP), `spectate::tests`,
  `server_notices_show_once_for_a_few_seconds`, and the Online and protocol
  wire tests.


## Delta snapshots (`delta1`, 2026-09-27, source only)

A wire and compatibility change (marker `delta1`): clients and servers must
be released together. LAN/loopback TCP and the legacy WebSocket bridge still
carry full JSON snapshots; the browser build does not use `peakrunner-net`.

**Quantization** (`crates/protocol/src/delta.rs`). Players and projectiles
are sent as fixed-point integers: positions 1/1024 m (about 1 mm; exact in
f32 within 16 km of the origin), velocities 1/64 m/s, yaw and pitch
2π/65536 (0.0055°), health and energy 1/64, player timers (respawn,
cooldown, throw clock, stun, coyote time) 1/256 s; projectile life 1/256 s,
spin 1/16 rad. Explosions, smoke and loot are sent by the tick they appeared
(ages to 1/60 s; smoke positions 1/64 m), so the same one is identical in
every snapshot. Everything else (flags, ball, equipment, deployables, feed,
clocks, votes) is exact. Other players are drawn 100 ms behind, blended (in
Football ahead, coasted and slid), so millimetres never show. The recipient's own player, which prediction
replays inputs from, is appended exactly per peer (`own_state`, the
difference in float bits: usually a few bytes), so the server's simulation
is untouched and prediction starts from the same bits as before. Without it
the local player would be off by up to 0.8 mm after replay
(`local_prediction_corrections_are_unchanged_by_quantization`).

**Deltas.** Each input datagram carries the newest snapshot tick the client
decoded. A relay keeps the last 32 snapshots it sent (1.6 s) and sends a
delta against the acknowledged one, or a full snapshot on join, when that
baseline is gone or when the delta would not be smaller. The client keeps
40 baselines; a delta whose baseline it lacks is skipped, not fatal. Players
are coded per field group against the baseline slot (positions against where
the baseline velocity carried them); projectiles against the same shooter's
projectile flown on, straight or falling; new smoke from the grenade that
trailed it; other items are copied, byte-patched or sent literally. A lost
snapshot costs only itself: the next delta is against whatever the client
acknowledged.

**Sharing and bounds** (`crates/server/src/link.rs`). The game thread
prepares each view (Ember, Glacier, no team; team chat is still filtered in
`Match::snapshot_for`) once: quantized form, full encoding, and each
player's exact own state. Spectators share the no-team view and get
deltas too, without an own-state trailer until they take a slot (same net
ID, so it starts with the next frame). A map change or restart needs no
special case: a delta against an old-map baseline is still exact, and one
no smaller than the full snapshot is sent full. Lag compensation rewinds the
server's exact history, never quantized values. Relays of a view share the frame, and a frame
encodes each delta once per baseline for every relay using it (up to 48).
Memory per peer is bounded: 32 shared frames per relay, 40 baselines per
client. The full encoding must fit 12.5 KB; `packets::prepare_fitting`
trims smoke, feed, explosions, loot and then the oldest projectiles first,
never players. Decoding checks every count, reference and value range;
corrupt data is an error, never a panic (`corrupt_and_hostile_deltas_are_rejected_without_panics`),
and `online::sanitize` still checks what decodes (it now keeps turret
projectiles, whose owner is the `MAX_PLAYERS` sentinel; they were dropped
before, so online clients never drew turret bolts).

**Measured** (localhost, Apple Silicon, release; 28 players all moving,
jetting and firing every weapon):

| | before (LZ4 full) | after (delta1) |
| --- | --- | --- |
| bytes per snapshot, size probe | 4.6 KB mean, 9.0 KB max | 0.59 KB mean vs previous, 0.77 KB vs two back; 2.3 KB max; full 3.3 KB mean |
| datagrams per snapshot, 28 QUIC clients | 6.3 | 1.02 |
| payload per client | 990 kbit/s | 111 kbit/s |
| snapshots delivered at 1% / 2% / 5% loss | 94% / 88% / 71% | 99% / 98% / 95% |
| server CPU (game thread + 2 relay workers) | ~25% of a core | ~21% |
| encode cost | | 47 µs per view (prepare + full), 11 µs per delta; client decode 12 µs |

Loss was simulated independently per datagram in both directions (inputs,
and with them acknowledgements, too). Local prediction corrections in the
same runs were unchanged within noise. Reproduce the probe with
`cargo test --release -p peakrunner-protocol --lib snapshot_size_probe -- --ignored --nocapture`.
Not yet measured: real WAN paths and path-MTU behaviour beyond loopback.
