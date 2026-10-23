# SPDX-License-Identifier: AGPL-3.0-only
"""Validate conserved checkpoint bytes bound to an Eilmer flow snapshot."""
import hashlib
import math
import struct

MARKER = "ace-exact-conserved-checkpoint"
MAGIC = 0x41434543504B5431
HEADER = struct.Struct("=5Qd")
CELL = struct.Struct("=Q4d")


def validate_conserved_blocking(directory, flow_path, *, block, cells, time, step):
    """Reject stale, malformed or corrupted native-endian benchmark sidecars."""
    path = flow_path.with_name(flow_path.name + ".conserved")
    blob = path.read_bytes()
    if len(blob) != 144 + CELL.size * cells:
        raise ValueError(f"Conserved checkpoint size mismatch: {path}")
    if hashlib.sha256(blob[:-32]).digest() != blob[-32:]:
        raise ValueError(f"Conserved checkpoint checksum mismatch: {path}")
    if HEADER.unpack_from(blob) != (MAGIC, block, cells, step, 4, time):
        raise ValueError(f"Conserved checkpoint metadata mismatch: {path}")
    flow_hash = hashlib.sha256(flow_path.read_bytes()).digest()
    reference_hash = hashlib.sha256((directory / "sphere-ref-residuals.saved").read_bytes()).digest()
    if blob[48:80] != flow_hash or blob[80:112] != reference_hash:
        raise ValueError(f"Conserved checkpoint snapshot/reference mismatch: {path}")
    for index, (cell, rho, momx, momy, energy) in enumerate(CELL.iter_unpack(blob[112:-32])):
        if cell != index or not all(map(math.isfinite, (rho, momx, momy, energy))):
            raise ValueError(f"Conserved checkpoint cell order or finite-value failure: {path}")
        if rho <= 0 or energy <= 0.5 * (momx * momx + momy * momy) / rho:
            raise ValueError(f"Conserved checkpoint nonphysical state at cell {index}: {path}")
    return dict(path=str(path.relative_to(directory)), sha256=hashlib.sha256(blob).hexdigest(),
                kind="conserved", cells=cells, step=step, time=time)
