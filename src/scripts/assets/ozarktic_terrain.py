"""Original terrain for Ozarktic Blast (docs/ozarktic-blast.md).

Generated only from PeakRunner's own hash noise (shared with Tower Complex's
terrain module) and authored shapes; no heightfield from any other game is
read, resampled, traced or fitted. The layout: a ridge astride the flag
line, high ground on one side, a low dock valley on the other.

Flag axis is world Z (Ember at low Z); the flags lie on the line x = 1024.
- The west is high ground: a broad rounded upland rising away from the
  flag line (on your right as you face the enemy from Ember).
- The Ridge crosses the flag line at the centre: a spur off the west upland
  with a small flat top about 95 m over the flags and a steep east face.
- The east is a low valley holding the two dry docks, one per half, then a
  rim of hills at the east edge where the sniper's perch stands.
- The flags sit on low ground with a gentle rise behind each.

The large forms mirror front to back (z -> 2048 - z), so neither team has
the better ground; the surface detail is not mirrored, so the land still
reads as natural. Structures sit in "sites": the ground is blended toward an
authored surface around each one. Terrain holes sit only under structures.
"""
import numpy as np

from assets.tower_complex_terrain import _fbm, _smooth, slope_degrees, N, STEP

CENTRE = 1024.0
FLAG_X = 1024.0
FLAG_DZ = 314.0                          # flags at CENTRE -/+ this along Z
PLAIN = 150.0
WEST = dict(x0=1010.0, x1=700.0, h=90.0)            # upland: rises from x0 to its crest at x1
RIDGE = dict(top=262.0, peak=92.0, core=55.0, shoulder=150.0, shoulder_h=40.0,
             east=(1010.0, 1085.0), west_end=640.0)  # crest profile along z; east face between these x
VALLEY = dict(x=(1070.0, 1400.0), depth=3.0)
EAST_RIM = dict(x=1500.0, h=60.0, w=110.0)
BEHIND = dict(dz=100.0, h=40.0, r=140.0)  # the gentle rise behind each flag
DETAIL, DETAIL_MID, FINE, CRAG = 11.0, 7.0, 2.4, 22.0


def _warp(x, z, seed, amount=50.0):
    """Domain warp: bend coordinates so shapes never read as perfect curves."""
    wx = (_fbm(x+333, z-117, 260, 3, seed+41)-.5)*2*amount
    wz = (_fbm(x-911, z+505, 260, 3, seed+43)-.5)*2*amount
    return x+wx, z+wz


def _mirror(f, x, z, seed):
    """Front/back mirrored large form: the max of f at z and at 2048 - z."""
    return np.maximum(f(x, z, seed), f(x, 2*CENTRE-z, seed))


def _west(x, z, seed):
    wx, wz = _warp(x, z, seed+1, 40.0)
    rise = _smooth(WEST['x0'], WEST['x1'], wx)
    return WEST['h']*rise*(.85+.3*_fbm(wx+50, wz+900, 300, 3, seed+21))


def _ridge_half(x, z, seed):
    """One half of the ridge crest (the other half is its mirror)."""
    dz = np.abs(z-CENTRE)
    crest = RIDGE['peak']*np.exp(-(dz/RIDGE['core'])**2)+RIDGE['shoulder_h']*np.exp(-(dz/RIDGE['shoulder'])**2)
    east = 1-_smooth(*RIDGE['east'], x)              # the steep east face
    # A spur: highest at the flag line, sinking westward into the upland.
    west = .25+.75*_smooth(RIDGE['west_end'], FLAG_X-20, x)
    return crest*east*west


def _ridge(x, z, seed):
    h = _ridge_half(x, z, seed)
    # A small flat top.
    cap = RIDGE['top']-PLAIN
    return np.minimum(h, cap)+(h > cap)*0


def _east(x, z, seed):
    valley = -VALLEY['depth']*_smooth(VALLEY['x'][0], VALLEY['x'][0]+80, x)*(1-_smooth(VALLEY['x'][1]-60, VALLEY['x'][1]+40, x))
    rim = EAST_RIM['h']*np.exp(-((x-EAST_RIM['x'])/EAST_RIM['w'])**2)*(.8+.4*_fbm(x, z+300, 200, 3, seed+31))
    return valley+rim


def _behind(x, z, seed):
    out = 0.0
    for side in (-1, 1):
        cz = CENTRE+side*(FLAG_DZ+BEHIND['dz'])
        out = out+BEHIND['h']*np.exp(-((x-FLAG_X)**2+(z-cz)**2)/BEHIND['r']**2)
    return out


def natural(x, z, seed):
    """Natural ground before any structure site."""
    big = _mirror(_west, x, z, seed)+_ridge(x, z, seed)+_east(x, z, seed)+_behind(x, z, seed)
    wx, wz = _warp(x, z, seed+7, 45.0)
    calm = 1-.7*np.clip(_ridge(x, z, seed)/60, 0, 1)
    detail = calm*(DETAIL*(_fbm(wx, wz, 200, 4, seed)-.5)*2+DETAIL_MID*(_fbm(wx+400, wz+700, 70, 3, seed+9)-.5)*2)
    # Crags: sharp ridged detail that gives the land its steep bits.
    detail = detail+calm*CRAG*(_fbm(wx-200, wz+150, 110, 3, seed+15, ridged=True)-.35)
    h = PLAIN+big+detail+FINE*(_fbm(x+900, z+100, 36, 2, seed+5)-.5)*2
    edge = np.maximum(np.abs(x-CENTRE), np.abs(z-CENTRE))
    return h+60*_smooth(760, 1010, edge)


def heights(seed, sites=(), pins=None):
    """256x256 heights in metres (row = z cell, column = x cell).
    `sites` are (surface(x,z)->height, weight(x,z)->0..1) pairs over world
    grids; `pins` maps (ix, iz) grid vertices to fixed heights (the corners
    of a tunnel's lid)."""
    z, x = np.mgrid[0:N, 0:N].astype(np.float64)*STEP
    h = natural(x, z, seed)
    for surface, weight in sites:
        w = weight(x, z)
        h = h*(1-w)+surface(x, z)*w
    for (ix, iz), y in (pins or {}).items():
        h[iz, ix] = y
    return np.clip(h, 2, 2040)


def weights(h):
    """Frosted grass / rock / snow / dark heath splat: bare rock on the
    steep faces, snow on the ridge top and the west heights, heath on middle
    slopes, grass everywhere else."""
    s = slope_degrees(h)
    rock = np.clip((s-30)/10, 0, 1)*.94+.02
    high = np.clip((h-232)/30, 0, 1)
    snow = np.clip(1-rock, 0, 1)*np.clip(high*.85+np.clip((10-s)/40, 0, .12), 0, .9)
    duff = np.clip(1-rock-snow, 0, 1)*np.clip((s-9)/20, 0, .6)
    grass = np.clip(1-rock-snow-duff, 0, 1)
    out = np.stack([grass, rock, snow, duff], axis=-1)
    return np.round(out*255).astype(np.uint8)
