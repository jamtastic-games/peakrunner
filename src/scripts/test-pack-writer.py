#!/usr/bin/env python3
"""Tests for the shared pack writer's overlap pass. Run from src/ (Python
with scripts/requirements.txt installed): scripts/test-pack-writer.py"""
import sys, unittest
from pathlib import Path
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from assets import pack_writer, surface_checks


def tri(points, material=4, lightmap=0.0):
    """One render triangle: 12 floats per vertex (pos, normal, uv, lightmap uv,
    material, light layer)."""
    out = []
    for p in points:
        out += [*p, 0, 1, 0, 0, 0, lightmap, lightmap, material, 0]
    return out


FLOOR = [(0, 0, 0), (0, 0, 4), (4, 0, 0)]       # facing up (+Y)
SMALL = [(0.5, 0, 0.5), (0.5, 0, 2), (2, 0, 0.5)]
NO_COLLISION = np.zeros(0, '<f4').tobytes()


class OverlapTests(unittest.TestCase):
    def test_the_smaller_of_an_overlapping_pair_moves_forward(self):
        v = np.array(tri(FLOOR) + tri(SMALL, lightmap=0.5), '<f4')
        self.assertEqual(len(surface_checks.overlap_pairs(v)), 1)
        out = np.frombuffer(pack_writer.resolve_overlaps(v.tobytes(), NO_COLLISION), '<f4').reshape(-1, 3, 12)
        self.assertTrue(np.allclose(out[0, :, 1], 0.0), 'the larger face stays')
        self.assertTrue(np.allclose(out[1, :, 1], pack_writer.OVERLAP_NUDGE), 'the smaller rises along its normal')
        self.assertEqual(surface_checks.overlap_pairs(out.reshape(-1)), [])
        self.assertTrue(np.array_equal(out[:, :, 3:], v.reshape(-1, 3, 12)[:, :, 3:]), 'nothing but position changes')

    def test_chained_overlaps_all_separate(self):
        tiny = [(0.7, 0, 0.7), (0.7, 0, 1.2), (1.2, 0, 0.7)]
        v = np.array(tri(FLOOR) + tri(SMALL, lightmap=0.5) + tri(tiny, lightmap=0.9), '<f4')
        out = np.frombuffer(pack_writer.resolve_overlaps(v.tobytes(), NO_COLLISION), '<f4')
        self.assertEqual(surface_checks.overlap_pairs(out), [])

    def test_a_pack_without_overlaps_is_unchanged(self):
        apart = [(10, 0, 10), (10, 0, 12), (12, 0, 10)]
        data = np.array(tri(FLOOR) + tri(apart), '<f4').tobytes()
        self.assertIs(pack_writer.resolve_overlaps(data, NO_COLLISION), data)


if __name__ == '__main__':
    unittest.main()
