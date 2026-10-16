"""Preserve binary64 values when loading structured single-species Eilmer snapshots."""
import gzip
import json
import math
import shlex
import struct
import sys


def encode_flow(text):
    """Encode the native-endian legacy rawbinary layout consumed by Eilmer.

    Decimal conversion in some D runtimes does not round to the nearest binary64.
    Python converts the saved decimals before the solver reads binary doubles.
    Restrict the field layout to this benchmark's single-temperature ideal gas.
    """
    lines = text.splitlines()
    prefix = ["pos.x", "pos.y", "pos.z", "volume", "rho", "vel.x", "vel.y", "vel.z",
              "p", "a", "mu", "k", "mu_t", "k_t", "S"]
    if len(lines) < 9 or lines[0] != "structured_grid_flow 1.0":
        raise ValueError("Expected a structured_grid_flow 1.0 snapshot")
    keys = ["label", "sim_time", "variables", "dimensions", "nicell", "njcell", "nkcell"]
    headers = [line.partition(":") for line in lines[1:4] + lines[5:9]]
    if any(key != expected or separator != ":"
           for (key, separator, _), expected in zip(headers, keys, strict=True)):
        raise ValueError("Invalid structured flow header")
    label, time, count, dimensions, ni, nj, nk = [value.strip() for _, _, value in headers]
    names = shlex.split(lines[4])
    if (len(names) != 19 or int(count) != len(names) or names[:15] != prefix
            or not names[15].startswith("massf[0]-") or names[16:] != ["u", "T", "dt_local"]):
        raise ValueError("Unsupported field layout; expected the single-species benchmark")
    shape = [int(value) for value in (dimensions, ni, nj, nk)]
    rows = [list(map(float, line.split())) for line in lines[9:]]
    if (shape[0] != 2 or min(shape[1:]) <= 0 or shape[3] != 1
            or len(rows) != math.prod(shape[1:])
            or any(len(row) != len(names) or not all(map(math.isfinite, row)) for row in rows)
            or not math.isfinite(float(time))):
        raise ValueError("Invalid snapshot dimensions, row count, or numeric values")
    blob = bytearray(lines[0].encode("ascii"))

    def string(value):
        data = value.encode("utf-8")
        blob.extend(struct.pack("=i", len(data)))
        blob.extend(data)

    string(label)
    blob.extend(struct.pack("=di", float(time), len(names)))
    for name in names:
        string(name)
    blob.extend(struct.pack("=4i", *shape))
    for row in rows:
        blob.extend(struct.pack("=19d", *row))
    return bytes(blob)


def convert_restart_blocking(directory):
    """Convert a copied case's text snapshots, retaining originals and stopping controls."""
    config_path = directory / "config/sphere.config"
    config = json.loads(config_path.read_text())
    if config.get("new_flow_format", False):
        raise ValueError("Only the legacy flow format is supported")
    if config.get("flow_format", "gziptext") == "rawbinary":
        return dict(format="rawbinary", converted=0, byteorder=sys.byteorder)
    if config.get("flow_format", "gziptext") != "gziptext":
        raise ValueError("Expected gziptext or rawbinary flow format")
    paths = sorted(directory.glob("flow/t*/sphere.flow.b*.gz"))
    if not paths:
        raise ValueError("No structured flow snapshots to convert")
    for path in paths:
        data = encode_flow(gzip.decompress(path.read_bytes()).decode("utf-8"))
        path.with_suffix(".bin").write_bytes(data)
    config["flow_format"] = "rawbinary"
    config_path.write_text(json.dumps(config, indent=2))
    return dict(format="rawbinary", converted=len(paths), byteorder=sys.byteorder)
