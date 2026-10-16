"""Exercise the progress monitor through a child process and actual output files."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

from compare import load_wall
from eilmer_run import run
from eilmer_nk import measure_blocking
from eilmer_binary import encode_flow
from test_eilmer_binary import SNAPSHOT


class EilmerMonitorTests(unittest.TestCase):
    def test_newton_krylov_requires_three_stable_load_increments_blocking(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "config").mkdir()
            (directory / "config/sphere.config").write_text(json.dumps({
                "flow_format": "rawbinary", "nfluidblock": 1,
                "block_0": {"nic": 1, "njc": 1, "nkc": 1}}))
            (directory / "config/sphere.times").write_text("0001 0.125 0.01 1 100 1e-12\n")
            (directory / "flow/t0001").mkdir(parents=True)
            (directory / "flow/t0001/sphere.flow.b0000.t0001.bin").write_bytes(encode_flow(SNAPSHOT))
            (directory / "sphere-ref-residuals.saved").write_text("1000 1 2 3 4\n")
            control = directory / "config/sphere.control"
            control.write_text('{"steady_state_solver_options": {"stop_on_relative_global_residual": 1e-12}}')
            (directory / "run.log").write_text(
                "GLOBAL: 1000\nSTEP= 100\n  global 1e-12 1e-15\n"
                "  mass 1e-12\n  x-mom 1e-12\n  y-mom 1e-12\n"
                "  total-energy 1e-12\nrelative global residual is below target\n")
            for index, heat in enumerate([10.0, 20.0, 20.0, 20.0]):
                folder = directory / f"loads/t{index:04d}"
                folder.mkdir(parents=True)
                (folder / "wall.loads.dat").write_text(
                    "# loads\n# 1:pos.x 2:pos.y 3:q_total 4:outsign 5:p 6:area\n"
                    f"-1 0.1 {-heat} 1 100 1\n")
            case = {"inflow": {"density": 1.0, "velocity": [1.0, 0.0, 0.0], "pressure": 1.0},
                    "gas": {"gamma": 1.4}, "grid": {"radius": 1.0},
                    "solver": {"residual_tolerance": 1e-6, "heat_flux_tolerance": 1e-5}}
            evidence = measure_blocking(directory, case, load_wall, 1.0)
            self.assertFalse(evidence["converged"])
            self.assertEqual(evidence["wall_change"], 0.5)
            first = directory / "loads/t0000/wall.loads.dat"
            first.write_text(first.read_text().replace("-10.0", "-20.0"))
            self.assertTrue(measure_blocking(directory, case, load_wall, 1.0)["iteration_converged"])
            self.assertFalse(measure_blocking(directory, case, load_wall, 1.0)["converged"])
            times = directory / "config/sphere.times"
            times.write_text(times.read_text().replace("1 100 ", "1 99 "))
            self.assertFalse(measure_blocking(directory, case, load_wall, 1.0)["iteration_converged"])
            times.write_text(times.read_text().replace("1 99 ", "1 100 "))
            control.write_text(control.read_text().replace("1e-12", "1e-8"))
            self.assertFalse(measure_blocking(directory, case, load_wall, 1.0)["iteration_converged"])
            control.write_text(control.read_text().replace("1e-8", "1e-12"))
            (directory / "restart-input.json").write_text('{"reference_residual": 2000.0}')
            evidence = measure_blocking(directory, case, load_wall, 1.0)
            self.assertFalse(evidence["converged"])
            self.assertFalse(evidence["reference_preserved"])

    def test_repeated_progress_cannot_count_one_wall_snapshot_as_three_checks(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "config").mkdir()
            (directory / "config/sphere.control").write_text('{"halt_now": 0}')
            (directory / "config/sphere-residuals.txt").write_text(
                "step time wall mass xmom ymom zmom energy\n20 0 0 0 0 0 0 0\n")
            wall = ("# loads\n# 1:pos.x 2:pos.y 3:q_total 4:outsign 5:p 6:area\n"
                    "-1 0.1 -10 1 100 1\n")
            for index in range(2):
                folder = directory / f"loads/t{index:04d}"
                folder.mkdir(parents=True)
                (folder / "wall.loads.dat").write_text(wall)
            executable = directory / "solver"
            executable.write_text(
                f"#!{sys.executable}\n"
                "for step in range(20, 241, 20):\n"
                "    print(f'Step= {step} t= 0.0', flush=True)\n")
            executable.chmod(0o755)
            case = {"inflow": {"density": 1.0, "velocity": [1.0, 0.0, 0.0], "pressure": 1.0},
                    "gas": {"gamma": 1.4}, "grid": {"radius": 1.0},
                    "solver": {"check_interval": 20, "residual_tolerance": 1e-6,
                               "heat_flux_tolerance": 1e-5}}
            result = run(directory, executable, case, dict(os.environ), load_wall)
            self.assertFalse(result["converged"])
            self.assertEqual(result["history"], [])
            self.assertEqual(json.loads((directory / "config/sphere.control").read_text())["halt_now"], 0)


if __name__ == "__main__":
    unittest.main()
