"""Where a CTF centre's turret stands on the shared C&H tower
(`cnh_tower.py`): on top of the crown's lantern box, clear of its glow and
prongs; and how far it reaches. Apart from `cnh_tower` so the maps that only
place the tower keep that module's source hash (`cnh_asset_sha256`)."""
import math

from assets import cnh_tower

TURRET_Y = cnh_tower.CROWN_Y+4.65

# A spec's `"turret_range": "half_to_base"`: the turret reaches half way
# from its tower to the nearest building a base's generator powers.
HALF_TO_BASE = 'half_to_base'


def half_to_base(centre, entities):
    """Half the level distance from `centre` to the nearest building on a
    generator's circuit (a station, turret, sensor or repair post; not the
    generators themselves), either base."""
    powered = {(e['team'], e['circuit']) for e in entities if e['kind'] == 'generator'}
    near = min(math.dist((e['position'][0], e['position'][2]), (centre[0], centre[2]))
               for e in entities if e['kind'] != 'generator' and (e['team'], e['circuit']) in powered)
    return round(near/2, 1)


def resolve_ranges(control_points, entities):
    """Fill in the turrets' ranges marked `HALF_TO_BASE`, once the bases'
    buildings are all placed."""
    for cp in control_points:
        turret = cp.get('turret')
        if turret and turret.get('range') == HALF_TO_BASE:
            turret['range'] = half_to_base(cp['pos'], entities)
