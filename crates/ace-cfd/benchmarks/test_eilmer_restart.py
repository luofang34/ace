"""Check actual restart-file copying, schedule changes and tolerance preservation."""
import json
from pathlib import Path
import tempfile
import unittest

from eilmer_restart import prepare_restart_blocking
from eilmer_binary import encode_flow
from test_eilmer_binary import SNAPSHOT


class RestartTests(unittest.TestCase):
    def test_saved_reference_and_stopping_rules_survive_restart_preparation(self):
        with tempfile.TemporaryDirectory() as temporary:
            source, target = Path(temporary) / "source", Path(temporary) / "target"
            (source / "config").mkdir(parents=True)
            (source / "flow/t0001").mkdir(parents=True)
            (source / "flow/t0002").mkdir(parents=True)
            checkpoint = "flow/t0001/sphere.flow.b0000.t0001.bin"
            payload = encode_flow(SNAPSHOT.replace("sim_time: 0.125", "sim_time: 0.1"))
            (source / checkpoint).write_bytes(payload)
            (source / "config/sphere.config").write_text(json.dumps({
                "flow_format": "rawbinary", "nfluidblock": 1,
                "block_0": {"nic": 1, "njc": 1, "nkc": 1}}))
            (source / "sphere-ref-residuals.saved").write_text("1.5e12 1 2 3 4\n")
            (source / "config/sphere.times").write_text(
                "# time\n0000 0 nan 1 0 1.5e12\n"
                "0001 0.1 0.01 1.0 100 1e8\n0002 0.05 0.01 1.0 50 2e8\n")
            control = {"halt_now": 1, "steady_state_solver_options": {
                "stop_on_relative_global_residual": 1e-12,
                "stop_on_absolute_global_residual": 1e-99}}
            (source / "config/sphere.control").write_text(json.dumps(control))
            for name in ("convergence.json", "results.json", "metrics.json", "trace-result.json",
                         "repeatability.json", "ace-trace-only", "ace-trace-state-purity"):
                (source / name).write_text('{"converged": true}')
            result = prepare_restart_blocking(source, target, [(101, 50.0)])
            self.assertEqual(result["reference_residual"], 1.5e12)
            self.assertIn("--snapshot-start=1", result["command_arguments"])
            self.assertEqual((target / "sphere-ref-residuals.saved").read_bytes(),
                             (source / "sphere-ref-residuals.saved").read_bytes())
            self.assertEqual((target / checkpoint).read_bytes(), payload)
            updated = json.loads((target / "config/sphere.control").read_text())
            self.assertEqual(updated["halt_now"], 0)
            for key, value in control["steady_state_solver_options"].items():
                self.assertEqual(updated["steady_state_solver_options"][key], value)
            self.assertEqual(json.loads((source / "config/sphere.control").read_text()), control)
            for name in ("convergence.json", "results.json", "metrics.json", "trace-result.json",
                         "repeatability.json", "ace-trace-only", "ace-trace-state-purity"):
                self.assertFalse((target / name).exists())
                self.assertTrue((source / name).exists())
            with self.assertRaises(ValueError):
                prepare_restart_blocking(source, Path(temporary) / "bad", [(100, 50.0)])


if __name__ == "__main__":
    unittest.main()
