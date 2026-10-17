#!/usr/bin/env python3
"""Test the tower-complex asset builder."""
import importlib.util
import math
from pathlib import Path
import unittest
from assets import budgets
from assets import tower_complex
from assets import tower_complex_materials

loader=importlib.util.spec_from_file_location('kit',Path(__file__).with_name('build-original-map.py'))
kit=importlib.util.module_from_spec(loader);loader.loader.exec_module(kit)


def floor_heights_at(collision, x, z):
    """All triangle intersections of a vertical ray at (x,z) against the
    collision soup, sorted low to high."""
    import numpy as np
    triangles = np.array(collision).reshape(-1,3,3)
    heights=[]
    for a,b,c in triangles:
        mat=np.array([[b[0]-a[0],c[0]-a[0]],[b[2]-a[2],c[2]-a[2]]])
        if abs(np.linalg.det(mat))<1e-8: continue
        u,v=np.linalg.solve(mat,[x-a[0],z-a[2]])
        if u>=-1e-6 and v>=-1e-6 and u+v<=1+1e-6:
            heights.append(a[1]+u*(b[1]-a[1])+v*(c[1]-a[1]))
    return sorted(heights)


class TowerComplexTests(unittest.TestCase):
    def test_level_floors_and_headroom_away_from_shaft(self):
        mesh=kit.Mesh(); tower_complex.build(mesh,0,'one')
        # Sample points on the east lane / balcony, clear of the atrium void,
        # the tube hole and both ramps.
        for y in (tower_complex.L1, tower_complex.L2, tower_complex.L3):
            for x,z in [(9.5,4),(9.5,-4)]:
                heights=floor_heights_at(mesh.collision,x,z)
                self.assertTrue(any(abs(h-y)<.6 for h in heights),(x,z,y,'floor'))
                self.assertFalse(any(y+.6<h<y+2.5 for h in heights),(x,z,y,'headroom'))

    def test_ramps_keep_headroom_up_to_the_next_floor(self):
        mesh=kit.Mesh(); tower_complex.build(mesh,0,'one')
        tc=tower_complex
        # A body is 2.56 m tall; ramps keep that clear all the way up.
        for z in range(-10,11):
            y=tc.ramp1_height(z); heights=floor_heights_at(mesh.collision,tc.RAMP1['x'],z)
            self.assertTrue(any(abs(h-y)<.05 for h in heights),(z,'ramp 1 surface'))
            self.assertFalse(any(y+.2<h<y+2.6 for h in heights),(z,y,'ramp 1 headroom'))
        for x in range(-10,11):
            y=tc.ramp2_height(x); heights=floor_heights_at(mesh.collision,x,tc.RAMP2['z'])
            self.assertTrue(any(abs(h-y)<.05 for h in heights),(x,'ramp 2 surface'))
            self.assertFalse(any(y+.2<h<y+2.6 for h in heights),(x,y,'ramp 2 headroom'))

    def test_shaft_drops_to_the_keel_with_one_open_face_per_level(self):
        import numpy as np
        mesh=kit.Mesh(); tower_complex.build(mesh,0,'one')
        heights=floor_heights_at(mesh.collision,0,0)
        self.assertTrue(any(abs(h-tower_complex.KEEL)<.6 for h in heights),'tube needs its keel-level floor')
        for y in (tower_complex.L1, tower_complex.L2, tower_complex.L3):
            self.assertFalse(any(abs(h-y)<.6 for h in heights),(y,'shaft should be open'))
        tris=np.array(mesh.collision).reshape(-1,3,3)
        def blocked(a,b):
            a,b=np.array(a,float),np.array(b,float); d=b-a
            for p0,p1,p2 in tris:
                e1,e2=p1-p0,p2-p0; h=np.cross(d,e2); det=e1@h
                if abs(det)<1e-9: continue
                s=a-p0; u=(s@h)/det; q=np.cross(s,e1); v=(d@q)/det; t=(e2@q)/det
                if u>=0 and v>=0 and u+v<=1 and 0<=t<=1: return True
            return False
        faces={'-z':(0,-5),'+z':(0,5),'-x':(-5,0),'+x':(5,0)}
        for side,ya,yb in tower_complex.SHAFT_OPENINGS:
            level=ya
            for other,(fx,fz) in faces.items():
                ray=blocked((0,level+1.5,0),(fx,level+1.5,fz))
                self.assertEqual(ray, other!=side, (level,other,'open face' if other==side else 'closed face'))

    def test_only_uses_valid_kit_materials(self):
        mesh=kit.Mesh(); tower_complex.build(mesh,0,'one')
        # Mesh.triangle appends pos3, normal3, uv2, 0, 0, material_index, -1
        # per vertex (12 floats); material index is the 11th value (offset 10).
        vstride=12
        indices={int(mesh.vertices[i+10]) for i in range(0,len(mesh.vertices),vstride)}
        self.assertTrue(indices.issubset(set(range(len(kit.MATERIALS)))))

    def test_no_overlapping_coplanar_floor_or_roof_plates(self):
        class RecordingMesh(kit.Mesh):
            def __init__(self):
                super().__init__(); self.floor_rects=[]
            def box(self,p,size,mat='concrete',solid=True):
                tc=tower_complex
                floor_ys={tc.L1,tc.L2,tc.L3,tc.ROOF,tc.KEEL,tc.L1+tc.TUNNEL_H,tc.L1+tc.ROOM_H}
                # Only thin floor/roof plates (slab's 1 m boxes), not tall walls
                # whose top happens to land on the same level height.
                if solid and mat in ('panel','grate','concrete') and size[1]<=1.2 and any(abs(p[1]+size[1]/2-y)<1e-6 for y in floor_ys):
                    self.floor_rects.append((p,size))
                super().box(p,size,mat,solid)
        mesh=RecordingMesh(); tower_complex.build(mesh,0,'one')
        for i,(a,sa) in enumerate(mesh.floor_rects):
            for b,sb in mesh.floor_rects[i+1:]:
                if abs(a[1]-b[1])>1e-6: continue
                overlaps=all(min(a[k]+sa[k]/2,b[k]+sb[k]/2)-
                             max(a[k]-sa[k]/2,b[k]-sb[k]/2)>1e-6 for k in [0,2])
                self.assertFalse(overlaps, f'Coplanar floor overlap: {a} / {b}')

    def test_arbitrary_map_transform_and_anchors(self):
        a,b=kit.Mesh(),kit.Mesh()
        b.origin=(321,75,987); b.yaw=.73
        anchors=tower_complex.build(a,0,'one')
        moved=tower_complex.build(b,1,'two')
        self.assertEqual(anchors,moved)
        self.assertEqual(len(a.collision),len(b.collision))
        for i in range(0,len(a.collision),3):
            expected=b.point(tuple(a.collision[i:i+3]))
            for x,y in zip(expected,b.collision[i:i+3]): self.assertAlmostEqual(x,y,places=3)
        self.assertTrue(all(e['team']==1 and e['circuit']=='two' for e in b.entities))
        self.assertEqual(sum(e['kind']=='turret' for e in b.entities),2)
        self.assertEqual(sum(e['kind']=='generator' for e in b.entities),1)
        self.assertEqual(len(moved['entrances']),9)
        self.assertLess(len(b.collision)//9, budgets.COLLISION_TRIS_PER_BASE)

    def test_two_instances_have_unique_equipment_ids_and_independent_power(self):
        mesh=kit.Mesh();tower_complex.build(mesh,0,'red')
        mesh.origin=(1000,240,1400);mesh.yaw=math.pi
        tower_complex.build(mesh,1,'blue')
        ids=[e['id'] for e in mesh.entities]
        self.assertEqual(len(ids),len(set(ids)))
        self.assertEqual({e['circuit'] for e in mesh.entities},{'red','blue'})
        for team in [0,1]:
            self.assertEqual(sum(e['kind']=='generator' and e['team']==team for e in mesh.entities),1)
            self.assertEqual(sum(e['kind']=='turret' and e['team']==team for e in mesh.entities),2)

    def test_materials_are_deterministic_opaque_and_distinct(self):
        names = ['concrete', 'panel', 'grate', 'trim', 'ember', 'glacier', 'light', 'bark']
        for name in names:
            a = tower_complex_materials.texture(name, 7, kit.noise, kit.value_noise)
            b = tower_complex_materials.texture(name, 7, kit.noise, kit.value_noise)
            self.assertEqual(a, b, f'{name} texture is not deterministic')
            self.assertEqual(len(a), 256*256*4)
            self.assertTrue(all(a[i] == 255 for i in range(3, len(a), 4)), f'{name} has transparent texels')
        textures = {name: tower_complex_materials.texture(name, 7, kit.noise, kit.value_noise) for name in names}
        for i, a in enumerate(names):
            for b in names[i+1:]:
                self.assertNotEqual(textures[a], textures[b], f'{a} and {b} render identically')

    def test_every_structure_hangs_a_keel_below_its_deck(self):
        import numpy as np
        mesh=kit.Mesh(); anchors=tower_complex.build(mesh,0,'one')
        pts=np.array(mesh.collision).reshape(-1,3)
        centres={'tower':(0,0),'armory':anchors['armory'][::2],
                 'ship_platform':anchors['ship_platform'][::2],
                 'turret_left':anchors['turret_left'][::2],'turret_right':anchors['turret_right'][::2]}
        for name,(cx,cz) in centres.items():
            near=pts[(abs(pts[:,0]-cx)<6)&(abs(pts[:,2]-cz)<6)]
            self.assertLess(near[:,1].min(),-9,f'{name} has no keel under its deck')
        # Bridges and tunnels cross open air: nothing solid directly under
        # their midpoints for at least 20 m.
        for x,z in [(-9,(-tower_complex.TOWER_HALF+tower_complex.POD_Z)/2),
                    (-8,(tower_complex.TOWER_HALF+tower_complex.REAR_Z0)/2)]:
            below=[h for h in floor_heights_at(mesh.collision,x,z) if h<-1.5]
            self.assertFalse(any(h>-20 for h in below),(x,z,below))

    def test_flag_stands_on_its_plinth(self):
        mesh=kit.Mesh(); anchors=tower_complex.build(mesh,0,'one')
        fx,fy,fz=anchors['flag']
        top=tower_complex.L2+tower_complex.PLINTH
        heights=floor_heights_at(mesh.collision,fx,fz)
        self.assertTrue(any(abs(h-top)<.02 for h in heights),heights)
        self.assertAlmostEqual(fy,top+.05,places=6)
        self.assertFalse(any(top+.1<h<top+2.5 for h in heights),'flag needs headroom')

    def test_level3_windows_are_open_entries(self):
        import numpy as np
        mesh=kit.Mesh(); tower_complex.build(mesh,0,'one')
        def hits(tris,a,b):
            a,b=np.array(a,float),np.array(b,float); d=b-a; n=0
            for p0,p1,p2 in tris:
                e1,e2=p1-p0,p2-p0; h=np.cross(d,e2); det=e1@h
                if abs(det)<1e-9: continue
                s=a-p0; u=(s@h)/det; q=np.cross(s,e1); v=(d@q)/det; t=(e2@q)/det
                if u>=0 and v>=0 and u+v<=1 and 0<=t<=1: n+=1
            return n
        solid=np.array(mesh.collision).reshape(-1,3,3)
        render=np.array(mesh.vertices).reshape(-1,3,12)[:,:,:3]
        y=tower_complex.L3+2.2
        # Mid-bay on each side wall: open, so shots and players pass and
        # nothing is drawn in the way. The aperture (sill 1.1 m, head 3.9 m)
        # is taller than a 2.56 m body, so each window is a jet-in entry.
        self.assertGreater(tower_complex.HEAD-tower_complex.SILL,2.56)
        for a,b in [((1.5,y,10),(1.5,y,14)),((10,y,1.5),(14,y,1.5)),((-10,y,-1.5),(-14,y,-1.5)),((7.4,y,-10),(7.4,y,-14))]:
            for dy in (-.95,0,.95):
                a2,b2=(a[0],a[1]+dy,a[2]),(b[0],b[1]+dy,b[2])
                self.assertEqual(hits(solid,a2,b2),0,(a2,'blocked'))
            self.assertEqual(hits(render,a,b),0,(a,'view blocked'))
        # Sill and head stay solid wall.
        self.assertGreater(hits(solid,(1.5,tower_complex.L3+.2,10),(1.5,tower_complex.L3+.2,14)),0)
        self.assertGreater(hits(solid,(1.5,tower_complex.HEAD+tower_complex.L3+.4,10),(1.5,tower_complex.HEAD+tower_complex.L3+.4,14)),0)
        # The front banner strip stays solid wall.
        self.assertGreater(hits(render,(0,y,-10),(0,y,-14)),0)

    def test_keel_level_holds_the_generator_behind_a_baffled_hatch(self):
        import numpy as np
        tc=tower_complex
        mesh=kit.Mesh(); anchors=tc.build(mesh,0,'one')
        gens=[e for e in mesh.entities if e['kind']=='generator']
        self.assertEqual(len(gens),1)
        gx,gy,gz=anchors['generator']
        self.assertEqual((gx,gy,gz),tc.GENERATOR)
        self.assertLess(max(abs(gx),abs(gz))+2.5,tc.KEEL_IN)
        # Generator stands on the keel floor with the L1 slab as its ceiling.
        heights=floor_heights_at(mesh.collision,gx,gz+1.9)
        self.assertTrue(any(abs(h-tc.KEEL)<.02 for h in heights),heights)
        self.assertTrue(any(abs(h-tc.KEEL_CEILING)<.02 for h in heights),heights)
        tris=np.array(mesh.collision).reshape(-1,3,3)
        def blocked(a,b):
            a,b=np.array(a,float),np.array(b,float); d=b-a
            for p0,p1,p2 in tris:
                e1,e2=p1-p0,p2-p0; h=np.cross(d,e2); det=e1@h
                if abs(det)<1e-9: continue
                s=a-p0; u=(s@h)/det; q=np.cross(s,e1); v=(d@q)/det; t=(e2@q)/det
                if u>=0 and v>=0 and u+v<=1 and 0<=t<=1: return True
            return False
        y=tc.KEEL+1.5
        # The hatch passage is open from the ledge to the room wall...
        self.assertFalse(blocked((tc.LEDGE_OUT-.5,y,0),(tc.KEEL_IN+.2,y,0)))
        # ...but the baffle stops every straight look from the passage into the room,
        self.assertTrue(blocked((tc.HATCH_OUT,y,0),(-6,y,0)))
        for z in (-1.8,0,1.8):
            for tx,tz in ((-6,-6),(-6,6),(0,0),(-7,0)):
                self.assertTrue(blocked((tc.HATCH_OUT-.2,y,z),(tx,y,tz)),(z,tx,tz))
        # and a player can walk round it on either side.
        bx0,bx1,bz0,bz1=tc.KEEL_BAFFLE
        for s in (-1,1):
            self.assertFalse(blocked((tc.KEEL_IN-.8,y,0),(tc.KEEL_IN-.8,y,s*(bz1+1.5))))
            self.assertFalse(blocked((tc.KEEL_IN-.8,y,s*(bz1+1.5)),(bx0-1,y,s*(bz1+1.5))))
        # Every other wall of the keel room is closed at body height.
        for d in ((-1,0),(0,1),(0,-1)):
            self.assertTrue(blocked((d[0]*4,y,d[1]*4+(3.5 if d[0] else 0)),(d[0]*12,y,d[1]*12+(3.5 if d[0] else 0))),d)
        # The tube's keel face is open toward -x, its other keel faces closed.
        self.assertFalse(blocked((0,y,0),(-5,y,0)))
        for fx,fz in ((5,0),(0,5),(0,-5)): self.assertTrue(blocked((0,y,0),(fx,y,fz)))
        # Ledge: standable, with nothing solid for 12 m above it (the Level 2
        # jet ledge is 16 m up), so a jetting player flies straight in.
        lx=(tc.HATCH_OUT+tc.LEDGE_OUT)/2
        self.assertEqual([h for h in floor_heights_at(mesh.collision,lx,3.5) if tc.KEEL+.1<h<tc.KEEL+12],[])
        # Engine pods and the lower keel stay below the keel-level floor.
        inside=tris[(np.abs(tris[:,:,0]).max(1)<tc.KEEL_IN-.05)&(np.abs(tris[:,:,2]).max(1)<tc.KEEL_IN-.05)]
        low=inside[:,:,1].min(1)
        self.assertFalse(((low>tc.KEEL-.9)&(low<tc.KEEL-.05)).any(),'solid poking up through the keel floor')

    def test_atrium_is_open_from_the_atrium_floor_to_the_roof(self):
        tc=tower_complex; mesh=kit.Mesh(); tc.build(mesh,0,'one')
        vx0,vx1,vz0,vz1=tc.VOID
        for x,z in [(vx0+1.5,vz0+1.5),(vx1-1.5,vz1-1.5),(vx0+1.5,vz1-1.5),(vx1-1.5,0)]:
            heights=floor_heights_at(mesh.collision,x,z)
            # Atrium floor at L1, then nothing solid until the roof slab.
            self.assertTrue(any(abs(h-tc.L1)<.05 for h in heights),(x,z))
            self.assertFalse(any(tc.L1+2.6<h<tc.ROOF-1.01 for h in heights),(x,z,heights))
        self.assertGreaterEqual(tc.ROOF-1-tc.L1,20)
        # Balcony rings keep flyable headroom over each level.
        for level,above in ((tc.L2,tc.L3-1),(tc.L3,tc.ROOF-1)):
            self.assertGreaterEqual(above-level,7.9)

    def test_doorways_are_six_metres_wide_and_open(self):
        import numpy as np
        tc=tower_complex; mesh=kit.Mesh(); tc.build(mesh,0,'one')
        tris=np.array(mesh.collision).reshape(-1,3,3)
        def blocked(a,b):
            a,b=np.array(a,float),np.array(b,float); d=b-a
            for p0,p1,p2 in tris:
                e1,e2=p1-p0,p2-p0; h=np.cross(d,e2); det=e1@h
                if abs(det)<1e-9: continue
                s=a-p0; u=(s@h)/det; q=np.cross(s,e1); v=(d@q)/det; t=(e2@q)/det
                if u>=0 and v>=0 and u+v<=1 and 0<=t<=1: return True
            return False
        T=tc.TOWER_HALF
        # Front door and both bridge doors on L1, the two L2 side doors: 6 m
        # wide and at least 6 m tall, open straight through the wall.
        doors=[('z',-T,(-3,3),tc.L1),('z',-T,(6,12),tc.L1),('z',-T,(-12,-6),tc.L1),
               ('x',-T,(-3,3),tc.L2),('x',T,(-3,3),tc.L2)]
        for axis,plane,(u0,u1),y0 in doors:
            self.assertGreaterEqual(u1-u0,6)
            for u in (u0+.6,(u0+u1)/2,u1-.6):
                for h in (.5,3,6):
                    a=(u,y0+h,plane-1.5) if axis=='z' else (plane-1.5,y0+h,u)
                    b=(u,y0+h,plane+1.5) if axis=='z' else (plane+1.5,y0+h,u)
                    self.assertFalse(blocked(a,b),(axis,plane,u,h))

    def test_no_z_fighting_surfaces(self):
        from assets import surface_checks, cnh_tower
        mesh=kit.Mesh(); tower_complex.build(mesh,0,'one')
        self.assertEqual(surface_checks.z_fighting(mesh.vertices,mesh.collision),[])
        mesh=kit.Mesh(); cnh_tower.build(mesh)
        self.assertEqual(surface_checks.z_fighting(mesh.vertices,mesh.collision),[])

    def test_collision_budget(self):
        mesh=kit.Mesh(); tower_complex.build(mesh,0,'one')
        self.assertLess(len(mesh.collision)//9, budgets.COLLISION_TRIS_PER_BASE)

    def test_lightmap_bake_is_deterministic_valid_and_shadows(self):
        import numpy as np
        from assets import lightmap_bake
        m=kit.Mesh(); m.lamps=[]
        # Two floor tiles: one under a roof slab, one in the open; a lamp strip.
        m.quad((-2,0,-2),(2,0,-2),(2,0,2),(-2,0,2),'panel')
        m.quad((13,0,-2),(17,0,-2),(17,0,2),(13,0,2),'panel')
        m.box((0,2.6,0),(9,.4,9),'concrete')
        m.box((0,4,12),(.4,.1,3),'light',False)
        verts=np.array(m.vertices,np.float32).reshape(-1,12)
        sun=(-.57735,.57735,-.57735); light=kit.MATERIALS.index('light'); base=20
        a,pa=lightmap_bake.bake(verts,m.lamps,sun,light,base,processes=1)
        b,pb=lightmap_bake.bake(verts,m.lamps,sun,light,base,processes=3)
        self.assertTrue(np.array_equal(a,b)); self.assertEqual(pa,pb)
        self.assertTrue(np.all((a[:,11]>=base)&(a[:,11]<base+len(pa))))
        self.assertTrue(np.all((a[:,8]>=0)&(a[:,8]<=1)&(a[:,9]>=0)&(a[:,9]<=1)))
        self.assertTrue(np.array_equal(a[:,:8],verts[:,:8]))   # geometry untouched
        def mean(tri):
            uv=a[tri*3:tri*3+6,8:10]; lay=int(a[tri*3,11])-base
            x0,y0=(uv.min(0)*256).astype(int); x1,y1=np.ceil(uv.max(0)*256).astype(int)
            page=np.frombuffer(pa[lay],np.uint8).reshape(256,256,4)
            return page[y0:y1,x0:x1,:3].mean()
        self.assertLess(mean(0),.7*mean(2),'shadowed floor should bake darker')
        glow=a[verts[:,10]==light]
        self.assertTrue(np.all(glow[:,11]==base))
        page=np.frombuffer(pa[0],np.uint8).reshape(256,256,4)
        self.assertTrue(np.all(page[:4,:4,:3]==255))

    def _build_module(self):
        loader=importlib.util.spec_from_file_location('tc_build',Path(__file__).with_name('build-tower-complex.py'))
        build=importlib.util.module_from_spec(loader);loader.loader.exec_module(build)
        import json
        spec=json.loads((Path(__file__).parent.parent/'maps/tower-complex.json').read_text())
        return build,spec

    def test_terrain_clears_every_hull_and_rolls_between_bases(self):
        import numpy as np
        from assets import tower_complex_terrain as terrain
        build,spec=self._build_module()
        grid=build.terrain_grid(spec)
        self.assertTrue(np.array_equal(grid,build.terrain_grid(spec)),'terrain must be deterministic')
        for base in spec['bases']:
            bx,by,bz=base['position']
            # Deepest keel tip is 34 m below the deck; keep 20 m of air under it
            # across the whole complex (tower, pods 30 m out, rear rooms 36 m).
            for dx in range(-16,17,4):
                for dz in range(-40,41,4):
                    self.assertLess(build.terrain_height(bx+dx,bz+dz,grid),by-54,(base['team'],dx,dz))
        # Flow: rolling ground, not a flat floor with mounds (compare the
        # original v4 canyon: 30% of cells under 3 degrees, median 5 degrees).
        play=grid[40:216,40:216]
        slope=terrain.slope_degrees(grid)[40:216,40:216]
        self.assertLess((slope<3).mean(),.08)
        self.assertTrue(12<np.median(slope)<26,np.median(slope))
        self.assertGreater(np.percentile(slope,90),28)
        self.assertGreater(np.ptp(play),120)
        # The straight line between flags dips into a midfield basin.
        (rx,_,rz),(bx,_,bz)=[b['position'] for b in spec['bases']]
        profile=[build.terrain_height(rx+(bx-rx)*t,rz+(bz-rz)*t,grid) for t in np.linspace(0,1,41)]
        self.assertGreater(min(profile[0],profile[-1])-min(profile),25)
        # "An actual terrain": continuous hills, not scattered spikes. Count
        # cells higher than all 8 neighbours, and those standing more than
        # 20 m over the median of the 80 m round them (the v6 peaks had 8).
        from numpy.lib.stride_tricks import sliding_window_view as window
        c=grid[1:-1,1:-1]
        nb=np.stack([grid[1+dz:255+dz,1+dx:255+dx] for dz in (-1,0,1) for dx in (-1,0,1) if dz or dx])
        maxima=c>nb.max(0)
        med=np.median(window(np.pad(grid,5,mode='edge'),(11,11)).reshape(256,256,-1),axis=2)[1:-1,1:-1]
        self.assertLess(int(maxima.sum()),160)
        self.assertEqual(int((maxima&(c-med>20)).sum()),0)

    def test_standing_support_is_the_floor_top_everywhere(self):
        """The engine finds a standing player's floor with a ray from 0.15 m
        above the feet. Wherever a player can stand, that ray's first hit must
        be the up-facing floor top, not a slab underside or a coplanar
        down-facing face (which leaves the player airborne and frictionless)."""
        import numpy as np
        mesh=kit.Mesh(); anchors=tower_complex.build(mesh,0,'one')
        tris=np.array(mesh.collision).reshape(-1,3,3)
        a,b,c=tris[:,0],tris[:,1],tris[:,2]
        n=np.cross(b-a,c-a); n/=np.linalg.norm(n,axis=1,keepdims=True)+1e-12
        def down_hits(x,z,y0):
            # Vertical ray straight down from (x,y0,z): (height, normal.y) sorted high to low.
            e1,e2=b-a,c-a; d=np.array([0.,-1.,0.])
            h=np.cross(d,e2); det=np.einsum('ij,ij->i',e1,h)
            ok=np.abs(det)>1e-9; s=np.array([x,y0,z])-a
            u=np.einsum('ij,ij->i',s,h)/np.where(ok,det,1)
            q=np.cross(s,e1); v=(q@d)/np.where(ok,det,1); t=np.einsum('ij,ij->i',e2,q)/np.where(ok,det,1)
            m=ok&(u>=-1e-6)&(v>=-1e-6)&(u+v<=1+1e-6)&(t>=0)
            order=np.argsort(t[m]); return list(zip((y0-t[m])[order],n[m][order,1]))
        checked=0
        regions=[(-11.4,11.4,-11.4,11.4,y) for y in (tower_complex.L1,tower_complex.L2,tower_complex.L3)]
        regions+=[(-12.4,-3.6,26.6,35.4,0),(3.6,12.4,26.6,35.4,0),(-11.4,-4.6,12.2,25.8,0),(4.6,11.4,12.2,25.8,0)]
        L2,T,LE=tower_complex.L2,tower_complex.TOWER_HALF,tower_complex.LEDGE
        regions+=[(T+.4,T+LE-.4,-3.1,3.1,L2),(-T-LE+.4,-T-.4,-3.1,3.1,L2)]
        K,KEEL=tower_complex.KEEL_IN,tower_complex.KEEL
        regions+=[(-K+.3,K-.3,-K+.3,K-.3,KEEL),(K+.7,tower_complex.LEDGE_OUT-.3,-1.9,1.9,KEEL),
                  (tower_complex.HATCH_OUT+.3,tower_complex.LEDGE_OUT-.3,-4.2,4.2,KEEL)]
        for x0,x1,z0,z1,level in regions:
            for x in np.arange(x0,x1+.01,.8):
                for z in np.arange(z0,z1+.01,.8):
                    above=down_hits(x,z,level+2.6)
                    # Only cells whose top surface is this level's floor with
                    # standing room (no solid between floor+0.2 and floor+2.4).
                    # Skip cells inside a solid resting on the floor: its
                    # bottom face ties with the floor top at the same height.
                    if not above or abs(above[0][0]-level)>.02 or any(
                        hy<.99 for hh,hy in above if abs(hh-above[0][0])<.01): continue
                    first=down_hits(x,z,level+.15)
                    checked+=1
                    self.assertTrue(first and abs(first[0][0]-level)<.02 and first[0][1]>.99,
                                    (x,z,level,first[:2]))
        self.assertGreater(checked,600)
        cen=tris.mean(axis=1)
        # No accidental slopes on the floors: tilted walkable solid faces sit
        # only on the ramps, the flag plinth chamfer and kit equipment.
        ents=[e['position'] for e in mesh.entities]
        for i in np.where((n[:,1]>.3)&(n[:,1]<.9999))[0]:
            x,y,z=cen[i]
            if y>tower_complex.ROOF+.5: continue   # exterior massing
            in_keel_level=((tower_complex.KEEL-.5<=y<=tower_complex.KEEL_CEILING and abs(x)<=K and abs(z)<=K)
                           or (tower_complex.KEEL-.5<=y<=tower_complex.KEEL+.5 and K<x<=tower_complex.LEDGE_OUT
                               and abs(z)<=tower_complex.HATCH_HALF+2.3))
            if y<tower_complex.L1-.5 and not in_keel_level: continue   # keels, engine pods
            if abs(x)>tower_complex.TOWER_HALF+.1 and y<tower_complex.L2-.9: continue   # Level 2 ledge keels
            if abs(x-tower_complex.RAMP1['x'])<=2.01 and abs(z)<=tower_complex.TOWER_HALF and y<=tower_complex.L2+.01: continue  # ramps
            if abs(z-tower_complex.RAMP2['z'])<=2.01 and tower_complex.L2-.01<=y<=tower_complex.L3+.01: continue
            if math.hypot(x-tower_complex.FLAG[0],z-tower_complex.FLAG[1])<1.8 and abs(y-tower_complex.L2-.15)<.3: continue
            if any(math.hypot(x-e[0],z-e[2])<3.3 for e in ents): continue
            self.fail(('accidental slope',round(float(n[i,1]),4),cen[i]))

    def test_spawn_points_stand_on_floor_with_room_and_face_open_space(self):
        import numpy as np
        mesh=kit.Mesh(); anchors=tower_complex.build(mesh,0,'one')
        points=anchors['spawn_points']
        self.assertGreaterEqual(len(points),6)
        self.assertEqual(tuple(anchors['spawn']),tuple(points[0][:3]))
        levels={round(p[1]-tower_complex.SPAWN_LIFT) for p in points}
        # Level 3's windows are open, so its spawns moved down; the rest
        # stay off lines through the open openings (see spawn_checks.exposed).
        self.assertEqual(levels,{tower_complex.L1,tower_complex.L2})
        tris=np.array(mesh.collision).reshape(-1,3,3)
        def first_hit(o,d):
            o,d=np.array(o,float),np.array(d,float); a,b,c=tris[:,0],tris[:,1],tris[:,2]
            e1,e2=b-a,c-a; h=np.cross(d,e2); det=np.einsum('ij,ij->i',e1,h)
            ok=np.abs(det)>1e-9; s=o-a; det=np.where(ok,det,1)
            u=np.einsum('ij,ij->i',s,h)/det; q=np.cross(s,e1); v=(q@d)/det; t=np.einsum('ij,ij->i',e2,q)/det
            m=ok&(u>=0)&(v>=0)&(u+v<=1)&(t>=0)
            return t[m].min() if m.any() else np.inf
        for x,y,z,yaw in points:
            floor=y-tower_complex.SPAWN_LIFT
            self.assertAlmostEqual(y-first_hit((x,y-.37,z),(0,-1,0))-.37,floor,places=3)
            self.assertGreater(first_hit((x,floor+.2,z),(0,1,0)),2.4,(x,z,'headroom'))
            for ang in np.linspace(0,2*np.pi,8,endpoint=False):
                for h in (.3,1.,1.8):
                    self.assertGreater(first_hit((x,floor+h,z),(np.cos(ang),0,np.sin(ang))),.7,(x,z,h,'clearance'))
            facing=(-math.sin(yaw),0,-math.cos(yaw))
            self.assertGreater(first_hit((x,floor+1.6,z),facing),3.,(x,z,'faces a wall'))

    def test_low_space_under_ramps_is_closed(self):
        import numpy as np
        mesh=kit.Mesh(); tower_complex.build(mesh,0,'one')
        tris=np.array(mesh.collision).reshape(-1,3,3)
        def first(o,d):
            o,d=np.array(o,float),np.array(d,float); a,b,c=tris[:,0],tris[:,1],tris[:,2]
            e1,e2=b-a,c-a; h=np.cross(d,e2); det=np.einsum('ij,ij->i',e1,h)
            ok=np.abs(det)>1e-9; s=o-a; det=np.where(ok,det,1)
            u=np.einsum('ij,ij->i',s,h)/det; q=np.cross(s,e1); v=(q@d)/det; t=np.einsum('ij,ij->i',e2,q)/det
            m=ok&(u>=0)&(v>=0)&(u+v<=1)&(t>=0)
            return o+d*t[m].min() if m.any() else None
        tc=tower_complex; tested=0
        r1,r2=tc.RAMP1,tc.RAMP2
        edge1=r1['x']+r1['width']/2; edge2=r2['z']+r2['width']/2
        for h in (.3,1.,1.8):
            # Under the L1->L2 ramp's low end: a probe from the room side toward
            # the wall stops at the skirt on the ramp's room edge.
            for z in np.arange(r1['z0']+.3,r1['z0']+(tc.RAMP_HEADROOM+.6)*(r1['z1']-r1['z0'])/(tc.L2-tc.L1),.5):
                if h>=tc.ramp1_height(z)-.6-tc.L1: continue
                p=first((edge1+2.5,tc.L1+h,z),(-1,0,0)); self.assertIsNotNone(p)
                self.assertGreater(p[0],edge1-.06,(z,h)); tested+=1
            # Under the L2->L3 ramp's low end, probing toward the front wall.
            for x in np.arange(r2['x0']-.3,r2['x0']-(tc.RAMP_HEADROOM+.6)*(r2['x0']-r2['x1'])/(tc.L3-tc.L2),-.5):
                if h>=tc.ramp2_height(x)-.6-tc.L2: continue
                p=first((x,tc.L2+h,edge2+2.5),(0,0,-1)); self.assertIsNotNone(p)
                self.assertGreater(p[2],edge2-.06,(x,h)); tested+=1
        self.assertGreater(tested,20)

    def _pad(self):
        import numpy as np
        from assets import landing_pad
        mesh=kit.Mesh(); anchors=landing_pad.build(mesh,0)
        tris=np.array(mesh.collision).reshape(-1,3,3)
        return landing_pad,mesh,anchors,tris

    @staticmethod
    def _down(tris,x,z,y0):
        """(height, normal.y) of every solid hit straight down from (x,y0,z), nearest first."""
        import numpy as np
        a,b,c=tris[:,0],tris[:,1],tris[:,2]; e1,e2=b-a,c-a
        n=np.cross(e1,e2); n/=np.linalg.norm(n,axis=1,keepdims=True)+1e-12
        d=np.array([0.,-1.,0.]); h=np.cross(d,e2); det=np.einsum('ij,ij->i',e1,h)
        ok=np.abs(det)>1e-9; det=np.where(ok,det,1); s=np.array([x,y0,z])-a
        u=np.einsum('ij,ij->i',s,h)/det; q=np.cross(s,e1); v=(q@d)/det; t=np.einsum('ij,ij->i',e2,q)/det
        m=ok&(u>=-1e-6)&(v>=-1e-6)&(u+v<=1+1e-6)&(t>=0)
        order=np.argsort(t[m]); return list(zip((y0-t[m])[order],n[m][order,1]))

    def test_landing_pad_deck_is_flat_standable_with_open_sky(self):
        import numpy as np
        pad,mesh,anchors,tris=self._pad()
        inner=pad.HALF-pad.LIP_T-.3
        checked=0
        for x in np.arange(-inner,inner+.01,.75):
            for z in np.arange(-inner,inner+.01,.75):
                # The support ray from 0.15 m above the feet meets the deck top.
                first=self._down(tris,x,z,.15)
                self.assertTrue(first and abs(first[0][0])<1e-4 and first[0][1]>.9999,(x,z,first[:2]))
                # Nothing solid over the deck: open landing from above.
                self.assertEqual([hh for hh,_ in self._down(tris,x,z,40.) if hh>1e-3],[],(x,z))
                checked+=1
        self.assertGreaterEqual(checked,900)
        # A heavy (same 0.52 m body as every armor) has room to spare.
        self.assertGreaterEqual(2*inner,20)
        # No accidental slopes on or above the deck; tilted solids are underside only.
        a,b,c=tris[:,0],tris[:,1],tris[:,2]
        n=np.cross(b-a,c-a); n/=np.linalg.norm(n,axis=1,keepdims=True)+1e-12
        tilted=(n[:,1]>.3)&(n[:,1]<.9999)
        self.assertTrue((tris[tilted][:,:,1].max(axis=1)<=-.99).all())

    def test_landing_pad_deploy_slots_edges_and_keel(self):
        pad,mesh,anchors,tris=self._pad()
        slots=anchors['deploy_slots']
        self.assertGreaterEqual(len(slots),3)
        for x,y,z in slots:
            self.assertEqual(y,0)
            self.assertLess(max(abs(x),abs(z))+pad.SLOT_HALF,pad.HALF-pad.LIP_T)
            self.assertAlmostEqual(self._down(tris,x,z,.15)[0][0],0,places=4)
        for (ax,_,az),(bx,_,bz) in [(a,b) for i,a in enumerate(slots) for b in slots[i+1:]]:
            self.assertGreater(math.hypot(ax-bx,az-bz),2*pad.SLOT_HALF+2)
        # Every side's lip has a walk-off gap at its midpoint and is solid either side of it.
        for gx,_,gz in anchors['edge_gaps']:
            ox,oz=gx*.97,gz*.97
            self.assertEqual([h for h,_ in self._down(tris,ox,oz,1.) if h>1e-3],[],(gx,gz))
            sx,sz=(ox+(pad.GAP+1)*(gz!=0),oz+(pad.GAP+1)*(gx!=0))
            self.assertAlmostEqual(self._down(tris,sx,sz,1.)[0][0],pad.LIP,places=4)
        self.assertEqual(anchors['keel_tip'][1],-1-pad.KEEL)
        self.assertEqual(mesh.entities,[])
        self.assertLess(len(mesh.collision)//9,400)

    def test_pads_float_over_terrain_one_per_team_half(self):
        import json, numpy as np
        build,spec=self._build_module()
        from assets import landing_pad, tower_complex_terrain as terrain
        grid=build.terrain_grid(spec)
        pads=spec['pads']
        self.assertEqual(sorted(p['team'] for p in pads),[0,1])
        (rx,_,rz),(bx,_,bz)=[b['position'] for b in spec['bases']]
        mid=((rx+bx)/2,(rz+bz)/2)
        for p in pads:
            x,y,z=p['position']; home=spec['bases'][p['team']]['position']
            # On its own team's half: nearer its own base than the enemy's.
            other=spec['bases'][1-p['team']]['position']
            self.assertLess(math.hypot(x-home[0],z-home[2]),math.hypot(x-other[0],z-other[2]))
            for dx in range(-14,15,2):
                for dz in range(-14,15,2):
                    self.assertLess(build.terrain_height(x+dx,z+dz,grid),y-landing_pad.KEEL-8,(p['team'],dx,dz))
            self.assertLessEqual(y-build.terrain_height(x,z,grid),80,'reachable by jetting from the ground')

class CaptureTowers(unittest.TestCase):
    """Capture & Hold towers: three, mirrored, standing on a walkable plateau
    with no hovering edge anywhere their solids meet the ground."""
    def test_towers_sit_on_levelled_ground(self):
        import json, numpy as np
        from assets import cnh_tower, tower_complex_terrain as terrain
        loader=importlib.util.spec_from_file_location('tc_build',Path(__file__).with_name('build-tower-complex.py'))
        build=importlib.util.module_from_spec(loader);loader.loader.exec_module(build)
        root=Path(__file__).resolve().parent.parent
        spec=json.loads((root/'maps/tower-complex.json').read_text())
        grid=build.terrain_grid(spec)
        points=spec['control_points']
        self.assertEqual(len(points),3)
        centre=(1024,1024)
        self.assertEqual((points[0]['x'],points[0]['z']),centre)
        (ax,az),(bx,bz)=[(p['x'],p['z']) for p in points[1:]]
        self.assertEqual((ax+bx,az+bz),(2*centre[0],2*centre[1]),'side towers mirror through the centre')
        for p in points:
            x,z=p['x'],p['z']; ground=round(build.terrain_height(x,z,grid),3)
            # Plinth and cover walls: terrain lies between their buried base and their top.
            outline=[(math.cos(a)*(cnh_tower.PLINTH_R+.4),math.sin(a)*(cnh_tower.PLINTH_R+.4)) for a in np.linspace(0,2*math.pi,16,endpoint=False)]
            for k in range(4):
                a=math.pi/4+k*math.pi/2
                for off in (-cnh_tower.COVER_LEN/2,0,cnh_tower.COVER_LEN/2):
                    outline.append((math.cos(a)*cnh_tower.COVER_R-math.sin(a)*off,math.sin(a)*cnh_tower.COVER_R+math.cos(a)*off))
            for dx,dz in outline:
                h=build.terrain_height(x+dx,z+dz,grid)
                self.assertGreater(h,ground-cnh_tower.SINK+.2,(p['id'],dx,dz,'base shows above ground'))
                self.assertLess(h,ground+cnh_tower.PLINTH_H-.05,(p['id'],dx,dz,'buried'))
            # The ring is walkable: gentle ground everywhere inside it.
            slope=terrain.slope_degrees(grid)
            ix,iz=int(x/8),int(z/8)
            self.assertLess(slope[iz-1:iz+3,ix-1:ix+3].max(),14,p['id'])

    def test_committed_pack_declares_the_points(self):
        import json
        root=Path(__file__).resolve().parent.parent
        m=json.loads((root/'assets/maps/tower-complex/map.json').read_text())
        pts=m['control_points']
        self.assertEqual([p['id'] for p in pts],['summit','westfall','eastfall'])
        self.assertTrue(all(p['radius']==12 for p in pts))
        # The Summit is the CTF centre, its plasma turret on the tower's crown.
        self.assertEqual([p['ctf_active'] for p in pts],[True,False,False])
        from assets import centre_turret
        summit=pts[0]
        self.assertEqual(summit['turret'],{'deployable':'plasma-turret',
            'at':[summit['pos'][0],round(summit['pos'][1]+centre_turret.TURRET_Y,3),summit['pos'][2]],
            'range':centre_turret.half_to_base(summit['pos'],m['entities'])})
        self.assertTrue(all('turret' not in p for p in pts[1:]))
        self.assertIn('sky',m['look'])


class RouteCounts(unittest.TestCase):
    """Ways in on the committed pack (assets/route_checks.py, airborne model:
    open-sky decks, drops and short jet hops). docs/map-pipeline.md asks for
    at least three into the main floors, two to the flag level and exactly two
    into the generator room. Both teams."""
    def test_committed_pack_entry_counts(self):
        import json
        from assets import route_checks
        tc=tower_complex
        root=Path(__file__).resolve().parent.parent
        pack=route_checks.Pack(root/'assets/maps/tower-complex')
        spec=json.loads((root/'maps/tower-complex.json').read_text())
        K=tc.KEEL_IN
        local={'main floors':(-11.6,11.6,tc.L1-.5,tc.L3+.5,-11.6,11.6),
               'flag level':(-11.6,11.6,tc.L2-.5,tc.L2+.5,-11.6,11.6),
               'generator':(-K,K,tc.KEEL-.5,tc.KEEL+.5,-K,K)}
        for base in spec['bases']:
            ox,oy,oz=base['position']; s=-1 if base['yaw']==180 else 1
            def world(b):
                x0,x1,y0,y1,z0,z1=b
                xs=sorted((ox+s*x0,ox+s*x1)); zs=sorted((oz+s*z0,oz+s*z1))
                return (xs[0],xs[1],oy+y0,oy+y1,zs[0],zs[1])
            found=route_checks.base_entries(pack,(ox,oz),{k:world(v) for k,v in local.items()},airborne=True)
            n={k:len(v) for k,v in found.items()}
            self.assertGreaterEqual(n['main floors'],6,(base['team'],n))
            self.assertGreaterEqual(n['flag level'],4,(base['team'],n))
            self.assertEqual(n['generator'],2,(base['team'],n))

    def test_many_distinct_routes_to_each_flag(self):
        """User rule: "two main entrances but ~10 ways of getting to the flag".
        route_checks.flag_routes counts (entry, approach) pairs: every way from
        the field into the tower (Level 1 to Level 3) times every way from that
        entry into the flag's stretch of the rear balcony."""
        import json
        from assets import route_checks
        tc=tower_complex
        root=Path(__file__).resolve().parent.parent
        pack=route_checks.Pack(root/'assets/maps/tower-complex')
        spec=json.loads((root/'maps/tower-complex.json').read_text())
        fx,fz=tc.FLAG
        for base in spec['bases']:
            ox,oy,oz=base['position']; s=-1 if base['yaw']==180 else 1
            def world(b):
                x0,x1,y0,y1,z0,z1=b
                xs=sorted((ox+s*x0,ox+s*x1)); zs=sorted((oz+s*z0,oz+s*z1))
                return (xs[0],xs[1],oy+y0,oy+y1,zs[0],zs[1])
            r=route_checks.flag_routes(pack,(ox,oz),world((-11.6,11.6,tc.L1-.5,tc.L3+.5,-11.6,11.6)),
                                       world((fx-5,fx+5,tc.L2-.5,tc.L2+.8,fz-2.8,11.6)),airborne=True)
            self.assertGreaterEqual(len(r['entries']),6,(base['team'],r['entries']))
            self.assertGreaterEqual(r['routes'],10,(base['team'],[len(a) for a in r['approaches']]))


class SpawnForwardClearance(unittest.TestCase):
    """Every committed spawn faces open floor: a clear body-width view for
    6 m and the same floor for a half-second walk (docs/map-pipeline.md)."""
    def test_committed_spawns_face_open_floor(self):
        from assets import spawn_checks
        pack = Path(__file__).resolve().parent.parent/'assets/maps/tower-complex'
        self.assertEqual(spawn_checks.problems(pack), [])

    def test_no_indoor_spawn_shows_through_an_opening_from_the_field(self):
        from assets import spawn_checks
        pack = Path(__file__).resolve().parent.parent/'assets/maps/tower-complex'
        self.assertEqual(spawn_checks.exposed(pack), [])


class PropsAndShade(unittest.TestCase):
    """Props and the terrain shade map on the committed pack (assets/prop_checks.py)."""
    def test_props_and_terrain_shade(self):
        from assets import prop_checks
        prop_checks.check_pack(self, Path(__file__).resolve().parent.parent/'assets/maps/tower-complex')

class Water(unittest.TestCase):
    """Mirrored ponds carved into the terrain (assets/water_bodies.py, water_checks.py)."""
    PACK = Path(__file__).resolve().parent.parent/'assets/maps/tower-complex'

    def test_ponds_are_carved_clear_and_mirrored(self):
        from assets import water_checks
        rows = water_checks.assert_ponds(self, self.PACK, 2, 2.8, 4.2)
        for r in rows: print('water', r)

    def test_legacy_plane_is_off(self):
        import json
        m = json.loads((self.PACK/'map.json').read_text())
        self.assertFalse(m['water_enabled'])
        for v in m['water_volumes']: self.assertLessEqual(len(v['polygon']), 64)

    def test_no_solid_prop_stands_in_water(self):
        import json
        from assets import water_bodies
        m = json.loads((self.PACK/'map.json').read_text())
        solid = [(b[0], b[1], b[2]) for b in m['props']['big']]+[(c[0], c[1], c[2]) for c in m['props']['cover']]
        for x, y, z in solid:
            for v in m['water_volumes']:
                self.assertFalse(water_bodies.contains(v, x, z) and y < v['surface']+1.0, (x, y, z, v['surface']))

if __name__=='__main__':unittest.main()
