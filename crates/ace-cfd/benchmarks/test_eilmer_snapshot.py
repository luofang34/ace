"""Reject stale snapshot slots even when a correctly named binary file still exists."""
import json
from pathlib import Path
import struct
import tempfile
import unittest

from eilmer_binary import encode_flow
from eilmer_restart import prepare_restart_blocking
from eilmer_snapshot import binary_header, validate_snapshot_blocking
from test_eilmer_binary import SNAPSHOT


class SnapshotTests(unittest.TestCase):
    def test_binary_header_checks_the_actual_payload(self):
        blob = encode_flow(SNAPSHOT)
        header = binary_header(blob)
        self.assertEqual(header["time"], 0.125)
        self.assertEqual(header["shape"], [1, 1, 1])
        for bad in (blob[:-1], blob + b"x", blob[:25], b"gziptext",
                    blob[:22] + struct.pack("=i", -1) + blob[26:]):
            with self.assertRaises(ValueError):
                binary_header(bad)

    def test_restart_rejects_stale_binary_before_creating_a_copy(self):
        with tempfile.TemporaryDirectory() as temporary:
            source, destination = Path(temporary) / "source", Path(temporary) / "restart"
            (source / "config").mkdir(parents=True)
            (source / "flow/t0001").mkdir(parents=True)
            (source / "config/sphere.config").write_text(json.dumps({
                "flow_format": "rawbinary", "nfluidblock": 1,
                "block_0": {"nic": 1, "njc": 1, "nkc": 1}}))
            (source / "config/sphere.times").write_text("0001 0.25 0.01 1 100 1e-3\n")
            (source / "sphere-ref-residuals.saved").write_text("2.5e9 1 2 3 4\n")
            (source / "config/sphere.control").write_text('{"halt_now":0}')
            path = source / "flow/t0001/sphere.flow.b0000.t0001.bin"
            path.write_bytes(encode_flow(SNAPSHOT))
            # The solver's rotation bug creates this current state but still reads .bin.
            current = encode_flow(SNAPSHOT.replace("sim_time: 0.125", "sim_time: 0.25"))
            path.with_suffix(".gz").write_bytes(current)
            with self.assertRaisesRegex(ValueError, "Snapshot/time-index mismatch"):
                prepare_restart_blocking(source, destination)
            self.assertFalse(destination.exists())
            path.write_bytes(current)
            manifest = prepare_restart_blocking(source, destination)
            self.assertTrue(manifest["snapshot_payload"]["payload_time_and_grid_verified"])
            self.assertEqual((destination / path.relative_to(source)).read_bytes(), current)
            with self.assertRaisesRegex(ValueError, "Snapshot/time-index mismatch"):
                validate_snapshot_blocking(source, 1, 0.125)


if __name__ == "__main__":
    unittest.main()
