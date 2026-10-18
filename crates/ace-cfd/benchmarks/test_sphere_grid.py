# SPDX-License-Identifier: AGPL-3.0-only
"""Check geometry, nested sampling and solver-control preservation at preparation."""
import copy
import json
import math
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from eilmer_case import prepare
from sphere_grid import aligned_case, nested_case


class SphereGridTests(unittest.TestCase):
    def setUp(self):
        self.template = json.loads((Path(__file__).parents[1] / "cases/sphere.json").read_text())
        self.template.update(curved_geometry=True, outflow="zero_gradient_cell_flux")

    def aligned(self, nr=16, nt=16, stretching=2.0):
        return aligned_case(self.template, nr, nt, 1.22, 1.6, stretching, 3.0)

    def test_wall_and_hyperbolic_guide_are_geometric_constraints(self):
        for stretching in (0.0, 2.0):
            case = self.aligned(stretching=stretching)
            points = case["mesh_vertices"]
            radius = self.template["grid"]["radius"]
            offset = 1.6 * 8
            for j in range(17):
                self.assertAlmostEqual(math.hypot(*points[j * 17]), radius)
                x, y, _ = points[j * 17 + 8]
                self.assertAlmostEqual((-x / radius - 1.22 - offset)**2
                                       - 8 * (y / radius)**2, offset**2)
            self.assertEqual(case["solver"], self.template["solver"])
            self.assertEqual(case["gas"], self.template["gas"])
            self.assertEqual(case["outflow"], self.template["outflow"])

    def test_recorded_shape_has_known_axis_spacing_and_outlet_extent(self):
        case = aligned_case(self.template, 96, 192, 1.219631340443862,
                            1.5931733351727628, 2.2753462770828685, 3.0)
        self.assertEqual(case["mesh_vertices"][1], [-0.010024853856377927, 0.0, 0.0])
        self.assertEqual(case["mesh_vertices"][-1][1], 0.05193429609132301)

    def test_coarsening_preserves_points_controls_and_exported_reference_mesh(self):
        master = self.aligned()
        unchanged = copy.deepcopy(master)
        for nr, nt in ((4, 16), (16, 4), (8, 8)):
            case = nested_case(master, nr, nt)
            for j in range(nt + 1):
                for i in range(nr + 1):
                    self.assertEqual(case["mesh_vertices"][j * (nr + 1) + i],
                                     master["mesh_vertices"][j * (16 // nt) * 17 + i * (16 // nr)])
            self.assertEqual(case["solver"], master["solver"])
            self.assertEqual(case["outflow"], master["outflow"])
            with tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary) / "reference"
                prepare(directory, case, case["solver"]["max_steps"])
                actual = [list(map(float, line.split()))
                          for line in (directory / "mesh.vtk").read_text().splitlines()[6:]]
                expected = [case["mesh_vertices"][j * (nr + 1) + nr - i]
                            for j in range(nt + 1) for i in range(nr + 1)]
                self.assertEqual(actual, expected)
            case["mesh_vertices"][0][0] = 123.0
        self.assertEqual(master, unchanged)

    def test_incompatible_master_cannot_fall_back_to_generated_geometry(self):
        master = self.aligned()
        for nr, nt in ((32, 8), (8, 32), (6, 8), (8, 6), (0, 8)):
            with self.assertRaises(ValueError):
                nested_case(master, nr, nt)
        master["mesh_vertices"].pop()
        with self.assertRaises(ValueError):
            nested_case(master, 8, 8)
        with self.assertRaises(ValueError):
            aligned_case(master, 8, 8, 1.22, 1.6, 2.0, 3.0)

    def test_unmeshed_template_retains_its_generator(self):
        self.template["grid"]["shock_shape_mach"] = 3.0
        case = nested_case(self.template, 96, 48)
        self.assertNotIn("mesh_vertices", case)
        self.assertEqual(case["grid"]["shock_shape_mach"], 3.0)
        self.assertEqual(case["grid"]["radial_cells"], 96)

    def test_directional_driver_rejects_incompatible_grid_before_creating_a_run(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "master.json"
            source.write_text(json.dumps(self.aligned()))
            for counts in ((4, 8, 32), (4, 8, 24), (4, 8, 8)):
                output = Path(temporary) / "run"
                result = subprocess.run([
                    sys.executable, str(Path(__file__).with_name("verify_directional.py")),
                    "--case", str(source), "--output", str(output),
                    "--rust", str(Path(temporary) / "unused-executable"),
                    "--resolutions", *map(str, counts)], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("uniform refinement ratio", result.stderr)
                self.assertFalse(output.exists())
            master = self.aligned(8, 8)
            source.write_text(json.dumps(master))
            result = subprocess.run([
                sys.executable, str(Path(__file__).with_name("verify_directional.py")),
                "--case", str(source), "--output", str(output),
                "--rust", str(Path(temporary) / "unused-executable"),
                "--resolutions", "4", "8", "16"], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("divide the supplied master grid", result.stderr)
            self.assertFalse(output.exists())

    def test_cli_writes_complete_case_and_refuses_to_overwrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            source, output = (Path(temporary) / name for name in ("source.json", "input.json"))
            source.write_text(json.dumps(self.template))
            command = [sys.executable, str(Path(__file__).with_name("sphere_grid.py")),
                       "--case", str(source), "--output", str(output), "--radial-cells", "8",
                       "--angular-cells", "8", "--nose", "1.22", "--curvature", "1.6",
                       "--stretching", "2", "--mach", "3"]
            subprocess.run(command, check=True, capture_output=True)
            self.assertEqual(json.loads(output.read_text()), self.aligned(8, 8))
            before = output.read_bytes()
            refused = subprocess.run(command, capture_output=True)
            self.assertNotEqual(refused.returncode, 0)
            self.assertEqual(output.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
