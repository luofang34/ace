"""Build the Eilmer adapter and check interoperability, streaming and malformed files."""
import argparse
import gzip
import json
from pathlib import Path
import random
import subprocess
import tempfile
import zlib


def verify_blocking(compiler, directory):
    source = Path(__file__).resolve().parent / "eilmer_gzip"
    binary = directory / "probe"
    subprocess.run([str(compiler), "-w", "-O2", f"-I{source}", str(source / "probe.d"),
                    str(source / "gzip.d"), f"-of={binary}"], check=True)
    path = directory / "data.gz"
    checks = 0

    def run(mode, data=None):
        return subprocess.run([str(binary), mode, str(path)], input=data, capture_output=True)

    samples = [b"", b"\n", b"a\n\nb", b"abc\r\n", b"x" * 1_000_000,
               random.Random(810).randbytes(1_000_000)]
    # Exact buffer boundaries exercise a decoder that fills its output at member end.
    for size in (32767, 32768, 32769, 65536):
        samples.extend((b"x" * size, random.Random(size).randbytes(size)))
    for data in samples:
        path.write_bytes(gzip.compress(data, mtime=0))
        decoded = run("read")
        assert decoded.returncode == 0 and decoded.stdout == data, decoded.stderr
        lines = run("lines")
        expected = data if not data or data.endswith(b"\n") else data + b"\n"
        assert lines.returncode == 0 and lines.stdout == expected, lines.stderr
        encoded = run("write", data)
        raw = path.read_bytes()
        assert encoded.returncode == 0 and raw[:2] == b"\x1f\x8b" and gzip.decompress(raw) == data, encoded.stderr
        decoded = run("read")
        assert decoded.returncode == 0 and decoded.stdout == data, decoded.stderr
        checks += 4
    path.write_bytes(b"".join(gzip.compress(data, mtime=0) for data in samples))
    decoded = run("read")
    assert decoded.returncode == 0 and decoded.stdout == b"".join(samples), decoded.stderr
    checks += 1
    # Exercise optional gzip filename metadata through an independent writer.
    with path.open("wb") as raw:
        with gzip.GzipFile(filename="field.dat", mode="wb", fileobj=raw, mtime=0) as stream:
            stream.write(samples[-1])
    decoded = run("read")
    assert decoded.returncode == 0 and decoded.stdout == samples[-1], decoded.stderr
    checks += 1
    valid = gzip.compress(b"state\n" * 20, mtime=0)
    corrupt = [b"", b"not gzip", zlib.compress(b"zlib is not gzip"), valid + b"garbage"]
    corrupt += [valid[:i] for i in range(1, len(valid))]
    for offset in (-8, -4):  # CRC32 and ISIZE are independently checked by inflate.
        data = bytearray(valid)
        data[offset] ^= 1
        corrupt.append(bytes(data))
    for data in corrupt:
        path.write_bytes(data)
        assert run("read").returncode != 0, "invalid stream was accepted"
        checks += 1
    return {"passed": True, "checks": checks, "adapter_license": "AGPL-3.0-only"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", required=True, type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="ace-gzip-") as directory:
        print(json.dumps(verify_blocking(args.compiler.resolve(), Path(directory))))


if __name__ == "__main__":
    main()
