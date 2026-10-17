# PeakRunner server

Host your own PeakRunner match. No domain name, certificate or settings
file is needed. To host from the game instead, use Host game on its main
menu: the game includes this server.

## Start it

- **Windows:** double-click `peakrunner-server.exe`.
- **macOS / Linux:** open a terminal in this folder and run `./peakrunner-server`.

The first time, it asks a few questions. Press Enter to take the answer in
[brackets]:

- server name, game mode and maps
- how many players, whether bots fill empty places, an optional password
- the UDP port (7777)
- whether to list the server publicly, and whether to ask your router to
  open the port (UPnP)

Your answers are saved to `server.json`, and later starts skip the
questions. Run `peakrunner-server --setup` to change them. The file and
the server's key live in:

- macOS: `~/Library/Application Support/PeakRunner/server/`
- Windows: `%APPDATA%\PeakRunner\server\`
- Linux: `~/.config/PeakRunner/server/`

Use `--config-dir DIR` to keep them somewhere else (one folder per server if
you run several). `server.json` holds the match password (if you set one) in
plain text, readable only by you on macOS and Linux; setting
`PEAKRUNNER_MATCH_PASSWORD` when starting the server overrides it.

## How players join

- **Public list:** if you announce, the server appears under **Find match**
  in the game once the directory has reached it. The server tells you whether
  that worked.
- **Invite link:** the server prints a link like
  `peakrunner://join/203.0.113.5:7777#sha256=...`. Players paste it into
  **Find match → Direct server address or invite link → Join directly**.
  Friends on your own network use the local link it prints. An unlisted
  server prints only that link unless you start it with `--publicip`, which
  asks the server list once for your public address to print the internet
  link too.

Players need a game released together with this server to join it (the
version shows in Find match and on the website's server list).

## Your own maps and mods

The server plays the maps in its `packs/` folder. To run more, put them in the
`content/` folder beside `server.json` (created the first time you save your
answers):

- `content/maps/<key>/`: a map folder (`map.json`, its files and `info.json`).
  Name it in the rotation: `{"map": "<key>", "mode": "ctf"}`.
- `content/mods/<id>/`: a mod folder (`mod.json`, an optional `defs.json`
  with the numbers it changes, `modes/<key>.json` for new modes, and
  `rules/<key>.rhai` for modes with their own rules, which only your server
  runs).

The `examples/mods/` folder in this package has four to start from: copy
one into `content/mods/` (`spree` shows a mode with its own rules script).
The full guide is at https://peakrunner.net/docs/mods/.

Then run `peakrunner-server --setup`: it lists the mods it found, asks which
to run, and offers their modes and your maps with the others. (Or edit
`server.json` by hand: `"mods": ["<id>"]`, and their modes in the rotation.)

Run `peakrunner-server --check` to check that folder the way the server reads
it: every problem is reported with its reason, and nothing broken is played.
The server list shows players your mods and how much joining downloads.

`peakrunner-server --healthcheck` (with the same `--config-dir`, if any)
exits 0 while your server's match is running: handy for a service manager or
a cron job. It reads a status page the server keeps on this computer only.
Players who lack a map or mod are asked when they join and download it from
your server; they never install anything by hand. A mod is data only, never
code.

## If players can't reach you

The server says so when the directory can't reach it. Forward **UDP** port
7777 (or the port you chose) on your router to this computer, and allow
`peakrunner-server` through the computer's firewall. UPnP does this
automatically on routers that allow it. Some internet connections (carrier
grade NAT, some mobile and apartment connections) can't accept incoming
players at all; the server warns when it sees one.

## Security

The server makes its own key the first time it starts. Players' games check
that key on every connection: the fingerprint comes from the public list
(the directory records it only after the server proved it holds the key at
your address) or from your invite link. Keep `server-key.pem` private; if you
delete it, a new key is made and old invite links stop working.

The operator of any server can read the game traffic on it. Use a match
password you don't use anywhere else.

## License and warranty

PeakRunner is free software. Its code is under the GNU General Public
License version 3 (LICENSE.txt), its art and maps under Creative Commons
Attribution-ShareAlike 4.0 (CC-BY-SA-4.0.txt), and the PeakRunner name and
logo are reserved; COPYRIGHT.txt says which is which. The server is a
playtest, provided "as is", without warranty of any kind; you run it at
your own risk, and you are responsible for the server you host and the
network it runs on. The third-party components it includes and their
licenses are listed in THIRD-PARTY-NOTICES.txt.
