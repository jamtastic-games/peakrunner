"""Fairness without mirroring (docs/map-pipeline.md, Terrain).

Maps are shaped like real land: one half's hills and valleys need not match
the other's. These checks stand in for exact symmetry. They compare what
each team gets:

- the ground of each half of the playing field: how steep it is (median and
  90th percentile slope, the share over 30 degrees) and how high it lies;
- the approach to each base: the climb an attacker makes from the field's
  middle up to that flag, and the drop a defender has leaving it, averaged
  over parallel lanes either side of the flag line.

`report` returns the numbers; `assert_fair` checks them against tolerances.
The flag axis is world Z (one flag at low Z, the other at high Z).
"""
import numpy as np

STEP = 8.0
N = 256


def slope_degrees(grid, step=STEP):
    gz, gx = np.gradient(np.asarray(grid, np.float64), step)
    return np.degrees(np.arctan(np.hypot(gx, gz)))


def sample(grid, x, z, step=STEP):
    """Bilinear height at world (x, z), as the engine samples the field."""
    fx = np.clip(np.asarray(x, np.float64)/step, 0, N-1.001); fz = np.clip(np.asarray(z, np.float64)/step, 0, N-1.001)
    ix, iz = fx.astype(int), fz.astype(int); tx, tz = fx-ix, fz-iz
    g = np.asarray(grid, np.float64)
    return (g[iz, ix]*(1-tx)*(1-tz)+g[iz, ix+1]*tx*(1-tz)+g[iz+1, ix]*(1-tx)*tz+g[iz+1, ix+1]*tx*tz)


def halves(flags, reach=360.0, step=STEP):
    """Boolean grid masks of each team's half of the playing field: the band
    `reach` metres either side of the flag line, from that flag's end of the
    field (40 m behind it) to the middle (keeping 24 m clear of it)."""
    (ax, _, az), (bx, _, bz) = flags
    mid_z, line_x = (az+bz)/2, (ax+bx)/2
    z, x = np.mgrid[0:N, 0:N].astype(np.float64)*step
    band = np.abs(x-line_x) <= reach
    lo, hi = sorted((az, bz))
    first = band & (z >= lo-40) & (z <= mid_z-24)
    second = band & (z >= mid_z+24) & (z <= hi+40)
    return (first, second) if az < bz else (second, first)


def approach(grid, flag, other, lanes=9, spread=45.0, ds=4.0):
    """(climb, drop): metres of ascent from the field's middle up to `flag`,
    and of descent from it back to the middle, averaged over `lanes`
    parallel lanes `spread` metres apart across the flag line."""
    fx, _, fz = flag; ox, _, oz = other
    mx, mz = (fx+ox)/2, (fz+oz)/2
    length = np.hypot(fx-mx, fz-mz)
    ux, uz = (fx-mx)/length, (fz-mz)/length          # middle -> flag
    px, pz = -uz, ux
    t = np.arange(0.0, length+1e-9, ds)
    climbs, drops = [], []
    for k in range(lanes):
        off = (k-(lanes-1)/2)*spread
        # Each lane closes on the flag over its last third, as players do.
        close = np.clip((t/length-2/3)*3, 0, 1)
        xs = mx+ux*t+px*off*(1-close); zs = mz+uz*t+pz*off*(1-close)
        d = np.diff(sample(grid, xs, zs))
        climbs.append(d[d > 0].sum()); drops.append(-d[d < 0].sum())
    return float(np.mean(climbs)), float(np.mean(drops))


def report(grid, flags, reach=360.0):
    """Per team (in flag order): slope median, 90th percentile, share over 30
    degrees, mean height of its half, and the approach climb and drop."""
    s = slope_degrees(grid)
    out = []
    for i, mask in enumerate(halves(flags, reach)):
        climb, drop = approach(grid, flags[i], flags[1-i])
        out.append(dict(median=float(np.median(s[mask])), p90=float(np.percentile(s[mask], 90)),
                        steep=float((s[mask] > 30).mean()), mean_height=float(np.asarray(grid)[mask].mean()),
                        climb=climb, drop=drop))
    return out


DEFAULT = dict(median=3.0, p90=4.0, steep=.06, mean_height=12.0, climb=.25, drop=.25)


def assert_fair(test, grid, flags, reach=360.0, **tolerances):
    """Both halves alike within tolerances: degrees for the slope figures, a
    share for `steep`, metres for `mean_height`, and a share of the larger
    for the approach climb and drop (with 6 m of slack for gentle ones).
    Returns the report."""
    tol = dict(DEFAULT, **tolerances)
    a, b = report(grid, flags, reach)
    for key in ('median', 'p90', 'steep', 'mean_height'):
        test.assertLessEqual(abs(a[key]-b[key]), tol[key], (key, a, b))
    for key in ('climb', 'drop'):
        test.assertLessEqual(abs(a[key]-b[key]), max(a[key], b[key])*tol[key]+6.0, (key, a, b))
    return a, b


def symmetric(grid, tol=1e-6):
    """Whether the field is exactly point-symmetric about the map centre (a
    map converted to unmirrored ground should no longer be)."""
    g = np.asarray(grid)
    return float(np.abs(g[1:, 1:]-g[1:, 1:][::-1, ::-1]).max()) < tol
