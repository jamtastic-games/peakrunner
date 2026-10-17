# Ozarktic Blast

An original CTF map for PeakRunner: frosted pine highlands where a tall
central mountain splits the field, one team holds a giant hill and the other
a deep valley. Status: in development (source only, not embedded or deployed
until its previews were reviewed).

## Design goals

- Bases as spread-out compounds rather than one building: a flag on a big,
  fully exposed raised deck, a command building beside it, a rock outcrop
  with inventory rooms behind, and parts of the base underground.
- A tall spire standing in front of each base as a landmark.
- Long, smooth ski valleys and a cliff through midfield: a fast, contested
  line and a slower safe loop.
- Cappers who start high and drop onto the lanes.

## Concept

**Site plan (flag axis along world Z; Ember at low Z, Glacier at high Z):**

```
            z = 470   ~~ crest of the Giant Hill (Ember's downhill start) ~~
            z = 650   [Ember]  Bluff base on the hill's shoulder, deck jutting
                               over the downslope, spire 50 m in front
            z = 800           long downhill ski lanes off the hill
            z = 1024  /\/\  Tall central mountain, long across the field (X):
                      west end tapers into a wide low pass; east end breaks
                      off in a cliff over a narrower, faster pass
            z = 1250          the valley throat opens from the mountain's foot
            z = 1400  [Glacier] Hollow base in the valley floor, deck jutting
                               off the west valley wall, spire 50 m in front
            z = 1560  ~~ valley head and rims (Glacier's high ground) ~~
```

- **Asymmetric on purpose.** Ember's base sits high on a giant hill: attackers
  arrive uphill, defenders leave downhill. Glacier's base sits low in a
  valley: attackers ski down into it, and defenders hold the rims above.
- **The central mountain** blocks the straight line between the flags, so every
  route picks a side:
  - the west pass is wide and low, slower and safer;
  - the east pass runs under a cliff, faster and exposed from the summit shoulder.
- **Bases** share one plan built in two styles, so neither side has more
  rooms or entrances:
  - **Flag deck:** a raised slab about 6 m up, open to the sky on all sides,
    with a ramp and a bridge onto it. On the hill it juts over the downslope;
    in the valley it juts off the valley wall.
  - **Command hall:** two inventory stations and the spawns. It has open
    front and side doors, and a roof reached by a ramp and joined to the
    deck by a bridge.
  - **Underground:** the generator room is under the hall, with exactly two
    ways in: a ramp down from the hall, and a tunnel from the outbuilding.
  - **Outbuilding:**
    - hill base: a bunker door in a hillside retaining wall;
    - valley base: a crater shaft with a ramp spiralling down into the tunnel.
    - Either way, it holds the third inventory station and is a way back up
      to the surface.
  - **Spire:** a tall tower about 50 m in front of the base, with a turret on
    a ledge and the sensor on top.

**What we keep (as ideas):**
- exposed raised flag decks;
- compound bases with underground parts;
- the landmark spire;
- a cliff route versus a loop route;
- high starts.

**What's new:**
- the asymmetric hill and valley pair;
- a central mountain rather than a cliff line;
- two different outbuilding styles (bunker and crater shaft);
- our frosted-pine materials and sky.

**Fairness without mirroring:**
- The same rooms, equipment, spawn count and entrance counts on each side.
- A flag-to-flag distance of about 750 m, like Dustreach and Frostline.
- Flag heights: the hill flag is higher (measured below). The valley team
  gets the rims above its base, the faster pass beneath the cliff, and a
  downhill run off the mountain's skirt into the attack.
- Human playtesting decides the real balance.

**Decisions (recommended answers taken when the name was given):**
- **Name:** Ozarktic Blast.
- **Bases:** both on the ground; neither floats. The flag decks are raised
  slabs.
- **Slot:** a new sixth CTF map. It replaces nothing and keeps the
  compatibility slot count honest.
- **Landmark:** the central mountain forces a choice between routes.
- **Exception:** the pipeline's "bases exactly mirrored" rule is set aside
  for this map. Equal entrances, equipment and spawns, plus measured
  routes, stand in for mirroring.

## As built (v3)

- **Build:** every shape, height and position here is our own.
- **Scale:** the flags are 628 m apart, close to our other maps.

Layout (flag axis along world Z, the flags on the line x = 1024):
- **The Ridge** crosses the flag line at the centre: a spur off the west
  upland with a small flat top, 87 m over the flags, and a steep east face.
- **West:** high ground. The outposts stand about 70 m over the flags.
  Each team's cliff-side outpost holds two inventory stations and faces its
  base, which makes five inventories per team.
- **East:** a low valley with two dry docks, one per half. Each is a basin
  with two crane gantries on rails and stacked cargo containers for cover.
  Our own weird hovering ship floats over each basin; its hideout is entered
  through the belly hatches. The sniper's perch stands on the east rim,
  overlooking both docks and both bases.
- **Symmetry:** the large forms mirror front to back, so neither team has
  the better ground. The fine detail isn't mirrored.
- **Flags:** on low ground, level with each other, with the ground rising
  behind each base.
- **Capture points:** Ridge Top and Dock Yard, both on the centre line.
- **Terrain slopes:** median 16.2°, 90th percentile 30.4°, 10.6% over 30°.
- **Collision:** 1578 triangles for the bluff base and 1554 for the hollow
  one (each with its spire and outpost), against a limit of 4500. The docks,
  ships and perch add 1215.
- **Tests:** 27 pass on the built pack.

**Spawns and the inventory screen:** the hall spawns were moved out of the
inventory stations' reach. The screen now opens on its own only after a
second alive, so you don't respawn into it.

## Outpost tunnels (2026-10-03)

Each team's cliff-side outpost now has its own way into the base
underground (`scripts/assets/ozarktic_tunnel.py`, asset `ozarktic-tunnel-v1`).

- **Route.** A door in the back wall of the short tunnel between the
  generator room and the outbuilding opens on a branch running away from
  the field. A row then runs along world X under the slope behind the base,
  and a column along world Z ends at a stair house just behind the outpost.
  The ground falls away in front of both bases, so the tunnels run behind
  them. Ember's runs 336 m, Glacier's 392 m, each plus its stair house.
  Glacier's is longer because its base is turned 180 degrees while the
  outposts mirror front to back, which puts its short tunnel on the side away
  from its outpost.
- **Generator.** The generator room keeps exactly two ways in: the hall
  stair and its tunnel door. The tunnel door is now also reached from the
  outpost, underground.
- **Inside.** 6.4 m wide with 5.5 m of headroom, lit down the middle of the
  ceiling, with a lamp every other cell. The floor climbs from the base's
  underground level to the stair foot, never more than 25 degrees and never
  dipping back toward the base. Going down is a ski run to the base; going up
  is a walk.
- **On the surface.** Every tunnel cell is a terrain hole. Its lid is the
  ground itself: the terrain's heights at the cell corners are pinned, so the
  land runs on unchanged (no corner needed raising), and the lid is laid in
  rock, a stone path marking the route. The lid is drawn as a finer grid with
  the terrain's own normals so it shades like the ground round it; it is
  solid as two triangles a cell.
- **Stair house.** A covered 30 degree ramp (13 m, inside the walk limit) from the tunnel up to a
  landing, open toward the outpost, with an apron levelled in front. It sits
  into the hillside, and its roof clears the ground round it.
- **Budget.** 2108 (Ember) and 2154 (Glacier) collision triangles per base,
  tunnel included, against 4500. 141 terrain holes.
- **Tests** (`test-ozarktic-blast.py`): every hole lies under a base, a lid
  or a stair house, and each lid corner keeps the terrain's height. Down every
  cell: floor under foot, the full headroom, ground over the roof, the slope
  limit, and lamps. The short tunnel's door is open, and the stair house is
  open with level ground at its door. No turret sees into either tunnel. The
  generator still has exactly two ways in. Captures were checked in the client
  (aerials, both tunnels, the junctions, the stair houses inside and out).
- **Not playtested:** whether an underground route from the outpost makes
  the generators too easy to reach, and how bots use the tunnels.

## Open questions

- Balance of the hill and valley starts, after measured route timings and a
  playtest.
- Whether the valley needs a second escape path out of the head of the valley.
