# Saved Find a Rift settings

The native client saves its player name, directory address and direct server
address in `client.json`, separate from the installation and updater generations.
Find match's favorites are saved there too, as `favorites`: each starred server's
listing ID, name and join address (an invite link keeps its public fingerprint;
addresses with credentials or query strings are refused), at most 32, checked
again on load. `auto_download_mb` (default 0: always ask) is read back on load,
capped at 10,000. `player` is the game's anonymous ID: a random number (32 hex
digits) made the first time it joins a server (`Store::player_id`), kept
through renames and reloads (a damaged one is replaced). When it joins, the
game sends a code mixed from it and the server's host name
(`peakrunner_net::server_player`), so the official servers, which share
play.peakrunner.net, see one player and servers at different addresses can't
match players up; the official servers count those codes (`docs/multiplayer.md`
"Player counts").

| Platform | Default path |
| --- | --- |
| macOS | `~/Library/Application Support/PeakRunner/client.json` |
| Windows | `%APPDATA%\PeakRunner\client.json` |
| Linux | `$XDG_CONFIG_HOME/PeakRunner/client.json`, or `~/.config/PeakRunner/client.json` |

Linux ignores a relative XDG_CONFIG_HOME. If APPDATA is unavailable on Windows,
the fallback is the player's `AppData/Roaming/PeakRunner` directory. A missing
configuration directory disables persistence without preventing play.

Edits save after 600 ms without typing, on joining/refreshing/leaving the browser,
and on normal shutdown. The last valid name/address is retained when a field is
blank or invalid. Hover the automatic-save hint to see the exact file path.
Malformed/oversized files fall back to defaults with a visible warning and are
not rewritten just by loading them. Save failures remain nonfatal and visible.

An anti-aliasing checkbox (4× MSAA, on by default) is saved as `antialiasing`.
It applies immediately; GPUs without 4× support render at 1× regardless. Files
written before this setting existed load with it on.

A "Glow (bloom)" row (Off / Low / High, default Low) is saved as `bloom` with
the values `off`, `low` or `high`. It applies immediately. An unknown value, or
a file written before the setting existed, reads as Low. Low costs about
0.2 ms of GPU time in the heavy-fight probe; Off removes it entirely. QA
captures, which run on in-memory settings, can force it with
`QA_BLOOM=off|low|high`.

Key bindings save as `keys`, an action-to-key map (`src/keybinds.rs`; the
Controls screen, `docs/loadouts.md`). The full map is `"map"` (default M,
2026-10-06); the mine moved from M to N, and a file that saved M for the
mine keeps it, which leaves the map on the mine's old key until rebound.

Only schema, name, directory, direct address, `antialiasing`, `bot_difficulty`
and `bloom` are serialized. Match passwords,
sessions and chat are never saved. Addresses with URL credentials, query strings,
fragments or control characters are not persisted. Native transport and server
validation remain authoritative. Files are replaced using a same-directory
temporary file and rename, with 0600 permissions on Unix.

For isolated testing, set an absolute `PEAKRUNNER_CONFIG_DIR`. Never point tests
at a real user's profile. Automated `PEAKRUNNER_JOIN` sessions intentionally use
memory-only preferences, avoiding accidental replacement with a QA server/name.

This is a new, unshipped feature after release `.20260919.4`. It needs its own
tested main merge and subsequent client release; the already published binaries
do not gain persistence from changes to this source tree.

## Graphics screen (2026-09-26)

**Graphics…** on the main menu (and **Graphics** in the pause menu) opens
the display settings. Each applies at once and is saved in the `graphics`
preference group:

- **Display mode:** windowed or fullscreen.
- **Render scale:** 100, 85, 70 or 50%. The 3D scene is drawn at that share
  of the window's pixels and stretched to fill it; the HUD stays sharp.
- **Vsync:** on or off, applied live through
  `Frame::set_wgpu_surface_config`.
- **Anti-aliasing and glow (bloom):** moved here from Find match.
- **Show FPS:** also toggled with F3. It shows the frame rate, frame-time
  range, detected refresh interval, and how many frames ran 0, 1 or 2+
  simulation steps.

Invalid saved values read as the defaults.

Frame times are paced to whole display refreshes (`src/frame_pacer.rs`),
and the local camera is drawn between fixed steps, for smooth motion on
every OS.

