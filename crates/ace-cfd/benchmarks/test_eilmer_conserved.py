# SPDX-License-Identifier: AGPL-3.0-only
"""Exercise exact checkpoint validation through real copied restart files."""
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest

from eilmer_binary import encode_flow
from eilmer_conserved import CELL, HEADER, MAGIC, MARKER
from eilmer_restart import prepare_restart_blocking
from test_eilmer_binary import SNAPSHOT


def checkpoint_case_blocking(directory):
    (directory / "config").mkdir(parents=True)
    (directory / "flow/t0001").mkdir(parents=True)
    (directory / "config/sphere.config").write_text(json.dumps({
        "flow_format": "rawbinary", "nfluidblock": 1,
        "block_0": {"nic": 1, "njc": 1, "nkc": 1}}))
    (directory / "config/sphere.times").write_text("0001 0.125 0.01 1 100 1e-3\n")
    (directory / "config/sphere.control").write_text('{"halt_now":0}')
    reference = b"2.5e9 1 2 3 4\n"
    (directory / "sphere-ref-residuals.saved").write_bytes(reference)
    flow = directory / "flow/t0001/sphere.flow.b0000.t0001.bin"
    flow.write_bytes(encode_flow(SNAPSHOT))
    (directory / MARKER).touch()
    data = (HEADER.pack(MAGIC, 0, 1, 100, 4, 0.125)
            + hashlib.sha256(flow.read_bytes()).digest() + hashlib.sha256(reference).digest()
            + CELL.pack(0, 0.003483713638738896, 3.62883418668214, -0.0, 2640.0))
    sidecar = flow.with_name(flow.name + ".conserved")
    sidecar.write_bytes(data + hashlib.sha256(data).digest())
    return flow, sidecar


class ConservedCheckpointTests(unittest.TestCase):
    def test_exact_values_and_provenance_survive_restart_copy(self):
        with tempfile.TemporaryDirectory() as temporary:
            source, target = Path(temporary) / "source", Path(temporary) / "target"
            flow, sidecar = checkpoint_case_blocking(source)
            result = prepare_restart_blocking(source, target)
            self.assertTrue(result["snapshot_payload"]["conserved_state_exact"])
            self.assertEqual(len(result["snapshot_payload"]["files"]), 2)
            for path in (flow, sidecar):
                self.assertEqual((target / path.relative_to(source)).read_bytes(), path.read_bytes())

    def test_invalid_sidecars_fail_before_copying(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "source"
            _, sidecar = checkpoint_case_blocking(source)
            original = sidecar.read_bytes()
            bad_payloads = [original[:-1], original + b"x", b"x" + original[1:]]
            # A valid checksum cannot hide a stale step, layout, binding or cell payload.
            for offset, replacement in ((0, struct.pack("=Q", 0)), (8, struct.pack("=Q", 1)),
                                        (24, struct.pack("=Q", 99)), (32, struct.pack("=Q", 5)),
                                        (40, struct.pack("=d", 0.25)), (48, b"x" * 32),
                                        (80, b"x" * 32), (112, struct.pack("=Q", 1)),
                                        (120, struct.pack("=d", float("nan"))),
                                        (120, struct.pack("=d", -1.0)),
                                        (144, struct.pack("=d", 1.0))):
                data = original[:offset] + replacement + original[offset + len(replacement):-32]
                bad_payloads.append(data + hashlib.sha256(data).digest())
            for index, payload in enumerate(bad_payloads):
                with self.subTest(index=index):
                    sidecar.write_bytes(payload)
                    target = Path(temporary) / f"bad-{index}"
                    with self.assertRaises(ValueError):
                        prepare_restart_blocking(source, target)
                    self.assertFalse(target.exists())
            sidecar.unlink()
            with self.assertRaises(FileNotFoundError):
                prepare_restart_blocking(source, Path(temporary) / "missing")
            sidecar.write_bytes(original)
            (source / MARKER).unlink()
            with self.assertRaisesRegex(ValueError, "without its exact-restart mode"):
                prepare_restart_blocking(source, Path(temporary) / "unmarked")


if __name__ == "__main__":
    unittest.main()
