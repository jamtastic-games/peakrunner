# Art direction: people, armor and weapons

PeakRunner is a team shooter of jetpacks and skiing across mountain, desert,
snow and coastal outposts. These rules keep every person, armor and weapon
recognizable and part of one world. The pipeline that turns them into models is
`docs/model-pipeline.md`.

## What a player must read, in order

1. **Team**, from as far as a player can be seen: Ember (red-orange) or Glacier
   (teal-blue), on the same large plates of every armor.
2. **Armor size**, from the outline alone, before any detail shows:
   small, medium or large.
3. **Weapon**, from its outline in the hands at 30 m, and from first person at a
   glance: every weapon has one signature shape no other has.

## Style: frontier industrial tech

- Practical, machined, field-maintained equipment: chunky plates with bevelled
  edges that catch the light, layered panels, visible fasteners and hinges,
  cable runs, vents. Engineered rather than ornamental.
- Not grimy realism, not toy-like, no spikes or skulls, no text, letters, logos
  or insignia anywhere.
- Shape hierarchy: a few big forms, fewer medium ones, many small details, about
  60/30/10. Big forms carry the read; small details stay quiet at range.
- Painted detail rather than noise: edge highlights, panel seams, light chips
  on edges, slight soot near vents and muzzles. Clean enough to read on a
  simply lit world.

## Palette

| Role | Color | Notes |
|---|---|---|
| Team plates | Ember `#BD331F`, Glacier `#1F82A6` | In source art they are **key magenta `#E000E0`**; the engine paints them in the team's color |
| Undersuit | charcoal `#16181B` | The dark suit between plates; matte |
| Frame metal | gunmetal `#3A4048` | Weapon bodies, armor frames |
| Bright metal | brushed steel `#8E98A3` | Edges, machined faces, rails |
| Rubber and straps | warm black `#1E1A17` | Grips, boots, webbing |
| Glow | cyan-white `#5CE6FF` | Lights, lenses, cells; the same on both teams |
| Warning | amber `#E0A030` | Only small marks: a safety, a cell gauge |

## Armor: three sizes

All three wear the same quilted charcoal undersuit, the same closed helmet
(a narrow cyan-white visor band over a vented faceplate; no face shows,
chosen 2026-10-05) and a jetpack; they differ in mass and outline. Team
paint goes on the same plates on every size: the chest plate, pauldrons,
thighs, the helmet's crown and cheeks. Gloves are dark and armored, with a
gunmetal back plate, knuckle guard and a small cyan light.

In play the engine colors the rest of the armor too (since 2026-10-14):
every bare plate the art leaves in steel or gunmetal takes a darker tone of
the team's color (`player_model::TEAM_PLATES`, `model.wgsl` `PLATE_TONE`
0.7, half its metal traded for paint), so the key-magenta paint still reads
as the bright accent and the undersuit stays charcoal. A body's team then
shows from behind and at range, the way players tell the teams apart now
that only the player under the crosshair is named. The art files don't
change: the tint follows the texture's own grays. Where a mode has no
teams (Rabbit, free-for-all) nobody wears a team's color: every body is in
the art's own steel, team paint included, and Rabbit's rabbit in the
flag's gold on every plate, lit by itself (`player_model::livery`,
`RABBIT_SHINE`).

- **Small** (scouts, runners, infiltrators): slim and upright. Close-fitting
  segmented plates with undersuit showing between them, small pauldrons that
  stay below the head, a narrow chest plate, light shin guards, a slim helmet
  with a short antenna, and a compact single-nozzle jetpack. Reads as fast.
- **Medium** (troopers, engineers, support): balanced. A full chest plate with
  an abdomen guard, rounded pauldrons as wide as the chest, thigh plates and
  greaves, a rounded helmet with a brow ridge, and a twin-nozzle jetpack. Reads
  as dependable.
- **Large** (heavies, anchors, siege): massive. Pauldrons that rise to the
  helmet line, a deep chest with a high collar around the neck, thick armored
  forearms, heavy greaves and broad boots, a squat helmet set into the collar,
  and a big four-nozzle jetpack. Reads as a wall.

## Weapons: one family, one signature each

Shared: gunmetal bodies, brushed-steel edges, warm-black grips, cyan-white glow
for energy parts, one small key-magenta team stripe. Size follows role: light
weapons are compact, heavy ones big and two-handed.

| Weapon | Signature shape |
|---|---|
| Disc launcher | A flat drum magazine on top with the next disc's edge showing through a slot; a clamshell muzzle |
| Repeater (id `chaingun`) | A thick finned heat-sink shroud ending in a slotted muzzle brake, a curved magazine angled forward |
| Grenade launcher | A stubby wide barrel and a six-chamber revolving cylinder |
| Mortar | A fat short tube with a funnel breech and stabilizer fins |
| Laser rifle | A long open frame around a glowing crystal rod, coil along the barrel, a scope |
| Railgun | Two parallel rails with an open gap at the front, a capacitor block |
| Plasma gun | A glass chamber of glowing plasma mid-body, a short wide emitter |
| Blaster | A compact sidearm with finned heat sinks and a ring muzzle |
| Breacher | A wide flat rectangular spread muzzle and a side shell rack |
| Longbore | A very long thin barrel with a muzzle brake and a large scope |
| Slugger | A heavy hand cannon with a huge bore |
| Hailstorm | Three big barrels in a triangular housing, a belt from a rear drum |
| Torch | Twin fuel cylinders under the body, a nozzle with a pilot light and heat-shield ring |
| Gust cannon | A wide bell mouth with turbine blades visible in the throat |
| Spark rifle | Forked electrode prongs at the muzzle, insulator rings along the barrel |
| Tether | A barbed hook on the barrel tip, a cable spool on the side |
| Hunter launcher | A shoulder launch tube with a targeting dish on top |
| Lance | A long pointed emitter with stacked rings that glow as it charges |
| Multitool | A three-pronged emitter claw, a tool head and a gauge |
| Drain gun | A wide intake nozzle and a canister |
| Target marker | A small hand designator with a lens and an antenna |

## Originality

Every design is original. Never base one on, imitate or name an existing
game, film, franchise or real product, in prompts, files or builds.
