# Match communications

T opens public chat; Y opens team chat. Enter or Send submits the open channel.
Enter, Space and Shift never open desktop chat; the closed HUD has no focusable
chat button. The opening T/Y text event is consumed instead of entering the draft.
Escape cancels without pausing. Typing suppresses movement, look, firing,
equipment use and weapon switching while simulation and networking continue.
Local practice messages stay local; bots do not pretend to answer.

The server accepts only message text, derives the sender from the connection,
and generates frag records itself. Frags capture killer, victim and weapon at
death, so disconnects or slot reuse cannot change old records. Turret and fall
deaths are identified explicitly. A self-frag has the same killer and victim.

Messages are limited to 160 characters / 240 UTF-8 bytes, with control and
bidi-direction override characters rejected. One accepted message per player
per second; transport frame budgets additionally bound flooding. Invalid or
over-rate messages are ignored. The UI also paces normal sends.

The latest 16 chat/frag entries travel in authoritative snapshots. Clients
replace rather than append history, avoiding duplicates and recovering recent
events after packet loss. This is recent match history, not durable chat:
joining players see it, and it is erased when everyone leaves. Rounds retain it.
The collapsed panel shows the latest four entries; opening chat reveals the
scrollable history. Messages are plain text, never executable markup.

QUIC carries chat on its reliable encrypted control stream, separately from
unreliable movement datagrams. TCP/WebSocket paths use the same message contract.
Team messages derive the team from the connection's player slot, not client data.
Snapshots are filtered per recipient before serialization, including on QUIC.
Public and team chat share the same rate limit. No private messages, mute/report
tools or persistent chat logs yet.
The `chat2:names1` protocol markers require matching client/server builds; this source
change does not deploy public servers or alter directory discovery.
