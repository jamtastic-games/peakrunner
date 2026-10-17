"""Ashveil's beasts: the bones of giant many-legged creatures from before
the caldera, half sunk in its floor (our own design, docs/ashveil.md;
`maps/ashveil.json` "beasts").

- The centipede: a long spine of vertebrae dipping in and out of the
  ground, a pair of curved legs on every segment arching out and down into
  the ash (a row of arches either side), and a head whose mandibles cross
  over the way in front of it.
- The stalker: a mantis-like giant fallen on its belly, its four walking
  legs still standing as tall arches, its two scythe forelegs raised, its
  abdomen a row of plates sinking into the ground.
- The ribcage: a fallen thorax, its ribs meeting overhead in pairs, a bone
  tunnel to ski through.

Everything is built in world metres from bone tubes and knobs, solid where
someone could hit it. `ground(x, z)` gives the terrain's height.
"""
import math

import numpy as np

from assets import ashveil_shapes as A


def bezier(p0, p1, p2, n):
    t = np.linspace(0, 1, n)[:, None]
    p0, p1, p2 = (np.asarray(p, np.float64) for p in (p0, p1, p2))
    return (1-t)**2*p0+2*(1-t)*t*p1+t**2*p2


def limb(points, r0, r1, seed, material='bone', sides=8, rough=.06):
    """A bone along `points`, `r0` thick at its start and `r1` at its end,
    with a knob at the start."""
    pts = np.asarray(points, np.float64)
    radii = np.linspace(r0, r1, len(pts))
    radii *= 1+.12*np.sin(np.linspace(0, math.pi, len(pts)))
    out = [A.tube(pts, radii, sides=sides, seed=seed, rough=rough, material=material, smooth=True)]
    c = pts[0]
    out.append(A.blob(c-r0*1.35, c+r0*1.35, seed+3, rough=.12, rings=5, sides=8, material=material))
    return out


def spike(base, tip, r, seed, material='bone'):
    """A thorn from `base` to `tip`."""
    return A.tube([base, (np.asarray(base)*.4+np.asarray(tip)*.6), tip], [r, r*.55, r*.05], sides=6, seed=seed,
                  material=material)


def centipede(spec, ground, seed):
    q = np.asarray(spec['points'], np.float64)
    # Resample the path every segment.
    seg = spec['segment']
    d = np.r_[0, np.cumsum(np.linalg.norm(np.diff(q, axis=0), axis=1))]
    s = np.arange(0, d[-1], seg)
    path = np.c_[np.interp(s, d, q[:, 0]), np.interp(s, d, q[:, 1])]
    tan = np.gradient(path, axis=0); tan /= np.linalg.norm(tan, axis=1)[:, None]
    rng = np.random.default_rng(seed)
    n = len(path)
    out = []
    lift = spec['lift']
    for i, ((x, z), (tx, tz)) in enumerate(zip(path, tan)):
        u = i/max(n-1, 1)
        # The spine rides up and down through the ground, lowest at the tail.
        rise = lift*(.55+.45*math.sin(u*math.tau*spec['waves']+.7))*(1-.6*u)
        y = ground(x, z)+rise
        w = spec['width']*(1-.45*u)
        left = np.array([-tz, tx])
        if y-w*.35 < ground(x, z)-.5:
            continue
        # A plate of its back over a knobbed vertebra, and a dorsal spine.
        a = np.array([x-tx*seg*.42, y, z-tz*seg*.42]); b = np.array([x+tx*seg*.42, y, z+tz*seg*.42])
        out.append(A.tube([a, (a+b)/2, b], [w*.26, w*.32, w*.26], sides=10, seed=seed+i, rough=.08, material='bone'))
        plate = A.blob((-seg*.62, -w*.95, -w*.18), (seg*.62, w*.95, w*.42), seed+i*5, rough=.08, rings=6, sides=14,
                       material='bone_grime', flat_bottom=.35)
        ang = math.atan2(tz, tx)
        c_, s_ = math.cos(ang), math.sin(ang)
        v = plate.v.copy()
        plate.v = np.c_[x+v[:, 0]*c_-v[:, 1]*s_, y+w*.12+v[:, 2], z+v[:, 0]*s_+v[:, 1]*c_]
        A._orient(plate)
        out.append(plate)
        out.append(spike((x, y+w*.5, z), (x-tx*2.0, y+w*.5+rng.uniform(2.5, 4.5), z-tz*2.0), w*.14, seed+i*7))
        # A pair of legs bowing up and out, then down into the ash; some broke.
        for side in (-1, 1):
            if rng.random() < .08: continue
            hip = np.array([x+left[0]*side*w*.8, y+w*.1, z+left[1]*side*w*.8])
            reach = spec['reach']*(1-.3*u)*rng.uniform(.88, 1.1)
            fx, fz = x+left[0]*side*reach-tx*rng.uniform(0, 3), z+left[1]*side*reach-tz*rng.uniform(0, 3)
            foot = np.array([fx, ground(fx, fz)-1.5, fz])
            knee_xz = hip[[0, 2]]+(np.array([fx, fz])-hip[[0, 2]])*.42
            knee = np.array([knee_xz[0], max(hip[1], ground(*knee_xz))+spec['arch']*(1-.35*u), knee_xz[1]])
            r = w*.15
            femur = bezier(hip, hip+(knee-hip)*.15+np.array([0, (knee[1]-hip[1])*.9, 0]), knee, 9)
            out_dir = np.array([foot[0]-knee[0], 0, foot[2]-knee[2]])
            tibia = bezier(knee, knee+out_dir*.55+np.array([0, 1.5, 0]), foot, 10)
            if rng.random() < .12:
                # Broken at the knee: the lower bone lies in the ash.
                out += limb(femur, r, r*.75, seed+i*13+side)
                lx, lz = knee[0]+(fx-knee[0])*.7, knee[2]+(fz-knee[2])*.7
                out += limb([(knee[0], ground(knee[0], knee[2])+r*.6, knee[2]), (lx, ground(lx, lz)+r*.5, lz)], r*.7, r*.4,
                            seed+i*17+side)
                continue
            out += limb(femur, r, r*.8, seed+i*13+side)
            out += limb(tibia, r*.8, r*.3, seed+i*19+side)
            p = tibia[3]
            out.append(spike(p, p+np.array([out_dir[0]*.12, 1.2, out_dir[2]*.12]), r*.3, seed+i*23+side))
    # The head: a cracked dome, and mandibles crossing in front of it.
    (x, z), (tx, tz) = path[0], -tan[0]
    y = ground(x, z)+lift*.9
    hw = spec['width']*1.1
    c = np.array([x+tx*hw*.6, y, z+tz*hw*.6])
    out.append(A.blob(c-np.array([hw, hw*.6, hw]), c+np.array([hw, hw*.7, hw]), seed+901, rough=.14, rings=8, sides=14,
                      material='bone_grime', lumps=.08))
    left = np.array([-tz, tx])
    for side in (-1, 1):
        root = c+np.array([left[0]*side*hw*.7+tx*hw*.5, -hw*.2, left[1]*side*hw*.7+tz*hw*.5])
        tip = c+np.array([tx*spec['mandible']-left[0]*side*hw*.4, spec['mandible']*.15, tz*spec['mandible']-left[1]*side*hw*.4])
        mid = (root+tip)/2+np.array([left[0]*side*spec['mandible']*.45, spec['mandible']*.55, left[1]*side*spec['mandible']*.45])
        out.append(A.tube(bezier(root, mid, tip, 12), np.linspace(hw*.32, hw*.04, 12), sides=8, seed=seed+933+side,
                          rough=.05, material='bone'))
        for k in (.3, .55):
            p = bezier(root, mid, tip, 12)[int(k*11)]
            out.append(spike(p, p+np.array([0, -hw*.35, 0])+(c-p)*.08, hw*.08, seed+950+int(k*10)+side))
    return out


def stalker(spec, ground, seed):
    x, z = spec['at']
    a = math.radians(spec['facing'])
    f = np.array([math.cos(a), math.sin(a)]); l = np.array([-f[1], f[0]])
    rng = np.random.default_rng(seed)
    sc = spec['scale']
    out = []
    g = ground(x, z)

    def at(fwd, side, up):
        p = np.array([x, z])+f*fwd*sc+l*side*sc
        return np.array([p[0], ground(*p)+up*sc, p[1]])

    # The thorax, lying on the ground, and the head on its neck.
    t0, t1 = at(-6, 0, 2.2), at(8, 0, 3.0)
    out.append(A.tube(bezier(t0, (t0+t1)/2+np.array([0, 1.2*sc, 0]), t1, 9), np.array([4.6, 5.2, 5.0, 4.7, 4.4, 4.0, 3.6, 3.2,
                      2.8])*sc, sides=12, seed=seed+1, rough=.08, material='bone_grime'))
    neck = at(13, 0, 6.5)
    out.append(A.tube(bezier(t1, (t1+neck)/2+np.array([0, 1.0*sc, 0]), neck, 6), np.linspace(1.6, 1.1, 6)*sc, sides=10,
                      seed=seed+2, material='bone'))
    head = neck+np.array([f[0]*2.5*sc, .5*sc, f[1]*2.5*sc])
    out.append(A.blob(head-np.array([3.6, 2.6, 3.6])*sc, head+np.array([3.6, 3.0, 3.6])*sc, seed+3, rough=.1, rings=7,
                      sides=12, material='bone_grime'))
    for side in (-1, 1):
        horn = head+np.array([l[0]*side*1.6*sc, 1.5*sc, l[1]*side*1.6*sc])
        out.append(spike(horn, horn+np.array([l[0]*side*2.5*sc+f[0]*1.5*sc, 4.5*sc, l[1]*side*2.5*sc+f[1]*1.5*sc]), .5*sc,
                         seed+4+side))
    # The abdomen: plates sinking toward the tail.
    for k in range(7):
        c = at(-8-k*4.6, rng.uniform(-.5, .5), 2.4-k*.42)
        r = (4.8-k*.42)*sc
        out.append(A.blob(c-np.array([r, r*.55, r]), c+np.array([r, r*.6, r]), seed+20+k, rough=.1, rings=6, sides=12,
                          material='bone_grime' if k % 2 else 'bone', flat_bottom=.3))
    # Four walking legs still standing as arches.
    for k, (fwd, spread) in enumerate(((5, 26), (-2, 30), (-7, 29), (-12, 24))):
        for side in (-1, 1):
            hip = at(fwd, side*2.6, 3.0)
            foot = at(fwd+(-6 if fwd < 0 else 6)*.6, side*spread, -1.6)
            knee = at(fwd+(-3 if fwd < 0 else 3), side*spread*.48, spec['knee']+rng.uniform(-2, 2))
            r = .95*sc
            femur = bezier(hip, hip+(knee-hip)*.2+np.array([0, (knee[1]-hip[1])*.85, 0]), knee, 10)
            out_dir = np.array([foot[0]-knee[0], 0, foot[2]-knee[2]])
            tibia = bezier(knee, knee+out_dir*.5+np.array([0, 3*sc, 0]), foot, 11)
            out += limb(femur, r, r*.8, seed+40+k*3+side)
            out += limb(tibia, r*.8, r*.3, seed+60+k*3+side)
            for t in (.35, .6):
                p = tibia[int(t*10)]
                out.append(spike(p, p+np.array([l[0]*side*1.6*sc, .8*sc, l[1]*side*1.6*sc]), .22*sc, seed+80+k+int(t*10)))
    # The scythes: raised, folded at the elbow, toothed.
    for side in (-1, 1):
        sh = at(10, side*2.4, 5.0)
        elbow = at(14, side*5.5, spec['scythe'])
        tip = at(20, side*4.0, spec['scythe']*.55)
        out += limb(bezier(sh, (sh+elbow)/2+np.array([0, 2*sc, 0]), elbow, 8), 1.1*sc, .9*sc, seed+100+side)
        blade = bezier(elbow, elbow+(tip-elbow)*.4+np.array([f[0]*4*sc, 5*sc, f[1]*4*sc]), tip, 11)
        out.append(A.tube(blade, np.linspace(.9, .08, 11)*sc, sides=8, seed=seed+110+side, material='bone'))
        for t in (.25, .45, .65):
            p = blade[int(t*10)]
            out.append(spike(p, p+np.array([0, -1.6*sc, 0])+(elbow-p)*.1, .18*sc, seed+120+side+int(t*10)))
    return out


def ribcage(spec, ground, seed):
    x, z = spec['at']
    a = math.radians(spec['facing'])
    f = np.array([math.cos(a), math.sin(a)]); l = np.array([-f[1], f[0]])
    rng = np.random.default_rng(seed)
    out = []
    n, length, half, tall = spec['ribs'], spec['length'], spec['half'], spec['tall']
    # The spine along the top, sagging where the ribs gave way.
    tops = []
    for k in range(n):
        u = k/(n-1)
        c = np.array([x, z])+f*(u-.5)*length
        h = tall*(1-.18*(2*u-1)**2)+rng.uniform(-1, 1)
        tops.append(np.array([c[0], ground(*c)+h, c[1]]))
    out.append(A.tube(tops, np.linspace(1.5, 1.1, n), sides=10, seed=seed+1, rough=.08, material='bone_grime'))
    for k, top in enumerate(tops):
        c = top[[0, 2]]
        w = half*(1-.25*abs(2*k/(n-1)-1))
        for side in (-1, 1):
            if rng.random() < .08: continue
            fx, fz = c+l*side*w+f*rng.uniform(-1.5, 1.5)
            foot = np.array([fx, ground(fx, fz)-1.2, fz])
            ctrl = np.array([c[0]+l[0]*side*w*1.25, top[1]+2.0, c[1]+l[1]*side*w*1.25])
            out += limb(bezier(top, ctrl, foot, 11), .95, .45, seed+10+k*2+side, material='bone')
        out.append(spike(top+np.array([0, 1.0, 0]), top+np.array([-f[0]*2.0, rng.uniform(3, 5), -f[1]*2.0]), .5, seed+50+k))
    return out


def build(spec, ground, seed=0):
    """Every beast in `spec` ("beasts"): forms in the world."""
    out = []
    for k, c in enumerate(spec.get('centipedes', [])):
        out += centipede(c, ground, seed+1000*k+7)
    for k, s in enumerate(spec.get('stalkers', [])):
        out += stalker(s, ground, seed+1000*k+307)
    for k, r in enumerate(spec.get('ribcages', [])):
        out += ribcage(r, ground, seed+1000*k+607)
    return out


def footprints(spec):
    """(x, z, radius) circles round each beast, to keep spawns and loose
    rock off them."""
    out = []
    for c in spec.get('centipedes', []):
        q = np.asarray(c['points'], np.float64)
        d = np.r_[0, np.cumsum(np.linalg.norm(np.diff(q, axis=0), axis=1))]
        for s in np.arange(0, d[-1], 10.0):
            out.append((float(np.interp(s, d, q[:, 0])), float(np.interp(s, d, q[:, 1])), c['reach']+4.0))
    for s in spec.get('stalkers', []):
        out.append((s['at'][0], s['at'][1], 33.0*s['scale']))
    for r in spec.get('ribcages', []):
        out.append((r['at'][0], r['at'][1], r['length']/2+r['half']))
    return out


def rock_arches(spec, ground, seed):
    """Natural rock arches in the field: [x, z, bearing, span, rise]."""
    rng = np.random.default_rng(seed+77)
    out = []
    for x, z, bearing, span, rise in spec:
        t = math.radians(bearing)
        u = np.array([math.cos(t), math.sin(t)])
        a, b = np.array([x, z])-u*span/2, np.array([x, z])+u*span/2
        ya, yb = ground(*a), ground(*b)
        top = max(ya, yb)+rise
        pts, rad = [], []
        for k in range(11):
            s = k/10
            p = a+(b-a)*s
            y = (ya*(1-s)+yb*s)*(1-math.sin(math.pi*s))+top*math.sin(math.pi*s)
            pts.append((p[0], y-1.0, p[1])); rad.append(span*.11-span*.04*math.sin(math.pi*s))
        out.append(A.tube(pts, rad, sides=9, seed=int(rng.integers(1 << 30)), rough=.24, material='bark'))
    return out
