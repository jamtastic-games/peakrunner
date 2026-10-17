#!/usr/bin/env python3
"""PeakRunner original environment kit. No game files, downloads, or third-party
art dependencies. Deterministic standard-library geometry/material compiler.
Coordinates: meters, Y up. JSON is data, never executable code.
"""
import argparse
from array import array
import hashlib
import json
import math
from pathlib import Path
import random
import struct
import sys

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ('base', 'bridge', 'tower', 'bunker', 'landing_pad', 'inventory', 'generator', 'sensor', 'turret', 'repair', 'tree', 'rock')
MATERIALS = ('meadow', 'rock', 'soil', 'moss', 'concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light', 'bark', 'leaf', 'water')


def smooth(t):
    t = max(0., min(1., t))
    return t*t*(3-2*t)


def height(x, z, spec, flatten_objects=True):
    t = spec['terrain']
    river = 945 + 48*math.sin(x/280)
    h = t['base_height'] - t['ravine_depth']*math.exp(-((z-river)/t['ravine_width'])**2)
    h += t['ridge_height']*(0.5+0.5*math.sin(x/165+0.65*math.sin(z/210)))
    h += 12*math.sin(z/115)*math.cos(x/140)+5*math.sin((x+z)/53)
    h += 1.4*math.sin(x/18)*math.cos(z/23)
    for b in spec['bases']:
        bx, by, bz = b['position']
        blend = smooth((115-math.hypot(x-bx, z-bz))/42)
        h += (by-h)*blend
    # A deliberately authored bridge shelf.
    for edge in [835, 1045]:
        blend = smooth((55-math.hypot(x-1000, z-edge))/25)
        h += (105-h)*blend
    if flatten_objects:
        for obj in spec['objects']:
            if obj.get('grounded') and obj['asset'] in ['bunker','tower']:
                ox,_,oz=obj['position'];radius=17 if obj['asset']=='bunker' else 9
                blend=smooth((radius+10-math.hypot(x-ox,z-oz))/10)
                if blend>0:h+=(height(ox,oz,spec,False)-h)*blend
    return max(6, h)


def add(a,b): return tuple(x+y for x,y in zip(a,b))
def sub(a,b): return tuple(x-y for x,y in zip(a,b))
def mul(a,s): return tuple(x*s for x in a)
def cross(a,b): return (a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0])
def unit(v): return mul(v,1/max(1e-12,math.sqrt(sum(x*x for x in v))))


class Mesh:
    def __init__(self):
        self.vertices = array('f'); self.collision = array('f')
        self.origin = (0,0,0); self.yaw = 0; self.instances = []; self.entities = []

    def point(self,p):
        c,s = math.cos(self.yaw), math.sin(self.yaw)
        return add(self.origin,(p[0]*c+p[2]*s,p[1],-p[0]*s+p[2]*c))

    def triangle(self, pts, material, solid=True):
        pts = [self.point(p) for p in pts]
        n = unit(cross(sub(pts[1],pts[0]),sub(pts[2],pts[0])))
        axis = max(range(3), key=lambda i: abs(n[i]))
        uv_axes = [(2,1),(0,2),(0,1)][axis]
        for p in pts:
            uv = (p[uv_axes[0]]/4, p[uv_axes[1]]/4)
            self.vertices.extend((*p,*n,*uv,0,0,MATERIALS.index(material),-1))
        if solid: self.collision.extend(v for p in pts for v in p)

    def quad(self,a,b,c,d,mat,solid=True):
        self.triangle([a,b,c],mat,solid); self.triangle([a,c,d],mat,solid)

    def box(self,p,size,mat='concrete',solid=True):
        x,y,z = p; a,b,c = [v/2 for v in size]
        corners = [(x+dx*a,y+dy*b,z+dz*c) for dx,dy,dz in
                   [(-1,-1,-1),(1,-1,-1),(1,1,-1),(-1,1,-1),(-1,-1,1),(1,-1,1),(1,1,1),(-1,1,1)]]
        for face in [(0,3,2,1),(4,5,6,7),(0,4,7,3),(1,2,6,5),(0,1,5,4),(3,7,6,2)]:
            self.quad(*(corners[i] for i in face),mat,solid)

    def collision_box(self,p,size):
        """A box that collides exactly as `box` would but isn't drawn (the
        game draws its own model there)."""
        x,y,z = p; a,b,c = [v/2 for v in size]
        corners = [self.point((x+dx*a,y+dy*b,z+dz*c)) for dx,dy,dz in
                   [(-1,-1,-1),(1,-1,-1),(1,1,-1),(-1,1,-1),(-1,-1,1),(1,-1,1),(1,1,1),(-1,1,1)]]
        for face in [(0,3,2,1),(4,5,6,7),(0,4,7,3),(1,2,6,5),(0,1,5,4),(3,7,6,2)]:
            q = [corners[i] for i in face]
            for tri in ((q[0],q[1],q[2]),(q[0],q[2],q[3])):
                self.collision.extend(v for pt in tri for v in pt)

    def ramp(self,x,width,z0,z1,y0,y1,mat='grate'):
        a=(x-width/2,y0,z0); b=(x+width/2,y0,z0)
        c=(x+width/2,y1,z1); d=(x-width/2,y1,z1)
        self.quad(a,d,c,b,mat)
        # Closed slab, no invisible wedge filling the room beneath it.
        aa,bb,cc,dd=[add(p,(0,-.6,0)) for p in [a,b,c,d]]
        self.quad(aa,bb,cc,dd,'trim'); self.quad(a,aa,dd,d,'trim'); self.quad(b,c,cc,bb,'trim')

    def column(self,p,r,height,mat='trim',sides=8,top=None,solid=True):
        top = r if top is None else top
        x,y,z=p
        for i in range(sides):
            a=i*math.tau/sides; b=(i+1)*math.tau/sides
            q=(x+r*math.cos(a),y,z+r*math.sin(a)); w=(x+r*math.cos(b),y,z+r*math.sin(b))
            e=(x+top*math.cos(b),y+height,z+top*math.sin(b)); f=(x+top*math.cos(a),y+height,z+top*math.sin(a))
            self.quad(q,f,e,w,mat,solid)
            self.triangle([(x,y+height,z),e,f],mat,solid)

    def entity(self,kind,p,team,circuit,radius=3,weapon='bullet'):
        self.entities.append(dict(id=f'{kind}-{len(self.entities)}',kind=kind,position=self.point(p),
                                  team=team,circuit=circuit,radius=radius,weapon=weapon))

    def equipment(self,kind,p,team,circuit,weapon='bullet'):
        x,y,z=p; accent='ember' if team==0 else 'glacier'
        if kind=='inventory':
            self.box((x,y+.15,z),(4,.3,4),'trim')
            for dx in [-1.7,1.7]:
                self.box((x+dx,y+1.8,z+.9),(.45,3.6,.65),'panel')
                self.box((x+dx,y+2,z+.53),(.2,2.6,.05),accent,False)
            self.box((x,y+3.7,z+.9),(3.9,.5,.7),'panel')
            self.box((x,y+1.4,z+1.5),(2.8,2.8,.5),'grate')
            self.box((x,y+2.2,z+1.22),(2.1,.7,.08),'light',False)
            self.entity(kind,(x,y+1,z-1),team,circuit,3)
            # The booth opens toward local -z: players step in from there.
            c,s = math.cos(self.yaw), math.sin(self.yaw)
            self.entities[-1]['facing'] = [round(-s,4)+0.0, round(-c,4)+0.0]
        elif kind=='generator':
            self.box((x,y+.5,z),(5,1,4),'trim')
            self.column((x,y+1,z),1.35,4.3,'panel',10)
            for dy in [1.4,2.3,3.2,4.1]: self.column((x,y+dy,z),1.46,.16,accent,10,solid=False)
            for dx in [-2,2]: self.box((x+dx,y+2.7,z),(.55,4.4,3.4),'grate')
            self.box((x,y+5.5,z),(4.5,.55,3),'panel')
            self.entity(kind,(x,y+2.5,z),team,circuit,2.8)
        elif kind=='sensor':
            # Base and mast. The radar head on top is the game's own model
            # (models/equipment/sensor), turning while the sensor is powered;
            # the head's old bar still collides here, undrawn.
            self.column((x,y,z),1.6,1,'trim'); self.column((x,y+1,z),.38,6,'panel')
            self.collision_box((x,y+6.7,z),(5,1.1,.4))
            self.entity(kind,(x,y+5.8,z),team,circuit,2.8)
        elif kind=='turret':
            # Static mount: base ring, buttressed pedestal and bearing collar.
            # The head (housing, cradle, barrels) is runtime geometry so it
            # tracks the server's aim.
            self.column((x,y,z),2,1.2,'trim'); self.column((x,y+1.2,z),1.1,1.1,'panel')
            for dx,dz in [(1.25,0),(-1.25,0),(0,1.25),(0,-1.25)]:
                self.box((x+dx,y+1.6,z+dz),(.3 if dx else .9,.8,.9 if dx else .3),'trim')
            self.column((x,y+2.3,z),1.35,.3,'trim',12)
            self.column((x,y+2.33,z),1.42,.12,accent,12,solid=False)
            self.entity(kind,(x,y+2.8,z),team,circuit,2.2,weapon)
        elif kind=='repair':
            self.column((x,y,z),2.8,.25,'trim',12)
            self.box((x,y+.3,z),(3,.06,.5),'light',False)
            self.box((x,y+.3,z),(.5,.06,3),'light',False)
            self.entity(kind,(x,y+.8,z),team,circuit,3)

    def base(self,team,circuit,equipment):
        accent='ember' if team==0 else 'glacier'
        # Lower service hall and upper flag deck, connected by wide ski ramps.
        self.box((0,-10.6,0),(64,1.2,56),'grate')
        for x in [-31,31]: self.box((x,-1,0),(2,18,56),'concrete')
        self.box((0,-1,27),(64,18,2),'concrete')
        for x in [-21,21]: self.box((x,-1,-27),(22,18,2),'concrete')
        self.box((0,6,-27),(20,4,2),'panel')
        # Roof halves leave a central atrium; bridge connects flag to both wings.
        for x in [-22,22]: self.box((x,8,0),(20,1.2,56),'panel')
        self.box((0,8,12),(24,1.2,16),'panel')
        self.ramp(0,18,-52,-27,0,-10)
        for x in [-9.5,9.5]: self.box((x,-5,-39.5),(1,10,25),'concrete')
        self.ramp(-20,9,-19,19,-10,8.6)
        self.ramp(20,9,-19,19,-10,8.6)
        # Entrance retaining cheeks keep the terrain cutout deliberately bounded.
        for x in [-31,31]: self.box((x,-5,-40),(2,10,26),'concrete')
        for x in [-21,21]: self.box((x,-5,-51),(22,10,2),'concrete')
        # Broad sloped shoulders frame the approach and form alternate roof routes.
        for x in [-22,22]: self.ramp(x,18,-52,-27,0,8.6,'concrete')
        # Faceted service spire behind an exposed flag deck.
        self.column((0,8.6,14),5,1.1,'trim',8)
        self.column((0,9.7,14),4.7,.16,accent,8,solid=False)
        self.column((0,8.6,22),5.8,18,'concrete',8,top=4.4)
        self.column((0,26.6,22),4.8,1.2,'trim',8)
        self.column((0,24.7,22),4.6,.4,accent,8,solid=False)
        self.column((0,27.8,22),.5,4,'panel',6)
        for x in [-4,4]:
            self.box((x,16.5,18.8),(.6,12,.6),'trim',False)
        # Ribbed cladding, lamps, and bands give the kit a consistent vocabulary.
        for x in [-32.05,32.05]:
            for z in range(-24,26,8): self.box((x,0,z),(.15,15,.7),'trim',False)
            self.box((x,6,0),(.2,.6,53),accent,False)
        for x in [-28,28]:
            for z in [-19,0,19]: self.box((x,4,z),(.3,.5,3),'light',False)
        for item in equipment: self.equipment(item['kind'],item['position'],team,circuit,item.get('weapon','bullet'))

    def bridge(self):
        self.box((0,0,0),(13,1.4,212),'grate')
        self.ramp(0,13,-114,-106,0,.7)
        self.ramp(0,13,106,114,.7,0)
        for x in [-6.5,6.5]:
            self.box((x,.9,0),(.45,1.7,212),'trim')
            for z in range(-96,100,16): self.box((x,1.8,z),(.52,.1,2),'light',False)
        for z in [-64,64]:
            for x in [-5.5,5.5]: self.box((x,-24,z),(2.2,48,4),'concrete')
        for z in range(-88,100,16): self.box((0,-2.4,z),(14,3,1.5),'panel')

    def tower(self,team,circuit):
        self.column((0,-3,0),7,5,'concrete',8,top=5)
        self.column((0,2,0),3.5,20,'panel',8,top=2.7)
        self.column((0,22,0),7,.8,'trim',8)
        for x in [-4,4]: self.box((x,25,0),(.6,5,.6),'panel')
        self.box((0,27.5,0),(11,.6,8),'concrete')
        self.equipment('turret',(0,22.8,-1),team,circuit)
        self.equipment('sensor',(0,27.8,1),team,circuit)

    def bunker(self):
        self.box((0,0,0),(22,.8,20),'grate')
        for x in [-10,10]: self.box((x,3,0),(2,6,20),'concrete')
        self.box((0,3,9),(22,6,2),'concrete'); self.box((0,6,0),(24,1,22),'panel')
        self.ramp(0,18,-15,-10,-2,.4)
        for x in [-7,7]: self.box((x,4,-9),(.6,.3,2),'light',False)

    def tree(self,rng):
        h=rng.uniform(10,19)
        self.column((0,-1,0),.65,h,'bark',7,top=.18)
        if rng.random()<.5:
            for level in range(4):
                y=h*(.4+level*.17); radius=h*(.28-level*.035)
                self.column((0,y,0),radius,h*.32,'leaf',9,top=.15,solid=False)
        else:
            for cx,cy,cz,r in [(0,h*.88,0,h*.3),(-h*.18,h*.72,0,h*.22),(h*.17,h*.76,h*.12,h*.23)]:
                def vertex(a,b):return (cx+r*math.sin(a)*math.cos(b),cy+r*.5*math.cos(a),cz+r*math.sin(a)*math.sin(b))
                for lat in range(6):
                    for lon in range(12):
                        a,b=lat*math.pi/6,lon*math.tau/12
                        self.quad(vertex(a,b),vertex(a+math.pi/6,b),vertex(a+math.pi/6,b+math.tau/12),vertex(a,b+math.tau/12),'leaf',False)

    def landing_pad(self,team):
        self.column((0,0,0),11,.35,'trim',12)
        self.column((0,.35,0),9.5,.12,'grate',12)
        accent='ember' if team==0 else 'glacier'
        for x in [-4,4]:self.box((x,.5,0),(.5,.06,10),accent,False)
        self.box((0,.5,0),(8,.06,.5),accent,False)
        self.box((8,1.1,8),(1.6,2.2,1.2),'panel')
        self.box((8,1.8,7.37),(1.1,.5,.04),'light',False)

    def rock(self,rng):
        self.column((0,-.6,0),rng.uniform(1.8,4.8),rng.uniform(2,5),'rock',7,top=.8)


def noise(x,y,seed):
    n = (x*374761393+y*668265263+seed*1274126177)&0xffffffff
    n = ((n^(n>>13))*1274126177)&0xffffffff
    return ((n^(n>>16))&65535)/65535


def value_noise(x,y,period,seed):
    ix,iy=math.floor(x),math.floor(y);u=smooth(x-ix);v=smooth(y-iy)
    a=noise(ix%period,iy%period,seed);b=noise((ix+1)%period,iy%period,seed)
    c=noise(ix%period,(iy+1)%period,seed);d=noise((ix+1)%period,(iy+1)%period,seed)
    return (a*(1-u)+b*u)*(1-v)+(c*(1-u)+d*u)*v


def cloud_noise(x,y,z,seed):
    ix,iy,iz=math.floor(x),math.floor(y),math.floor(z)
    u,v,w=smooth(x-ix),smooth(y-iy),smooth(z-iz)
    def at(dx,dy,dz): return noise(ix+dx+(iz+dz)*331,iy+dy,seed)
    return sum(at(dx,dy,dz)*(u if dx else 1-u)*(v if dy else 1-v)*(w if dz else 1-w)
               for dx in [0,1] for dy in [0,1] for dz in [0,1])


def texture(name,seed):
    palette = {'meadow':(69,86,40),'rock':(98,102,97),'soil':(114,104,78),'moss':(49,68,37),
               'concrete':(106,116,119),'panel':(62,77,84),'grate':(48,57,62),'trim':(31,42,49),
               'ember':(186,76,31),'glacier':(29,134,169),'light':(155,219,218),'bark':(81,72,52),
               'leaf':(40,67,42),'water':(48,80,87)}
    data=bytearray()
    for y in range(256):
        for x in range(256):
            grain=(noise(x,y,seed)-.5)*32
            broad=24*(value_noise(x/32,y/32,8,seed)-.5)+12*(value_noise(x/8,y/8,32,seed+17)-.5)
            value=grain+broad
            if name in ['meadow','moss']:
                value += (noise(x//2,y//5,seed+9)-.5)*20
            if name in ['concrete','panel']:
                if x%128<2 or y%64<2: value-=29
                elif x%128<4 or y%64<4: value+=16
                if (x%128-9)**2+(y%64-9)**2<6: value-=40
            if name=='grate': value += 22 if (x+y)%16<3 or (x-y)%16<3 else -12
            if name=='bark': value += 17*math.sin(x*math.tau/16+math.sin(y*math.tau/128))
            if name in ['ember','glacier','light']: value=grain*.2+(15 if y%32<3 else 0)
            if name=='water': value=8*math.sin((x+y)*math.tau/32)+grain*.2
            data.extend(max(0,min(255,round(c+value))) for c in palette[name]); data.append(255)
    return bytes(data)


def sky(face):
    data=bytearray()
    for y in range(256):
        for x in range(256):
            u,v=x/255*2-1,y/255*2-1
            d=[(u,-v,-1),(1,-v,u),(-u,-v,1),(-1,-v,-u),(u,1,-v),(u,-1,v)][face]
            dx,dy,dz=unit(d)
            cloud = sum((cloud_noise(dx*f+11,dy*f+4,dz*f+7,32)-.5)/2**i for i,f in enumerate([4,9,21,47]))
            cloud=smooth((cloud+.17)/.42)
            horizon=smooth(dy/.4)
            for low,high in zip((158,169,175),(56,76,98)):
                c=low*(1-horizon)+high*horizon+cloud*55*horizon
                data.append(max(0,min(255,int(c))))
            data.append(255)
    return bytes(data)


def downsample(data,side):
    out=bytearray()
    for y in range(0,side,2):
        for x in range(0,side,2):
            for c in range(4): out.append(sum(data[((y+dy)*side+x+dx)*4+c] for dx,dy in [(0,0),(1,0),(0,1),(1,1)])//4)
    return bytes(out)


def base_textures(seed):
    """Every kit material plus the six sky faces, as a level-major mip chain."""
    imgs=[texture(name,seed+i) for i,name in enumerate(MATERIALS)]+[sky(i) for i in range(6)]
    texture_bytes=bytearray();side=256;level=imgs
    while True:
        texture_bytes.extend(b''.join(level))
        if side==1:break
        level=[downsample(im,side) for im in level];side//=2
    return bytes(texture_bytes)


def base_ambient(seed):
    # Original filtered-noise wind/rain, not a recording or a sustained tone.
    # Warm the filters over one identical period to make the loop continuous.
    audio_rng=random.Random(seed+391);count=44100*4
    white=[audio_rng.uniform(-1,1) for _ in range(count)]
    samples=array('f');slow=fast=0.
    for i in range(count*2):
        n=white[i%count];slow=.995*slow+.005*n;fast=.6*fast+.4*n
        if i>=count:samples.append((slow*.4+fast*.018)*(.7+.3*math.sin(math.tau*i/count)))
    if sys.byteorder!='little':samples.byteswap()
    return samples.tobytes()


def layer_manifest(water_height):
    return dict(texture_count=len(MATERIALS)+6,terrain_layers=[0,1,2,3],
                sky_layers=list(range(len(MATERIALS),len(MATERIALS)+6)),water_layer=MATERIALS.index('water'),
                water={'position':f'-384 -160 {water_height}','scale':'720 150 1'})


# The shared starting textures, ambience and layer layout that the other
# original maps paint over: the kit set generated with the first map's seed.
BASE_SEED=84271
BASE_WATER_HEIGHT=54


def base_pack():
    return dict(textures=base_textures(BASE_SEED),ambient=base_ambient(BASE_SEED),
                manifest=layer_manifest(BASE_WATER_HEIGHT))


def validate(spec):
    if spec.get('version')!=1: raise ValueError('Unsupported map definition')
    if len(spec['bases'])!=2 or sorted(b['team'] for b in spec['bases'])!=[0,1]: raise ValueError('Exactly two team bases required')
    ids=set()
    for obj in spec['bases']+spec['objects']:
        if obj['id'] in ids: raise ValueError('Duplicate object ID')
        ids.add(obj['id'])
        if len(obj['position'])!=3 or not all(math.isfinite(n) and 0<=n<=2040 for n in obj['position']): raise ValueError('Invalid position')
        if not math.isfinite(obj.get('yaw',0)): raise ValueError('Invalid yaw')
    for obj in spec['objects']:
        if obj['asset'] not in ASSETS or obj['asset']=='base': raise ValueError('Unknown world asset')
        if obj.get('team',0) not in [0,1]: raise ValueError('Invalid team')
    for b in spec['bases']:
        if b.get('yaw',0)%180 or any(b['position'][i]%8 for i in [0,2]):
            raise ValueError('Basement bases require 8m grid alignment and 0/180 degree yaw')
    for item in spec['base_equipment']:
        if item.get('weapon','bullet') not in ['bullet','plasma','rocket']: raise ValueError('Unknown turret weapon')
        if item['kind'] not in ['inventory','generator','turret','sensor','repair']:
            raise ValueError('Unknown equipment behavior')
        if len(item['position'])!=3 or not all(math.isfinite(v) and abs(v)<100 for v in item['position']):
            raise ValueError('Invalid base equipment position')
    if not any(item['kind']=='generator' for item in spec['base_equipment']): raise ValueError('Bases require a generator')
    if not 0<=spec['environment']['fog_start']<spec['environment']['visibility']<=10000: raise ValueError('Invalid visibility')
    if not 0<=spec['environment']['water_height']<500: raise ValueError('Invalid water height')
    if not all(math.isfinite(v) and 0<v<500 for v in spec['terrain'].values()): raise ValueError('Invalid terrain settings')
    if any(not 0<=spec['scenery'][k]<=500 for k in ['trees','rocks']): raise ValueError('Scenery budget exceeded')


def build(spec,output):
    validate(spec)
    if output.exists(): raise ValueError(f'Refusing to overwrite {output}; use a new output directory')
    rng=random.Random(spec['seed']); mesh=Mesh(); flags=[]; spawns=[]; holes=[]
    for b in sorted(spec['bases'],key=lambda b:b['team']):
        mesh.origin=tuple(b['position']); mesh.yaw=math.radians(b.get('yaw',0))
        mesh.base(b['team'],b['id'],spec['base_equipment'])
        flags.append(mesh.point((0,10.05,14))); spawns.append(mesh.point((48,0,-65)))
        mesh.instances.append(dict(asset='base',**b))
    for obj in spec['objects']:
        p=list(obj['position'])
        if obj.get('grounded'): p[1]=height(p[0],p[2],spec)
        mesh.origin=tuple(p); mesh.yaw=math.radians(obj.get('yaw',0))
        if obj['asset']=='tower': mesh.tower(obj.get('team',0),next(b['id'] for b in spec['bases'] if b['team']==obj.get('team',0)))
        elif obj['asset']=='landing_pad':mesh.landing_pad(obj.get('team',0))
        elif obj['asset'] in ['inventory','generator','sensor','turret','repair']:
            team=obj.get('team',0)
            mesh.equipment(obj['asset'],(0,0,0),team,obj.get('circuit',next(b['id'] for b in spec['bases'] if b['team']==team)),obj.get('weapon','bullet'))
        elif obj['asset'] in ['tree','rock']:getattr(mesh,obj['asset'])(rng)
        else: getattr(mesh,obj['asset'])()
        mesh.instances.append(dict(obj,position=p))
    for asset,count in [('tree',spec['scenery']['trees']),('rock',spec['scenery']['rocks'])]:
        placed=0
        for _ in range(count*40):
            x,z=rng.uniform(180,1860),rng.uniform(200,1840)
            y=height(x,z,spec)
            if y<65 or any(math.hypot(x-b['position'][0],z-b['position'][2])<140 for b in spec['bases']): continue
            if any(math.hypot(x-o['position'][0],z-o['position'][2])<35 for o in spec['objects']): continue
            if abs(x-1000)<35 and abs(z-940)<150: continue
            mesh.origin=(x,y,z);mesh.yaw=rng.random()*math.tau
            getattr(mesh,asset)(rng);mesh.instances.append(dict(asset=asset,position=mesh.origin,yaw=math.degrees(mesh.yaw)))
            placed+=1
            if placed==count:break
    heights=bytearray();weights=bytearray()
    for iz in range(256):
        for ix in range(256):
            x,z=ix*8,iz*8; h=height(x,z,spec)
            heights.extend(struct.pack('<H',round(h*32)))
            slope=math.hypot(height(x+2,z,spec)-height(x-2,z,spec),height(x,z+2,spec)-height(x,z-2,spec))/4
            rock=smooth((slope-.25)/.8);soil=(1-rock)*(.2+.16*math.sin(x/70)*math.cos(z/100))
            moss=(1-rock-soil)*(.2+.15*math.sin((x+z)/90));grass=1-rock-soil-moss
            weights.extend(round(w*255) for w in [grass,rock,soil,moss])
            # Cut cells touched by the basement/entry footprint, then cover
            # the snapped boundary with a generated apron at ground height.
            cx,cz=x+4,z+4
            for b in spec['bases']:
                bx,by,bz=b['position'];a=-math.radians(b.get('yaw',0))
                lx=(cx-bx)*math.cos(a)+(cz-bz)*math.sin(a);lz=-(cx-bx)*math.sin(a)+(cz-bz)*math.cos(a)
                lx,lz=round(lx,6),round(lz,6)
                if abs(lx)<36 and -56<lz<32:
                    holes.append(iz*256+ix);break
    # Fill the portions of grid-aligned holes outside each exact room with
    # original retaining/apron geometry. Bases are aligned to the 8m grid.
    for b in spec['bases']:
        mesh.origin=tuple(b['position']);mesh.yaw=math.radians(b.get('yaw',0))
        for x in [-34,34]:mesh.box((x,-.27,-12),(4,.6,88),'concrete')
        mesh.box((0,-.27,30),(64,.6,4),'concrete')
        # Entrance mouth and small shoulders around the downward ramp.
        for x in [-21,21]:mesh.box((x,-.27,-40),(22,.6,24),'concrete')
        mesh.box((0,-.27,-54),(64,.6,4),'grate')
    texture_bytes=base_textures(spec['seed']);samples=base_ambient(spec['seed'])
    if sys.byteorder!='little':mesh.vertices.byteswap();mesh.collision.byteswap()
    output.mkdir(parents=True)
    files={'height.bin':bytes(heights),'weights.rgba':bytes(weights),'vertices.bin':mesh.vertices.tobytes(),
           'collision.bin':mesh.collision.tobytes(),'textures.rgba':texture_bytes,'ambient.f32':samples}
    for name,data in files.items():(output/name).write_bytes(data)
    e=spec['environment']
    manifest=dict(version=1,name=spec['name'],provenance='PeakRunner original procedural kit v1; no extracted assets',
                  flags=flags,spawns=spawns,holes=sorted(set(holes)),**layer_manifest(e['water_height']),
                  sky={'visibleDistance':str(e['visibility']),'fogDistance':str(e['fog_start'])},
                  ambient_emitters=[[1000,100,940,.35,200,2000]],entities=mesh.entities,instances=mesh.instances,
                  materials=list(MATERIALS),asset_catalog=list(ASSETS),
                  definition_sha256=hashlib.sha256(json.dumps(spec,sort_keys=True).encode()).hexdigest(),
                  compiler_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  files={n:hashlib.sha256(d).hexdigest() for n,d in files.items()})
    (output/'map.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(f'{output}: {len(mesh.instances)} instances, {len(mesh.vertices)//36} triangles, {len(mesh.entities)} equipment objects')
    return manifest


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--definition',type=Path,default=ROOT/'maps/old-holler.json')
    parser.add_argument('--output',type=Path,default=ROOT/'assets/maps/old-holler')
    args=parser.parse_args()
    build(json.loads(args.definition.read_text()),args.output)
