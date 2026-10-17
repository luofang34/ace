# SPDX-License-Identifier: AGPL-3.0-only
"""Check native conserved restart, snapshot rotation and corrupt-input rejection."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess

from eilmer_case import prepare
from eilmer_conserved import CELL, MARKER
from eilmer_nk import configure
from eilmer_restart import prepare_restart_blocking
from trace_eilmer import trace_blocking
from verify_eilmer_restart import verify_blocking as verify_rotation_blocking


def check_roundtrip_blocking(source, output, executable, installation, case):
    """Compare the native accepted residual and U with a no-iteration restart."""
    trace = trace_blocking(source, output, executable, installation, case, state_purity=True)
    manifest = json.loads((output / "restart-input.json").read_text())
    payload = manifest["snapshot_payload"]
    sidecars = [item for item in payload["files"] if item.get("kind") == "conserved"]
    if (len(sidecars) != 1 or not trace["conserved_state_exact"]
            or not trace["spatial_operator"]["matches"]):
        raise RuntimeError("Checkpoint verification requires one exact conserved block")
    sidecar = output / sidecars[0]["path"]
    accepted = source / (sidecars[0]["path"] + ".cells.csv")
    if accepted.read_bytes() != (output / "trace-cells.csv").read_bytes():
        raise RuntimeError("Restart changed accepted cell states, gradients or residuals")
    values = b"".join(struct.pack("=4d", *row[1:])
                      for row in CELL.iter_unpack(sidecar.read_bytes()[112:-32]))
    if values != (output / "trace-conserved.bin").read_bytes():
        raise RuntimeError("Restart changed accepted conserved bits")
    if not trace["state_purity"]["passed"] or not trace["checkpoint_unchanged"]:
        raise RuntimeError("Repeated residual evaluation changed the accepted state")
    return dict(step=manifest["step"], cells=sidecars[0]["cells"],
                conserved_bits_identical=True, cell_trace_identical=True,
                repeated_residual_identical=True, iterations_advanced=0)


def check_rejections_blocking(source, output, executable, env):
    """Exercise the native loader directly so Python validation cannot mask a defect."""
    kinds = ("checksum", "step", "flow", "reference", "density", "energy", "missing",
             "unmarked", "frozen-sensor", "frozen-limiter")
    for kind in kinds:
        directory = output / kind
        manifest = prepare_restart_blocking(source, directory)
        record = next(item for item in manifest["snapshot_payload"]["files"]
                      if item.get("kind") == "conserved")
        path = directory / record["path"]
        data = bytearray(path.read_bytes()[:-32])
        if kind == "checksum":
            path.write_bytes(path.read_bytes()[:-1] + bytes([path.read_bytes()[-1] ^ 1]))
        elif kind in ("step", "flow", "reference", "density", "energy"):
            offset, replacement = {
                "step": (24, struct.pack("=Q", manifest["step"] + 1)),
                "flow": (48, bytes(32)), "reference": (80, bytes(32)),
                "density": (120, struct.pack("=d", 0.0)),
                "energy": (144, struct.pack("=d", 0.0)),
            }[kind]
            data[offset:offset + len(replacement)] = replacement
            path.write_bytes(data + hashlib.sha256(data).digest())
        elif kind == "missing":
            path.unlink()
        elif kind == "unmarked":
            (directory / MARKER).unlink()
        else:
            config_path = directory / "config/sphere.config"
            config = json.loads(config_path.read_text())
            if kind == "frozen-sensor":
                config["frozen_shock_detector"] = True
            else:
                config["freeze_limiter_on_step"] = manifest["step"] - 1
            config_path.write_text(json.dumps(config, indent=2))
        (directory / "ace-trace-only").touch()
        with (directory / "rejection.log").open("w") as log:
            result = subprocess.run([str(executable), *manifest["command_arguments"]],
                                    cwd=directory, env=env, stdout=log,
                                    stderr=subprocess.STDOUT, timeout=60)
        log = (directory / "rejection.log").read_text()
        if (result.returncode <= 0 or "STEP=" in log or "conserved" not in log.lower()
                or "ACE conserved checkpoint restored:" in log):
            raise RuntimeError(f"Native loader did not reject {kind} before restoration")
    return list(kinds)


def make_fixture_blocking(source, executable, installation, case, env, startup_steps):
    """Save accepted updates using the requested startup/final spatial operator."""
    prepare(source, case, 4)
    configure(source)
    (source / "ace-checkpoint-audit").touch()
    with (source / "prep.log").open("w") as log:
        subprocess.run([str(installation / "bin/e4shared"), "--prep", "--job=sphere"],
                       cwd=source, env=env, stdout=log, stderr=subprocess.STDOUT,
                       check=True, timeout=60)
    control_path = source / "config/sphere.control"
    control = json.loads(control_path.read_text())
    control["steady_state_solver_options"].update(
        number_total_steps=4, number_start_up_steps=startup_steps, snapshots_count=1,
        number_total_snapshots=2)
    control_path.write_text(json.dumps(control, indent=2))
    with (source / "run.log").open("w") as log:
        subprocess.run([str(executable), "--job=sphere", "--max-cpus=1"], cwd=source,
                       env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=120)


def verify_blocking(executable, installation, output, case):
    """Build a small unconverged fixture; stopping and accuracy gates stay unchanged."""
    output.mkdir()
    env = dict(os.environ, DGD=str(installation),
               DGD_LUA_PATH=str(installation / "lib/?.lua"),
               DGD_LUA_CPATH=str(installation / "lib/?.so"))
    case = json.loads(json.dumps(case))
    case["grid"].update(radial_cells=8, angular_cells=8, shock_shape_mach=None)
    case.pop("mesh_vertices", None)
    source = output / "accepted"
    # The round-trip comparison must use the same final operator on both sides.
    make_fixture_blocking(source, executable, installation, case, env, 0)
    records = [check_roundtrip_blocking(source, output / "trace-accepted", executable,
                                       installation, case)]
    rotation = verify_rotation_blocking(source, executable, installation, output / "rotation")
    for index in range(2):
        restarted = output / "rotation" / f"restart-{index}"
        records.append(check_roundtrip_blocking(restarted, output / f"trace-restart-{index}",
                                               executable, installation, case))
    rejected = check_rejections_blocking(source, output, executable, env)
    # The same executable still permits primitive-only diagnostics without the marker.
    primitive = output / "primitive-only"
    shutil.copytree(source, primitive)
    (primitive / MARKER).unlink()
    for path in primitive.glob("flow/t*/*.conserved*"):
        path.unlink()
    diagnostic = trace_blocking(primitive, output / "trace-primitive", executable,
                                installation, case, state_purity=True)
    if diagnostic["conserved_state_exact"]:
        raise RuntimeError("Primitive-only diagnostics claimed exact conserved recovery")
    startup = output / "startup"
    make_fixture_blocking(startup, executable, installation, case, env, 50)
    startup_trace = trace_blocking(startup, output / "trace-startup", executable,
                                   installation, case, state_purity=True)
    if startup_trace["spatial_operator"] != dict(diagnostic_order=2, iteration_order=1, matches=False):
        raise RuntimeError("Startup diagnostic did not identify a changed spatial operator")
    result = dict(passed=True, records=records, rejected=rejected,
                  startup_operator_mismatch_identified=True,
                  primitive_only_unqualified=True, thresholds_unchanged=rotation["thresholds_unchanged"],
                  executable_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
                  spatial_accuracy_qualified=False, timing_qualified=False)
    (output / "checkpoint-verification.json").write_text(json.dumps(result, indent=2))
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("executable", "installation", "output", "case"):
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(verify_blocking(args.executable.resolve(), args.installation.resolve(),
                                    args.output.resolve(), json.loads(args.case.read_text()))))
