# Example mods

Copy a folder into your content folder's `mods/` (see
`docs/making-content.md`), then run a server with it:

```sh
peakrunner-server --check examples               # check them (the content folder)
PEAKRUNNER_PACK_DIR=$PWD/examples PEAKRUNNER_MODS=quick-ctf \
  PEAKRUNNER_MATCH_ROTATION='[{"map":"old-holler","mode":"quick-ctf"}]' peakrunner-server
```

- `low-gravity`: changes one definition (gravity) for every mode.
- `quick-ctf`: adds a mode, a variant of Capture the Flag.
- `sidearms`: changes weapon definitions and adds a Team Deathmatch variant.
- `spree`: a mode with its own rules (`rules/spree.rhai`, run by the
  server): every third frag in a row scores double, a suicide costs a point.
- `disc-turret`: a deployable that extends the base turret to fire discs,
  and the pack that places it (the engine's classes).
- `drain-gun`: patches the base game's drain beam (a `stream` projectile):
  it reaches farther and mends teammates and equipment.
- `guardian`: a heavy armor with a plate in front (`mounts`), a ward on its
  class key (C), and a hook (`hooks/guardian.rhai`, run by the server).
- `motor-pool`: a pack that places the stock library's vehicle pad near
  your base, so any mode has the skiff, transport, Ridgerunner (a flying
  motorcycle) and Hogback (a tank).
- `sparkfall`: a full example of the engine's classes (`docs/mod-format.md`),
  below.

## Sparkfall
Nine classes, each with a class key (C), their own weapons, throwables and
packs; placed gadgets, fliers, a team purse and advanced scoring. Built on
the engine's classes (`docs/mod-format.md`) as a full example of what a mod
can do without code: every number is data, and the scoring is a hook
(`hooks/scoring.rhai`, run by the server only).

| Class | Role | Class key |
|---|---|---|
| Courier | Fast flag runner, two slots | Overdrive (too much hurts) |
| Longshot | Marksman, the Longbore | none |
| Veil | Infiltrator, veil pack, blight, tripmines | a breach charge |
| Striker | All-rounder, Hailstorm and Hunter | Surge |
| Bombard | Explosives, Hunter, Torch, Gust | a firebomb |
| Rigger | Builder, the Multitool, every placed thing | a medkit; touch mends |
| Warden | Support, pylons | self-veil |
| Bulwark | Heavy anchor, a plate in front | an emergency ward |
| Siegeframe | One per team: the Lance, a shoulder gun | Anchor |

Modes: `sparkfall-ctf` (with the team purse) and `sparkfall-tdm`.

```sh
peakrunner-server --check examples/mods/sparkfall  # one mod
PEAKRUNNER_PACK_DIR=$PWD/examples PEAKRUNNER_MODS=sparkfall \
  PEAKRUNNER_MATCH_ROTATION='[{"map":"old-holler","mode":"sparkfall-ctf"}]' peakrunner-server
```

Names are placeholders until checked for trademarks.
