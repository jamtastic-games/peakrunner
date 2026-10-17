# PeakRunner native playtest

Every map ships with the game in its packs folder: Old Holler, Tower
Complex, Cairnhold, Frostline, Dustreach, Ozarktic Blast, Reefbreak and
Ashveil (CTF, Capture & Hold, Team Deathmatch, Rabbit and Team Rabbit),
plus Longfield and Highgoal (football) and Tenfold Tower (the duel
servers' lobby). No separate asset download is needed.

Version .20261016.3 plays with .20261015.1 to .20261016.3 servers and
players (the launcher updates you). New in .20261016.3:
- Ashveil is rebuilt as our own map: an island caldera in a lava sea,
  with a new spired temple halfway between the flags, the cathedral under
  it rebuilt (a gallery all round, a nave below and five ways in: two
  lava tubes from collapse pits, two vents from the temple floor and the
  well), a mesa of basalt columns holding team 0's flag and stations,
  cinder cones, a lava fissure, lava ponds with hot air rising, rock
  arches, and the bones of three giant beasts half sunk in the ash. The
  skull tower, the lava valleys, the spires' lightning and the hall that
  burns its holders are as they were. An older version joining a server
  that plays Ashveil is asked to download the new map first.

New in .20261016.2:
- Football bots that catch a pass in the air keep the catch's push until
  they land, as a person does: before, a bot jetted off toward where it
  wanted to go at once, which took the push away whenever the pass came
  across or against that way.

New in .20261016.1:
- Favorites (F1-F4) and Use work while an inventory station's screen is
  open; holding the key no longer equips again and again, and F3 there
  no longer turns the frame-rate overlay on or off as well.
- Duels: someone watching a duel keeps their spectator's place in the
  lobby and their place in the queue. The team key (J) passes over full
  duels; a full duel leaves you where you are instead of putting you
  back at a spawn. Declining a challenge while watching names you.
- In a duel server's lobby no death counts anywhere: a fall off the
  plaza, a bump or Ctrl+K no longer goes in the feed or the scores.
- Getting out of a vehicle puts you beside it only where your whole body
  fits: never inside a wall, an enemy force field, another vehicle or a
  low deck. With no room anywhere you stay in, and a message says so,
  online too.
- Football: a bot you hit where your screen showed it counts as hit only
  for drawings between the present and a tenth of a second before it.
- Downloads: the game refuses a server that sends more than it listed,
  never trims maps the server you're joining needs from its download
  folder, and trims old mods too (at most 400 maps and 100 mods).

New in .20261015.1:
- Football: a fumble says FUMBLE on screen, without naming who came up
  with the ball, and sounds like a fumble, not a pass. INTERCEPTS shows
  only when the other side catches a pass in flight.
- Football draws the other players and the loose ball where they are
  now, not where they were a tenth of a second ago: everyone keeps going
  the way they were moving until the next update, and slides over if
  they turned. Tackles and catches line up with what you see, and a bot
  you hit where your screen showed it counts as hit.
- Catches reach a little (3%) farther round your body.
- Football bots carrying the ball see defenders a quarter second late
  and only in front of them, remembering those gone behind them for
  about 2.5 seconds: no more passing the instant before a tackle from
  behind they couldn't have seen.
- Tenfold Tower has no more spots to get stuck in: bodies no longer slip
  into the outer wall or sink into steps, the crystal cavern's rocks and
  crystals leave no crevices, and the narrow gaps by the ruins' walls and
  the cargo bay's barrels are gone. Sparring bots walk round molten pits
  instead of waiting at the edge.
- Players seen far away with empty hands, as in Football, no longer
  appear holding a weapon.
- The game has its own icon, PeakRunner's two slanted bars, in the dock,
  the taskbar and Finder, instead of a blank or borrowed one. The Windows
  game and server programs carry it too (the server's with a rack badge).

New in .20261014.7:
- Duels: a duel server's lobby lists everyone there. Challenge a person
  or a bot from the duel board (V), and each accepted challenge plays a
  one on one duel of its own, many at once: one life a round, first to
  3 rounds, light armor's default kit, from random spots on one of the
  server's maps. Watch duels from the board and switch between them with
  the team key. The new server is Springdale Duels.
- The lobby is Tenfold Tower: ten floors round a spiral stair (a cargo
  bay, a neon alley, ruins, a laboratory, a greenhouse, a foundry, a
  library, a crystal cavern, a throne hall and an open crown under a
  dome). On every floor your hands hold a blade and your jets are off:
  click to swing (each click a different swing), hold to charge a power
  swing, click fast for combos, hold right click to guard. Swings and
  guarding cost stamina; a blow on your guard costs stamina before
  health. Deaths there count for nothing: you're back in 5 seconds at a
  random spot. Sparring bots fight on the lower floors.
- Host game offers Duel: your own duel server with its tower.
- The website's server list shows one line per server, sortable and
  filterable, with a server's map, score and players under a click.

New in .20261014.6:
- In Football, when someone passes the ball to you, you hear a flag
  flap as it leaves their hand, and the edge of your screen glows gold
  on the side the ball is coming from, by the way you face: the top for
  ahead, the bottom for behind, the sides for left and right. The glow
  brightens as the ball nears and goes once you catch it or it lands.

New in .20261014.5:
- Football is played empty-handed: nobody holds a ball thrower any more.
  The carrier tucks the ball under the right arm (in first person it rests
  on your forearm), and a pass reaches forward and up.
- In Football a dead player's body stays down for 10 seconds where it
  fell, and a carrier who runs into it can trip and fumble.
- Fixed: catching a ball just before it hit the turf could give you a
  smaller push than the catch should, or none.
- In Rabbit, arrows over enemies and the dots on the minimap and full map
  need the same range and line of sight as anywhere else: the rabbit no
  longer sees every hunter through the hills. Hunters see the rabbit on
  their maps only during a ping, as on screen.

New in .20261014.4:
- In Rabbit the rabbit wears bright gold armor that shows in any shade,
  and every hunter wears the same steel: nobody looks like they're on a
  team that doesn't exist. Free-for-all is the same, all steel. Shots and
  the minimap's dots name no side either.
- Fixed: after a first shot from the repeater or grenade launcher, the
  barrel kept flashing and throwing casings as if the trigger were held.
- Offline the play feed says "You pick up the ball", not "You picks up".

New in .20261014.3:
- Football shows no SCORE HERE or YOUR END ZONE labels, and no arrow to
  the end zone while you carry the ball: the painted end zones and goal
  posts mark them.
- Football servers play 6 a side.

New in .20261014.2:
- Nobody carries a name over their head any more: only the player under
  your crosshair is named, in blue for a teammate and red for an enemy.
  In Football that's the player your pass goes to.
- Armor wears its team's color all over, so you can tell the teams apart
  from behind and at a distance.
- Football draws no arrows over enemies.

New in .20261014.1 (plays only with .20261014.1 and later):
- Football bots play football: they get open, catch passes in the air
  (jumping and jetting to meet a high ball), pass only when a teammate
  will get there first, block for the carrier, rush and tackle, cover
  receivers and play deep. In Football they go by football names.
- Two football servers: Springdale Football for people, and Springdale
  Football Bots, filled with bots (each player who joins takes a bot's
  place).
- Players look new: legs whose feet stay planted on the ground as the
  body runs over them, hops drawn as running bounds, light and shadow on
  armor and weapons, rebuilt armors and weapons, and bodies standing on
  the ground at full size.
- In first person the weapons sit higher, nearer and more to the middle,
  so shots leave from new muzzles.
- In Singleplayer every body shared one animation, so legs ran about six
  times too fast and every bot wore the same face; each body now has its
  own. Bots on servers no longer run with their legs frozen, and offline
  bots move smoothly at any frame rate.
- Football shows no label over the ball or its carrier: the ball's light
  and the carrier's flag mark them. An arrow at the screen's edge still
  points to the ball when it's out of view.

New in .20261013.1:
- Rabbit plays by the classic rules:
  - Everyone without the flag hunts the rabbit together, and hunters
    can't hurt each other.
  - Grabbing the flag is a point, and every 15 seconds you hold it, all
    your turns added up, is another. Tagging the rabbit is a point, every
    hunter the rabbit tags is two, and a suicide costs nothing. Most
    points when time runs out wins.
  - Every 24 seconds the hunters see where the rabbit is for 6 seconds,
    unless someone has hit the rabbit lately. A dropped flag goes home
    after 45 seconds.
  - No mortars or mines, and the rabbit can repair themselves.
- Shield and jammer packs use energy faster than you recharge, so they
  slowly empty your tank even standing still. A shield takes the same
  energy for a hit whatever armor you wear.
- Bots count as players in the server lists, marked as bots. A server
  full of bots still has room: each player who joins takes a bot's place.

New in .20261011.2:
- Two new modes around one gold flag:
  - Rabbit, everyone for themselves: grab the flag for a point and hold
    on for another every 5 seconds while everyone hunts you. Kills score
    too, and a suicide costs a point. First to 30 wins. The rabbit can't
    throw the flag or repair themselves.
  - Team Rabbit: each team has 50 lives and every death costs one. The
    first death drops a flag; while your team holds it, each of your kills
    takes two lives. Throw it to a teammate with Ctrl+F.
  - Both are in Singleplayer, Host game and the Library. Springdale Mix
    and Boticus Maximus now play them, in a new random order each time
    through.
- Discs burst where they hit water, at half damage and twice the push,
  instead of hanging under the surface.
- Hosts can play their rotation in random order ("rotation_order":
  "random" in server.json).

New in .20261011.1:
- Football plays by the classic rules:
  - A pass's power comes from whoever is under your crosshair, on either
    team, or the nearest player straight ahead. It leaves with the classic
    lift: about 14 degrees above a level look, less as you look up or down.
    Looking straight up still throws it about 30 m ahead.
  - The ball barely bounces and skids to a stop instead of rolling on, and
    it no longer spins.
  - Nobody can catch their own throw or fumble for a second, and a catch
    pushes you the way the ball was thrown.
  - Bodies leave their ball-thrower on the field for a while: run the ball
    over one and you'll likely fumble.
  - You can look 88 degrees up and down.
  - At half time the teams switch ends, the ball goes to the side that
    didn't have it, and the second half gets its full 15 minutes.
- Running looks better: the heel tucks under the hips as the thigh drives
  forward, the planted foot meets the ground, and a jog keeps its knee bend.
- peakrunner.net shows how many people have played on the official servers,
  in all and today. For this the game sends an anonymous code when it joins
  a server; the privacy notice (peakrunner.net/legal/) says what is kept.

New in .20261010.2:
- Smoother play, above all on Ashveil: the game checks collisions several
  times faster, so busy servers no longer lag (a server full of bots on
  Ashveil had been running close to its limit). Bots also learn a map's
  routes before anyone plays it, instead of during its first round.

New in .20261010.1:
- Bodies fall: someone killed in the air falls with the speed they had and
  lands, and the pack they drop falls with them and lies where they land.
  It used to hang in the air, with the pack on the ground below.
- Going down, dead or tackled in Football, a body goes limp and lies
  however it lands, its joints bending the way joints do.
- Repairing yourself makes you glow green; the repair stream goes only to
  teammates, equipment and deployables.
- Running looks like running: longer strides, a kick behind, and both feet
  off the ground between steps.
- Third-person view: the repair tool no longer shows on screen as it does
  in first person, and your muzzle flash comes from your character's gun.
- Mods that add weapons, armor or other classes now download when you join
  a server that runs them.
- Intel Macs have their own download (macOS 12 or later).

New in .20261009.2:
- The jump-boost: press your jets while you're on the ground, standing,
  running or skiing, and they kick you up with half a jump, however fast
  you're going, so a jump-jet rises while you keep flying forward. It comes
  once per press and doesn't add to a jump on the same takeoff.
- Ashveil: the cathedral under the temple now burns whoever's team holds
  its post, while the other team is safe inside. You can't hold the post
  from inside; guard it from outside. Old tablets in the cathedral and the
  tunnel say as much, for anyone who stops to read them.
- Ashveil's spires strike anyone flying again, as before, but reach 130 m
  instead of 140.
- The railgun's slug flies twice as fast, 600 m/s: a quarter of a second
  to 150 m, so you lead a fast target far less.
- Bots land before a big jet climb instead of starting it in mid-fall, and
  flying bots no longer take off into a ceiling.

New in .20261009.1:
- Steering with the jets now costs lift: whatever pushes you along the
  movement keys comes out of the climb, so leaning in from a standstill
  sinks you. Once you're moving fast the push fades and the whole jet
  lifts again, but turning hard across your line of travel still costs
  lift. Bots fly their climbs for it.
- The repair tool stays in your hands while you hold the key. When your
  energy runs out it dims and waits 1.5 seconds while the tank refills,
  then starts again, instead of flickering in and out.

New in .20261008.1:
- A new map, Ashveil: a volcanic basin with a domed temple, a cathedral
  buried under it, lava valleys round its sides and a tower of skulls in a
  lava moat. Hot air rises over its lava pools, and all of its lava burns.
- Touch the post at the bottom of Ashveil's cathedral to take it for your
  team (it's also a Capture & Hold point): while you hold it, the temple's
  spires glow in your color and strike enemies flying within 140 m with
  lightning that drains 30 energy and 15 health a second.
- Rebuilt bases with sharp new models: Reefbreak's ships (decks inside, a
  generator in the keel and a low-gravity column), Old Holler's flag
  towers with rocket turrets, and new detail on Dustreach, Frostline and
  Cairnhold.
- Cairnhold, Dustreach and Frostline are about twice as long, with
  distinct lanes between the bases.
- Closed-visor armor with gloves; hands now hold every weapon by its grips;
  a new football thrower.
- A shaded minimap that turns with you, and M opens the full map (throwing
  a mine moved to N). The kill feed is on the left.
- An over-the-shoulder third-person camera.
- Mods can add ground vehicles (a flying motorcycle and a tank in the
  stock library), with exposed seats and turrets that aim.

New in .20261002.1:
- A new look: textured armor in three sizes and new models for every
  weapon, many projectiles and the tower turrets.
- Three armors: light carries 3 weapons and alone has the laser rifle;
  medium carries 4 and every pack; heavy carries 5 and alone has the mortar
  and railgun. Light armor can't carry turrets, walls, force fields or
  stations.
- Two new weapons for every armor: the drain gun (hold it on an enemy to
  empty their energy) and the target marker (shows an enemy to your whole
  team for 12 seconds, even through walls).
- New packs: energy, shield, repair, sensor jammer and ammo packs to wear,
  and motion sensors, pulse sensors and jammer pylons to place.
- The railgun fires a glowing slug you lead: one hit downs light armor, and
  every shot empties your energy.
- Sights zoom 4x, and 8x with the mouse wheel; the more you zoom, the
  farther away you can tell who people are. The mortar's sight shows the
  angle you're aiming at.
- The chaingun is now the Repeater.
- The towers in the middle of Frostline, Cairnhold and Tower Complex can be
  captured in CTF: their turret (heat-seeking rockets at enemies using their
  jets, or plasma) fights for whoever holds them.
- Mods can now define armors, weapons, projectiles, packs, effects,
  deployables, vehicles and more as data (mod API 5); Sparkfall, a full
  example mod, comes with the server.

New in .20260930.5:
- Weapon slots: light armor carries 3 weapons and heavy armor 4, chosen at
  an inventory station from the disc launcher, chaingun, grenade launcher
  (mortar in heavy armor), laser rifle (railgun), plasma gun and blaster.
  Number keys pick slot 1, 2, 3 in the order you chose them; the HUD shows
  your slots.
- New weapons: the plasma gun fires slow glowing bolts that burst where they
  land; the blaster fires quick bolts on your energy that never run out and
  glance off the ground.
- Inventory stations open only when you step in from the front.
- Favorites: save up to four setups at a station and equip one with F1-F4.
- Field inventory: a pack that puts a small station anywhere (weapons and
  packs, not armor).
- Ctrl+F tosses the flag you carry, to a teammate or over a wall
  (rebindable).
- A new main menu: Singleplayer, Multiplayer and the Library, which explains
  the game in simple words with pictures.

New in .20260930.4:
- What you take at an inventory station is your setup: every respawn gives
  back your armor, rifle and pack, restocked, until you take something
  else. It lasts through rounds and map changes.

New in .20260930.3:
- Pick the time of day (dawn, day, dusk, night or the map's own) and the
  weather (clear, overcast, rain, storm, snow or fog), or let either be
  random each round: under Sky on the main menu, and on the Host screen
  when you host. Only the look changes.
- Springdale Central plays Capture the Flag only, under a random time of
  day and weather each round. New server: Springdale Mix plays every map
  once, taking turns through CTF, Capture & Hold and Team Deathmatch.
- Mods can choose a mode's weapons: any of the six, or none. Weapon keys,
  the inventory and bots follow; with none your hands are empty, and
  grenades and mines stay if the mode has them.
- Join prompts show small downloads in KB.

New in .20260930.2:
- Fixed: the end of a round online no longer takes you out of the match.
  The result shows over the game with nothing to click, your mouse stays
  with the game, and the next round starts by itself; Esc opens the menu as
  always. (A shot fired as the round ended could click the old end screen's
  Menu button and leave.)
- Fixed: after the game was in the background, a server could disconnect
  you for sending a burst of inputs.
- Find match shows each server's live player count, and small downloads in
  KB rather than "+0.0 MB".

New in .20260930.1:
- Find match is a server table: ping, mode, map, players, version and mods
  for every server, with a status light (green ready, amber slow, red can't
  join or not answering) and a lock for a password. Click a column to sort,
  filter by name, map, mode or mod, and hide empty or full servers.
- Favorites: click the star to keep a server at hand. "Favorites only"
  shows just those, and a favorite stays in the list even when it isn't
  listed.
- Click a server to see its address and what joining downloads; double-click
  or Join game to join. A password box appears only for servers that need
  one; direct addresses and invite links are under "Join by address…".
- The "download without asking" size limit is now kept between sessions.

New in .20260929.3 (servers running a mod with its own rules need it):
- Find match shows each server's mods and how much joining would download,
  before you click.
- Mods can bring their own rules: a short script on the server decides what
  scores, what wins and what's announced (for example Spree: every third
  frag in a row scores double). Scripts run only on servers; your game only
  ever downloads data.
- Server hosts: the setup (--setup) lists the mods in your content folder,
  asks which to run and offers their modes; --check also checks rules
  scripts; --healthcheck now works outside Docker; the server download has
  example mods in examples/mods. How-to: peakrunner.net/docs/mods/.

Version .20260929.2 required a matching server; older clients and servers
were incompatible. New in .20260929.2:
- Servers can run maps and mods you don't have. When you join one, the game
  names what's missing and its size, and downloads it only if you choose
  Download and join. Every file is checked, and downloads are kept for next
  time (up to 2 GB, oldest dropped first). Declining downloads nothing.
- Mods are data only: they change numbers (gravity, weapons, armor,
  equipment) and add variants of the modes, such as a five-capture CTF. A
  server never sends code to your game, and when you leave its match your
  game is its own again.
- Your own maps and mods go in the content folder the Host screen shows;
  the game (offline too) and the server you host both read it.
- The maps are now files beside the game instead of being built into it;
  how the game plays hasn't changed.
- Server hosts: put maps and mods in the content folder beside server.json,
  list mods under "mods", and check the folder with peakrunner-server
  --check (the server README explains).

New in .20260929.1:
- Host game on the main menu: run your own server from the game and play in
  it. Pick a name, a mode and maps in play order, players, bots, an optional
  password and the port. "Announce publicly" (checked) lists it under Find
  match; unchecked, share the invite link the Host screen shows.
- Alone in your own match? Press Esc, then Start match now: it plays with
  the other team empty, and others can join any time. Only the host can
  start it.
- Quitting the game stops your server. Players on the internet need UDP port
  forwarding to your computer; the server tries UPnP and says whether it
  worked.
- The download now includes the PeakRunner Server app for hosting, so it's
  larger.

New in .20260928.2:
- No more flickering or "clipping" surfaces: distant trim, banners and wall
  borders draw cleanly at any range, and overlapping surfaces on Dustreach,
  Frostline, Ozarktic Blast, Cairnhold and Tower Complex are fixed. Old
  Holler's bunker ramps no longer sink under the grass.
- Flags are grabbed by touching the pole anywhere up to its top, and a fast
  skier can't pass through one without taking it.
- The football no longer goes through Highgoal's walls when a pass hits
  them at an angle, and grenades bounce off deployed walls instead of
  sticking to them.
- Grenades and mines keep all of your momentum: thrown while skiing, they
  fly on ahead of you.
- Football sounds are sparse: a whistle for each new ball, a horn at half
  time, a chime for your catch, the capture sting for a touchdown, and a
  warning while you're out of bounds. No crowd, no cheer and no background
  hum. A body check thumps once per real hit instead of rattling.
- Server hosts: map keys now follow the map names (old-holler,
  tower-complex, cairnhold, frostline, dustreach). A saved server.json that
  names an older key skips it; run setup again to choose maps.
- Every download includes LICENSE.txt and THIRD-PARTY-NOTICES.txt; terms,
  privacy and licenses are at peakrunner.net/legal.

New in .20260928.1:
- Everyone is a person now: armor over an undersuit and an open-faced
  helmet, with a face (skin, eyes and brows vary from player to player).
- Football has its own movement: no steering in the air; a jet with a
  direction key held pushes you along and barely lifts; hold jump to
  hop-ski. Carriers run slower, a knocked-loose ball flies on with the hit
  and is up for grabs, and Ctrl+K takes 15 seconds.
- The ball carrier wears their team's flag and lights the ground round
  them; a loose ball glows too. Everyone holds a football thrower.
- The grenade launcher fires with a small orange-red blast, the chaingun a
  tiny one per round; the disc launcher stays blue.
- Fixed: the football reset when it rolled against Highgoal's walls.

New in .20260927.2:
- The mortar is scary again: its blast reaches 20 m. It kills light
  armor within about 8 m and still hurts at 15 m.
- The grenade launcher's blast reaches 15 m (it used to fade to
  almost nothing past a metre).
- The laser rifle does 60 damage (was 50).

New in .20260927.1:
- Smoother online play: the server no longer skips your inputs when the
  connection is jittery (much less rubber-banding), and other players move
  smoothly instead of stepping.
- Much less bandwidth: updates are about a ninth of the size and usually
  fit in one packet, so a lost packet rarely costs you an update.
- A 10-second countdown before every round, on every map and mode.
- Switch teams (J) and spectate (pause menu): free-fly or follow a player.
  A full server seats you as a spectator and puts you in when a slot opens.
  On bot servers, friends can play on the same side.
- The laser rifle hits where you saw your target (up to 250 ms of lag).
- Host your own server: run peakrunner-server, answer a few questions and
  share the invite link. Community servers appear in the server list.
- Fixed: turret shots were invisible online.

New in .20260926.5:
- Fixed: letting go of the movement keys on a slope (Ozarktic Blast's
  mesa especially) slid you up to 18 m downhill, which felt like ice when
  moving forward. You now stop in about 2-4 m in every direction. Walking,
  skiing and jetting are otherwise unchanged.

New in .20260926.4 (plays with .3):
- Fixed: the repair tool (Q) failed on generators from many angles; it now
  works wherever you can see one, and uses 5 energy a second instead of 12.
- Fixed: Capture & Hold and Team Deathmatch played the capture music over
  and over.
- Chat and the kill feed are separate: frags and announcements show top right.
- Distant sounds are quieter and carry less far.
- Lag no longer ends your session: a backlog of unacknowledged inputs is
  dropped instead of disconnecting you.
- Football: the ball carrier wears an amber flag (no more gold ring), and a
  new ball drops in from the sky.
- Landing only makes a sound when it hurts. Station prompts name the right
  keys (repair is Q, the repair tool).
- Grenades and mines throw on each press at full strength.

New in .20260926.3:
- Map votes: press V to call a vote for another map or mode in the server's
  rotation, or to vote on one. A failed vote locks voting for 5 minutes.
- Fixed: after a server map change nobody could move, jet or fire until
  they rejoined. Fixed: inventory station purchases online.
- Boticus Maximus, a third server full of bots that make way for players.
- Matches of up to 14 a side (the main server now holds 28).
- Anyone's fire wears down deployables and sets off mines, so you can clear
  your own team's. Your own grenades and mortars hurt you in full.
- G and M throw on each press at full strength; mash to empty your belt.
- Two-column scoreboard, server locations in the server list, the frame-rate
  overlay moved top left, fewer weather draw calls, map seams fixed on six
  maps, and empty servers keep their current map.

New in .20260926.2:
- Reefbreak's sea is a surface you ski across, never under, with spray,
  foam and ripples; water on other maps still slows and floats you.
- A Graphics screen (main and pause menus): display mode, render scale,
  vsync, anti-aliasing, glow and an FPS counter (also F3).
- Smoother motion, especially fullscreen: the camera is drawn between
  simulation steps and frame times are paced to the display.
- The map menu wraps and scrolls to fit the window.

New in .20260926.1:
- Loadouts: light and heavy armor, limited ammo, the mortar, the repair tool,
  inventory stations with a buy screen, deployable turrets, walls, force
  fields and ammo stations, hand grenades (G) and mines (M), a laser rifle
  (light) and railgun (heavy) with zoom (E), third person (R) and a Controls
  screen for rebinding keys. The dead drop a pack of what they carried.
- Football on Longfield and Highgoal, on its own server (UDP 7778).
- Team Deathmatch on every non-football map, each round with a random time of
  day, weather and a twist.
- Two new maps: Ozarktic Blast (a ridge, a dock valley, hovering ships) and
  Reefbreak (an atoll with floating freighter bases).
Map traversal and balance still need human playtesting. Approved CTF movement
constants are unchanged.

- macOS: Apple Silicon only. Extract the ZIP and open PeakRunner.app. The bundle
  is ad-hoc signed for integrity, not Developer ID signed or Apple notarized.
  If macOS blocks the unidentified developer, use System Settings > Privacy &
  Security > Open Anyway for this app. Do not disable Gatekeeper system-wide.
  A "damaged" warning is not expected; report it rather than bypassing it.
- Windows: x86-64 build. Extract the ZIP, then open PeakRunner.exe. Not code signed.
  Native Windows runtime verification is still pending; cross-compilation alone
  does not prove graphics, audio or input compatibility.
- Linux: x86-64, built against Debian 12 (glibc 2.36 or newer). Requires a desktop
  session and Vulkan-capable drivers, plus ALSA, udev and X11/Wayland libraries
  (including libxkbcommon-x11).
  Software Vulkan rendering is suitable for compatibility testing, not a promise
  of playable frame rates. Run the extracted peakrunner executable.

Select Find match to set your name and join a host. During a match, Escape opens
the match menu: edit Your name and choose Apply name. Current shows the server's
accepted name. Names allow 1–24 ASCII letters, numbers and spaces. Outer spaces
are trimmed; blank, punctuation, Unicode and control-character input is rejected.
The server permits one change every 10 seconds. Renaming does not reset scores
or identity. Display names are not verified accounts and need not be unique.

Standalone packages do not auto-update. The separate PeakRunner Launcher can
download verified updates without downloading unchanged map files.

Chat: T opens public chat; Y opens team-only chat. Enter sends; Escape cancels.
While chat is open, movement, weapons and other gameplay input are disabled.
The server validates both channels and filters team messages before transmission.

## License and warranty

PeakRunner is free software. Its code is under the GNU General Public
License version 3 (LICENSE.txt), its art and maps under Creative Commons
Attribution-ShareAlike 4.0 (CC-BY-SA-4.0.txt), and the PeakRunner name and
logo are reserved; COPYRIGHT.txt says which is which. It is a playtest,
provided "as is", without warranty of any kind; you use it at your own
risk. The third-party components it includes and their licenses are listed
in THIRD-PARTY-NOTICES.txt.
