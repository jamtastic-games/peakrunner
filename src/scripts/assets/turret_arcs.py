"""Facing and field of fire for fixed turrets.

Doorways and windows are open, so a turret standing in front of a building
would otherwise have a straight line back through them into its own rooms.
Instead of walling the openings, each turret faces the enemy flag and only
engages targets beyond 15 m (equipment::ALL_ROUND_RANGE) inside an
`ARC`-degree field (the manifest's `facing` and `arc`, enforced by the
server). Close in it still covers every direction, so it guards its own
bridge and ramps; 200 degrees covers the field and both flanks but not the
building behind it.
"""
import math

ARC = 200.0


def assign(entities, flags, arc=ARC, overrides=None):
    """Set `facing`/`arc` on every turret in `entities` (manifest dicts).

    `flags` is [red_flag, blue_flag] in world space. `overrides` maps an
    entity id to a (facing_xz, arc) pair for turrets that need something
    other than "toward the enemy flag"."""
    overrides = overrides or {}
    for e in entities:
        if e['kind'] != 'turret':
            continue
        if e['id'] in overrides:
            facing, a = overrides[e['id']]
        else:
            own, foe = flags[e['team']], flags[1 - e['team']]
            facing, a = (foe[0] - own[0], foe[2] - own[2]), arc
        n = math.hypot(*facing)
        e['facing'] = [round(facing[0] / n, 6), round(facing[1] / n, 6)]
        e['arc'] = a
    return entities
