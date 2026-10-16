"""Check that cross-code preparation consumes exported geometry without regeneration."""
import json
from pathlib import Path
import tempfile
import unittest

from eilmer_case import prepare


class MatchedMeshTests(unittest.TestCase):
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
