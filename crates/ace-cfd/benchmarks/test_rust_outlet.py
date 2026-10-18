# SPDX-License-Identifier: AGPL-3.0-only
"""Physical invariants and fail-closed input handling for the outlet audit."""
import copy
import json
import math
from pathlib import Path
import sys
import tempfile
import unittest

from sphere_grid import aligned_case
from verify_rust_outlet import cell_moments, gauss_points, validate_case, verify_blocking, viscous_flux


class RustOutletTests(unittest.TestCase):
    def setUp(self):
        template = json.loads((Path(__file__).parents[1] / "cases/sphere.json").read_text())
        template.update(curved_geometry=True, outflow="zero_gradient_cell_flux")
        self.case = aligned_case(template, 4, 4, 1.22, 1.6, 2.0, 3.0)
        self.saved = dict(curved_geometry=True, mesh_vertices=copy.deepcopy(self.case["mesh_vertices"]),
                          solution=dict(state=[[1.0, 2.0, 3.0, 0.0, 100.0] for _ in range(16)]))
        self.gas = dict(viscosity=2.0, conductivity=7.0)

    def test_gauss_integrates_all_polynomials_through_degree_23(self):
        points = gauss_points()
        self.assertEqual(len(points), 12)
        self.assertTrue(all(0 < x < 1 and w > 0 for x, w in points))
        for degree in range(24):
            self.assertAlmostEqual(math.fsum(w * x**degree for x, w in points),
                                   1 / (degree + 1), delta=1e-14)

    def test_cell_moments_match_integrated_spherical_sector(self):
        # The last angular cell spans [pi/4, pi/2] with constant inner/outer radii.
        vertices = [[-r * math.cos(t), r * math.sin(t), 0.0]
                    for t in (0.0, math.pi / 4, math.pi / 2) for r in (1.0, 2.0, 3.0)]
        actual = cell_moments(vertices, 2, 2)
        theta, cosine = math.pi / 4, math.sqrt(0.5)
        int_sin2 = (math.pi / 2 - theta) / 2 + math.sin(2 * theta) / 4
        int_sin3 = cosine - cosine**3 / 3
        int_sin4 = 3 * (math.pi / 2 - theta) / 8 + math.sin(2 * theta) / 4 - math.sin(4 * theta) / 32
        for i, (a, b) in enumerate(((1.0, 2.0), (2.0, 3.0))):
            even_y2 = (b**5 - a**5) / 5 * int_sin3 / ((b**3 - a**3) / 3 * cosine)
            odd_x = -(b**5 - a**5) / 5 * (1 - math.sin(theta)**3) / 3 / ((b**4 - a**4) / 4 * int_sin2)
            odd_y2 = (b**6 - a**6) / 6 * int_sin4 / ((b**4 - a**4) / 4 * int_sin2)
            for measured, expected in zip(actual[2 + i], (even_y2, odd_x, odd_y2), strict=True):
                self.assertAlmostEqual(measured, expected, delta=2e-13)

    def test_projection_removes_normal_heat_but_retains_shear_and_hoop_strain(self):
        gradient = [[0.0, 0.0, 0.0] for _ in range(6)]
        gradient[1] = [11.0, 3.0, 0.0]
        gradient[2] = [13.0, 2.0, 0.0]
        gradient[5] = [17.0, 19.0, 0.0]
        velocity, center, normal = (5.0, 4.0), (0.0, 2.0, 0.0), (1.0, 0.0, 0.0)
        projected = viscous_flux(velocity, gradient, center, normal, self.gas)
        for actual, expected in zip(projected, [0.0, -16 / 3, 6.0, 0.0, -80 / 3 + 24], strict=True):
            self.assertAlmostEqual(actual, expected, delta=1e-14)
        unprojected = viscous_flux(velocity, gradient, center, normal, self.gas, False)
        self.assertEqual(unprojected, [0.0, 24.0, 32.0, 0.0, 120 + 128 + 119])

    def test_isotropic_expansion_has_no_deviatoric_stress(self):
        gradient = [[0.0, 0.0, 0.0] for _ in range(6)]
        gradient[1][0] = gradient[2][1] = 3.0
        self.assertEqual(viscous_flux((6.0, 12.0), gradient, (2.0, 4.0, 0.0),
                                     (1.0, 0.0, 0.0), self.gas, False), [0.0] * 5)

    def test_unsupported_case_is_rejected_before_output_creation(self):
        validate_case(self.case, self.saved)
        variants = []
        for key, value in (("outflow", "one_sided"), ("curved_geometry", False),
                           ("mesh_vertices", [])):
            case = copy.deepcopy(self.case)
            case[key] = value
            variants.append((case, self.saved))
        for component, value in ((3, 1.0), (4, float("nan"))):
            saved = copy.deepcopy(self.saved)
            saved["solution"]["state"][0][component] = value
            variants.append((self.case, saved))
        saved = copy.deepcopy(self.saved)
        saved["mesh_vertices"][0][0] *= 2
        variants.append((self.case, saved))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for case, saved in variants:
                source, checkpoint, output = (root / name for name in ("input.json", "saved.json", "audit"))
                source.write_text(json.dumps(case))
                checkpoint.write_text(json.dumps(saved))
                with self.assertRaises(ValueError):
                    verify_blocking(source, checkpoint, Path(sys.executable), output)
                self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
