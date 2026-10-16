"""Check lossless restart values and rejection of unsupported snapshot layouts."""
import gzip
import json
from pathlib import Path
import struct
import tempfile
import unittest

from eilmer_binary import encode_flow
from eilmer_restart import prepare_restart_blocking


SNAPSHOT = '''structured_grid_flow 1.0
label: FluidBlock-0
sim_time: 0.125
variables: 19
"pos.x" "pos.y" "pos.z" "volume" "rho" "vel.x" "vel.y" "vel.z" "p" "a" "mu" "k" "mu_t" "k_t" "S" "massf[0]-benchmark-gas" "u" "T" "dt_local"
dimensions: 2
nicell: 1
njcell: 1
nkcell: 1
-0.01 0.001 -0.0 2.294806710736169210e-11 3.483713638738895804e-03 1.041656853287109243e+03 0 0 300 347 1.8e-5 0.025 0 0 0 1 2.152875000000001164e+05 300 0.001
'''


class BinaryFlowTests(unittest.TestCase):
    def test_snapshot_values_survive_without_decimal_reader_roundoff(self):
        binary = encode_flow(SNAPSHOT)
        values = struct.unpack("=19d", binary[-19 * 8:])
        expected = {2: "-0x0.0p+0", 3: "0x1.93b4e80ea72bfp-36",
                    4: "0x1.c89e084b45403p-9", 5: "0x1.046a09e25e99ep+10",
                    16: "0x1.a47bc00000004p+17"}
        for index, hexadecimal in expected.items():
            self.assertEqual(values[index].hex(), hexadecimal)

    def test_bad_dimensions_fields_and_nonfinite_values_are_rejected(self):
        invalid = [SNAPSHOT.replace("nicell: 1", "nicell: 2"),
                   SNAPSHOT.replace('"rho" "vel.x"', '"vel.x" "rho"'),
                   SNAPSHOT.replace("dimensions: 2", "dimensions: 3"),
                   SNAPSHOT.replace("3.483713638738895804e-03", "nan"),
                   SNAPSHOT.replace("sim_time: 0.125", "time: 0.125")]
        for text in invalid:
            with self.assertRaises(ValueError):
                encode_flow(text)

    def test_binary_restart_preserves_reference_controls_and_source_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            source, target = Path(temporary) / "source", Path(temporary) / "copy"
            (source / "config").mkdir(parents=True)
            (source / "flow/t0001").mkdir(parents=True)
            snapshot = source / "flow/t0001/sphere.flow.b0000.t0001.gz"
            snapshot.write_bytes(gzip.compress(SNAPSHOT.encode(), mtime=0))
            (source / "config/sphere.config").write_text(json.dumps({
                "flow_format": "gziptext", "nfluidblock": 1,
                "block_0": {"nic": 1, "njc": 1, "nkc": 1}}))
            (source / "config/sphere.times").write_text("0001 0.125 0.01 1 100 1e-3\n")
            (source / "sphere-ref-residuals.saved").write_text("2.8676568355777206e9 1 2 3 4\n")
            control = {"halt_now": 0, "steady_state_solver_options": {
                "stop_on_relative_global_residual": 1e-12}}
            (source / "config/sphere.control").write_text(json.dumps(control))
            before = {str(p.relative_to(source)): p.read_bytes()
                      for p in source.rglob("*") if p.is_file()}
            result = prepare_restart_blocking(source, target, binary_flow=True)
            self.assertEqual(result["flow_storage"]["converted"], 1)
            for name, contents in before.items():
                self.assertEqual((source / name).read_bytes(), contents)
                if name not in ("config/sphere.config", "config/sphere.control"):
                    self.assertEqual((target / name).read_bytes(), contents)
            self.assertEqual(json.loads((target / "config/sphere.control").read_text()), control)
            self.assertEqual(json.loads((target / "config/sphere.config").read_text())[
                "flow_format"], "rawbinary")
            self.assertEqual((target / snapshot.relative_to(source)).with_suffix(".bin").read_bytes(),
                             encode_flow(SNAPSHOT))


if __name__ == "__main__":
    unittest.main()
