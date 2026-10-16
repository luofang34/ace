"""Verify that an Eilmer restart payload belongs to its selected time index."""
import gzip
import hashlib
import json
import math
import struct

from eilmer_binary import encode_flow


def binary_header(blob):
    """Read the native binary header and reject truncated or unsupported payloads."""
    magic = b"structured_grid_flow 1.0"
    if not blob.startswith(magic):
        raise ValueError("Expected structured rawbinary flow payload")
    offset = len(magic)

    def take(layout):
        nonlocal offset
        size = struct.calcsize(layout)
        if size > len(blob) - offset:
            raise ValueError("Truncated rawbinary flow header")
        values = struct.unpack_from(layout, blob, offset)
        offset += size
        return values

    def string():
        nonlocal offset
        length, = take("=i")
        if length < 0 or length > len(blob) - offset:
            raise ValueError("Invalid rawbinary string length")
        value = blob[offset:offset + length].decode("utf-8")
        offset += length
        return value

    label = string()
    time, count = take("=di")
    if count != 19 or not math.isfinite(time):
        raise ValueError("Expected finite time and single-species benchmark fields")
    fields = [string() for _ in range(count)]
    prefix = ["pos.x", "pos.y", "pos.z", "volume", "rho", "vel.x", "vel.y", "vel.z",
              "p", "a", "mu", "k", "mu_t", "k_t", "S"]
    if (fields[:15] != prefix or not fields[15].startswith("massf[0]-")
            or fields[16:] != ["u", "T", "dt_local"]):
        raise ValueError("Unsupported rawbinary field layout")
    dimensions, ni, nj, nk = take("=4i")
    if dimensions != 2 or min(ni, nj) <= 0 or nk != 1:
        raise ValueError("Expected a two-dimensional structured benchmark grid")
    if len(blob) - offset != ni * nj * nk * count * 8:
        raise ValueError("Rawbinary field payload size does not match its grid")
    return dict(label=label, time=time, shape=[ni, nj, nk], fields=fields)


def validate_snapshot_blocking(directory, snapshot, expected_time):
    """Fail closed when the configured files disagree with the restart time or grid."""
    config = json.loads((directory / "config/sphere.config").read_text())
    mode = config.get("flow_format", "gziptext")
    if config.get("new_flow_format", False) or mode not in ("rawbinary", "gziptext"):
        raise ValueError("Only legacy gziptext/rawbinary benchmark snapshots are supported")
    blocks = config.get("nfluidblock")
    if not isinstance(blocks, int) or blocks <= 0 or not math.isfinite(expected_time):
        raise ValueError("Positive fluid block count and finite restart time required")
    records = []
    for block in range(blocks):
        suffix = "bin" if mode == "rawbinary" else "gz"
        path = directory / f"flow/t{snapshot:04d}/sphere.flow.b{block:04d}.t{snapshot:04d}.{suffix}"
        data = path.read_bytes()
        blob = data if mode == "rawbinary" else encode_flow(gzip.decompress(data).decode("utf-8"))
        header = binary_header(blob)
        shape = [config[f"block_{block}"][key] for key in ("nic", "njc", "nkc")]
        if header["time"] != expected_time or header["shape"] != shape:
            raise ValueError(f"Snapshot/time-index mismatch: {path}: "
                             f"payload time={header['time']!r}, index time={expected_time!r}, "
                             f"payload grid={header['shape']}, configured grid={shape}")
        records.append(dict(path=str(path.relative_to(directory)),
                            sha256=hashlib.sha256(data).hexdigest(), **header))
    return dict(snapshot=snapshot, expected_time=expected_time, files=records,
                payload_time_and_grid_verified=True)
