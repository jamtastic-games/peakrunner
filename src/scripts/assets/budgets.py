"""Shared limits that every map's test suite checks. Kept out of the builders
so changing a limit never changes any pack's recorded source hashes."""

# Solid (collision) triangles in one team base, including its outbuildings,
# tunnels and pads. Raised from 3000 for Dustreach's cistern level and
# storehouse after measuring tick and query cost; see docs/map-pipeline.md.
COLLISION_TRIS_PER_BASE = 4500
