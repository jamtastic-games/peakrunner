# Next development lineup

Agreed workflow: one focused branch per feature and one per map. Finish, review
and test a branch before merging and pushing to main. Start each next branch
from the updated main (refresh any reserved branch that has no work yet).
Do not build all these features together in one long-lived branch.

## First: establish the released baseline

The deployed `.20260919.4` source is archived in the visual-release runbook.
The baseline was assembled in an isolated release-review worktree from the exact
published source snapshot plus reviewed build/deployment documentation. The
original `terrain-materials` working tree was preserved during review. Main must
receive this reviewed baseline before the feature/map branches are initialized.
Never sweep private extraction directories, signing keys, VM disks or `.DS_Store`
files into a release commit. Saved client preferences are a separate new feature,
not part of the already published `.4` binaries.

## Ordered branches

| Order | Branch | Scope and completion target |
| --- | --- | --- |
| 1 | `feature/find-rift-preferences` | Persist name, directory and direct server per user on Mac/Windows/Linux; never passwords. Upgrade-safe location, validation, atomic saves and nonfatal errors. |
| 2 | `feature/multi-map-system` | Distinct map IDs/manifests, selectable installed packs, per-map compatibility, spawn metadata and supported modes. Server selects map; launcher distributes signed packs. Preserve Old Holler and avoid one-map-slot replacements. |
| 3 | `map/rift-crossing` | Proposed original map: readable routes and a central crossing, authored terrain and assets. First prove the multi-map pipeline end to end. |
| 4 | `map/frostline-basin` | Proposed original snow/alpine map with long ski routes and defensible bases. Independent art/layout/playtest branch. |
| 5 | — | Tower Complex fills the elevated-base role. |
| 6 | `feature/deathmatch` | Server-selected free-for-all rules, score/time limits, safe respawns, self/environment death handling, scoreboard, ties and round reset. Flags/CTF objectives disabled. |
| 7 | `feature/team-deathmatch` | Team score totals, team-colored spawns, balancing, team chat, explicit friendly-fire policy, victory and empty-server reset. Reuse common round rules. |
| 8 | `feature/inventory-loadouts` | Server-owned inventory choices, station access/power/team checks, limited carried deployables and loadout UI. No client-authoritative grants. Keep the existing kit as the default. |
| 9 | `feature/deployable-turrets` | Inventory-supplied turret placement, preview, authoritative range/surface/clearance validation, team ownership, LOS, per-player/team caps, damage/repair and round cleanup. |
| 10 | `feature/deployable-walls` | Inventory-supplied barriers with server collision, replicated destruction, placement restrictions around spawns/stations/objectives, finite stock/caps and cleanup. Test high-speed movement and projectile sweeps against them. |
| 11 | `feature/armor-classes` | Light/medium/heavy loadout choices: distinct silhouettes, health/energy/carry limits and tested mobility tradeoffs. Existing approved movement remains the light/default baseline; tune new classes explicitly. |

The three map names and inspiration choices are proposals, not claims of exact
copies or finished maps. Use original meshes, terrain, textures, audio and layout
authoring. Reference installations stay private and never enter packs/releases.
User can change map priorities before each map branch begins.

## Gate for every main merge

1. Review only the branch's scoped diff; no secrets, imported commercial assets,
   unrelated local changes or regressions to approved movement.
2. Pass workspace library tests, all-target compilation, app-boundary checks and
   applicable cross-platform builds. Test malformed network inputs and authority
   rules whenever gameplay messages or server state change.
3. Render and inspect real game views. Test collisions, spawning, joining,
   leaving/rejoining, match end and empty-server reset for the affected features.
4. New maps: deterministic builds and checksums, spawn safety, traversal in both
   directions, ceilings/doors/ramps, projectile collision, equipment circuits,
   visibility and performance. Do not remove Old Holler to add another map.
5. Modes/deployables/armors: actual multi-client matches, late joins, packet loss,
   team privacy and bounded entity/snapshot budgets. Record runtime coverage per
   platform honestly; cross-compilation is not a Windows gameplay pass.
6. Human playtest approval where feel or balance changes. Do not call pending
   platform or playtest work fully tested.
7. Merge the tested commit to main, push and verify the remote revision. Then
   perform a coordinated release where required, preserving previous artifacts.
   A main merge, public deployment and launcher publication are separate steps.

Do not delete branches or force-push without permission. Do not publish half of
an incompatible client/server/map update. Bump the compatibility marker only for
actual protocol/simulation/map changes, not merely a visual version alignment.

## Moddable engine (planned)

`docs/modding.md` is the agreed design for turning modes, definitions, rules
and maps into packages: data-only clients, the base game as a package,
default maps as installed packs, and missing content downloaded on join with
the player's approval. Its phases are separate branches in this workflow:

| Phase | Branch (proposed) | Milestone |
| --- | --- | --- |
| P0 | `mods/guard-rails` | Behaviour traces and budgets recorded |
| P1 | `mods/modular-packs` | Maps load from installed packs through the engine catalog |
| P2 | `mods/sessions-fetch` | **Modular maps downloaded with approval** |
| P3 | `mods/definitions` | Weapons, armor and equipment as data |
| P4 | `mods/mode-policy` | **Data-only mods** (tuning packs, mode variants) |
| P5 | `mods/rule-sets` | The `RuleSet` trait (done); typed events, generic items and zones (to do) |
| P6 | `mods/scripts` | Official modes run as scripted packages |
| P7 | `mods/sdk` | Public SDK, terms and security review |
