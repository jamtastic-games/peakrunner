#!/usr/bin/env python3
"""Definition, provenance and compiled-content checks; standard library only."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import unittest

ROOT=Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('kit',ROOT/'scripts/build-original-map.py')
kit=importlib.util.module_from_spec(spec);spec.loader.exec_module(kit)


class OriginalMapTests(unittest.TestCase):
    def setUp(self):
        self.definition=json.loads((ROOT/'maps/old-holler.json').read_text())
        self.root=ROOT/'assets/maps/old-holler'
        self.pack=json.loads((self.root/'map.json').read_text())

    def test_definition_and_compiled_hashes(self):
        kit.validate(self.definition)
        # The shipped pack comes from build-old-holler.py (the kit layout plus
        # the cleaned base); it records its definition and module sources.
        sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
        self.assertEqual(sha(ROOT/'maps/old-holler.json'),self.pack['definition_sha256'])
        for key,path in [('base_asset_sha256','scripts/assets/old_holler_base.py'),
                         ('structures_asset_sha256','scripts/assets/old_holler_structures.py'),
                         ('material_source_sha256','scripts/assets/old_holler_materials.py')]:
            self.assertEqual(sha(ROOT/path),self.pack[key],key)
        for name,digest in self.pack['files'].items():
            self.assertEqual(hashlib.sha256((self.root/name).read_bytes()).hexdigest(),digest,name)

    def test_all_assets_have_original_source_and_placements(self):
        self.assertIn('no extracted assets',self.pack['provenance'])
        self.assertNotIn('sources',self.pack)
        used={o['asset'] for o in self.pack['instances']}|{o['kind'] for o in self.pack['entities']}
        # Shared original assets live in their own modules, not the kit catalogue.
        shared={'cnh_tower'}
        for name in shared&used:
            self.assertTrue((ROOT/f'scripts/assets/{name}.py').is_file(),name)
        self.assertEqual(used-shared,set(kit.ASSETS))
        self.assertEqual(len(self.pack['entities']),18)

    def test_heightfield_is_generated_not_imported(self):
        data=(self.root/'height.bin').read_bytes()
        self.assertEqual(len(data),256*256*2)
        for z in range(0,256,11):
            for x in range(0,256,13):
                self.assertEqual(struct.unpack_from('<H',data,(z*256+x)*2)[0],round(kit.height(x*8,z*8,self.definition)*32))

    def test_rejects_bad_custom_definitions(self):
        for alter in [lambda d:d['objects'][0].update(asset='execute'),
                      lambda d:d['bases'][0].update(yaw=45),
                      lambda d:d['scenery'].update(trees=50000),
                      lambda d:d['environment'].update(visibility=1)]:
            d=copy.deepcopy(self.definition);alter(d)
            with self.assertRaises(ValueError):kit.validate(d)

    def test_geometry_uses_finite_values_and_valid_materials(self):
        import math
        data=(self.root/'vertices.bin').read_bytes()
        self.assertEqual(len(data)%144,0)
        for v in struct.iter_unpack('<12f',data):
            self.assertTrue(all(math.isfinite(x) for x in v))
            self.assertTrue(0<=v[10]<self.pack['texture_count'])
        self.assertEqual((self.root/'textures.rgba').stat().st_size,349524*self.pack['texture_count'])


if __name__=='__main__':unittest.main()
