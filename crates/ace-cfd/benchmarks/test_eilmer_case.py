"""Check that cross-code preparation consumes exported geometry without regeneration."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from eilmer_case import prepare


class MatchedMeshTests(unittest.TestCase):
    def test_outlet_selection_changes_only_the_generated_boundary(self):
        case = json.loads((Path(__file__).parents[1] / "cases/sphere.json").read_text())
        unchanged = copy.deepcopy(case)
        with tempfile.TemporaryDirectory() as temporary:
            native, projected = [Path(temporary) / name for name in ("native", "projected")]
            prepare(native, case, 100)
            prepare(projected, case, 100, outlet="zero-normal-gradient")
            for name in ("gas.lua", "mesh.vtk"):
                self.assertEqual((native / name).read_bytes(), (projected / name).read_bytes())
            expected = (native / "sphere.lua").read_text().replace(
                "north=OutFlowBC_Simple:new{}",
                "north=OutFlowBC_SimpleFlux:new{zeroNormalViscousGradients=true}")
            self.assertNotEqual(expected, (native / "sphere.lua").read_text())
            self.assertEqual((projected / "sphere.lua").read_text(), expected)
        self.assertEqual(case, unchanged)

    def test_unknown_outlet_is_rejected_before_creating_a_case(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / "case"
            with self.assertRaisesRegex(ValueError, "Unsupported Eilmer outlet"):
                prepare(directory, {}, 100, outlet="zero-traction")
            self.assertFalse(directory.exists())

    def test_common_outlet_comparison_rejects_incompatible_rust_modes(self):
        case = json.loads((Path(__file__).parents[1] / "cases/sphere.json").read_text())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for outflow in (None, "zero_gradient", "one_sided", "cell_flux"):
                if outflow is not None:
                    case["outflow"] = outflow
                source, output = root / "case.json", root / "result"
                source.write_text(json.dumps(case))
                result = subprocess.run([
                    sys.executable, str(Path(__file__).with_name("compare.py")),
                    "--case", str(source), "--output", str(output),
                    "--rust", str(root / "unused"), "--eilmer", str(root / "unused"),
                    "--eilmer-outlet", "zero-normal-gradient"], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("requires Rust outflow=zero_gradient_cell_flux", result.stderr)
                self.assertFalse(output.exists())

    def test_exported_vertices_are_reversed_only_in_the_radial_direction(self):
        case = json.loads((Path(__file__).parents[1] / "cases/sphere.json").read_text())
        case["grid"].update(radial_cells=4, angular_cells=4, shock_shape_mach=3.0)
        vertices = [[-1.0 - i * 0.1 - j * 0.001, j * 0.2, 0.0]
                    for j in range(5) for i in range(5)]
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / "case"
            prepare(directory, case, 100, vertices)
            lines = (directory / "mesh.vtk").read_text().splitlines()[6:]
            actual = [list(map(float, line.split())) for line in lines]
            expected = [vertices[j * 5 + 4 - i] for j in range(5) for i in range(5)]
            self.assertEqual(actual, expected)
            case["mesh_vertices"] = vertices
            embedded = Path(temporary) / "embedded"
            prepare(embedded, case, 100)
            self.assertEqual((embedded / "mesh.vtk").read_text(), (directory / "mesh.vtk").read_text())

    def test_shaped_grid_cannot_silently_fall_back_to_a_spherical_grid(self):
        case = json.loads((Path(__file__).parents[1] / "cases/sphere.json").read_text())
        case["grid"]["shock_shape_mach"] = 3.0
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(ValueError):
                prepare(Path(temporary) / "case", case, 100)


if __name__ == "__main__":
    unittest.main()
