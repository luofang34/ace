"""Evaluate an instrumented Eilmer checkpoint without advancing its solution."""
import argparse
import csv
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess

from eilmer_restart import prepare_restart_blocking
from trace_jacobian import check_jacobian_blocking
from trace_reconstruction import check_reconstruction_blocking


def read_trace_blocking(path):
    with path.open() as stream:
        rows = [{key: float(value) for key, value in row.items()}
                for row in csv.DictReader(stream)]
    if not rows or any(not math.isfinite(value) for row in rows for value in row.values()):
        raise ValueError(f"Empty or nonfinite trace: {path}")
    return rows


def state_hashes_blocking(directory, snapshot):
    paths = [directory / "config/sphere.times", directory / "sphere-ref-residuals.saved"]
    paths.extend(sorted((directory / f"flow/t{snapshot:04d}").glob("*")))
    return {str(path.relative_to(directory)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in paths if path.is_file()}


def shock_suppression(cells, faces):
    """Flagged internal interfaces must use unreconstructed neighbouring states."""
    cells = {int(cell["id"]): cell for cell in cells}
    errors = []
    for face in faces:
        left, right = int(face["left"]), int(face["right"])
        if face["S"] < 1 - 1e-15 or left not in cells or right not in cells:
            continue
        for key in ("rho", "p", "T"):
            expected = 0.5 * (cells[left][key] + cells[right][key])
            errors.append(abs(face[key] / expected - 1))
    maximum = max(errors) if errors else None
    return dict(passed=maximum is not None and maximum < 1e-12,
                internal_faces=len(errors) // 3, maximum_relative_deviation=maximum)


def trace_blocking(source, output, executable, installation, case, *, jacobian=False,
                   jacobian_outlet=False, binary_flow=False, state_purity=False,
                   constant_reconstruction=False):
    """Trace a copy; checkpoint hashes and the iteration log guard against advancement."""
    binary = executable.read_bytes()
    if any(marker not in binary for marker in
           (b"ace-trace-only", b"trace-axis-consistency.csv", b"trace-total.csv")):
        raise ValueError("Executable does not contain the residual-trace instrumentation")
    if jacobian_outlet and b"ace-trace-jacobian-outlet" not in binary:
        raise ValueError("Executable does not contain the outlet derivative probe")
    if state_purity and b"ace-trace-state-purity" not in binary:
        raise ValueError("Executable does not contain the repeated-residual purity probe")
    if constant_reconstruction and b"ace-trace-constant-reconstruction" not in binary:
        raise ValueError("Executable does not contain the constant-reconstruction probe")
    jacobian = jacobian or jacobian_outlet
    manifest = prepare_restart_blocking(source, output, binary_flow=binary_flow)
    # A copied trace must not supply evidence for an executable that did not
    # produce that diagnostic during this invocation.
    for pattern in ("trace-*.csv", "strain-*.csv", "saved-*.csv", "state-purity-*.csv"):
        for path in output.glob(pattern):
            path.unlink()
    frozen = output / "trace-executable"
    shutil.copy2(executable, frozen)
    before = state_hashes_blocking(output, manifest["snapshot"])
    (output / "ace-trace-only").touch()
    marker = output / "ace-trace-jacobian"
    marker.unlink(missing_ok=True)
    if jacobian:
        marker.touch()
    outlet_marker = output / "ace-trace-jacobian-outlet"
    outlet_marker.unlink(missing_ok=True)
    if jacobian_outlet:
        outlet_marker.touch()
    purity_marker = output / "ace-trace-state-purity"
    purity_marker.unlink(missing_ok=True)
    if state_purity:
        purity_marker.touch()
    reconstruction_marker = output / "ace-trace-constant-reconstruction"
    reconstruction_marker.unlink(missing_ok=True)
    if constant_reconstruction:
        reconstruction_marker.touch()
    env = dict(os.environ, DGD=str(installation),
               DGD_LUA_PATH=str(installation / "lib/?.lua"),
               DGD_LUA_CPATH=str(installation / "lib/?.so"))
    with (output / "trace.log").open("w") as log:
        subprocess.run([str(frozen), *manifest["command_arguments"]], cwd=output,
                       env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=60)
    if (before != state_hashes_blocking(output, manifest["snapshot"])
            or "STEP=" in (output / "trace.log").read_text()):
        raise RuntimeError("Trace changed a checkpoint or entered the iteration loop")
    cells = read_trace_blocking(output / "trace-cells.csv")
    faces = read_trace_blocking(output / "trace-inviscid.csv")
    axis = read_trace_blocking(output / "trace-axis-consistency.csv")
    wall_pipeline = output / "trace-wall-pipeline.csv"
    wall_error = None
    if wall_pipeline.exists():
        wall_error = max(abs(row["actual"] / row["expected"] - 1)
                         for row in read_trace_blocking(wall_pipeline))
    residuals = {key: max(abs(row[key]) for row in cells)
                 for key in ("mass", "momx", "momy", "energy")}
    rho = case["inflow"]["density"]
    speed = max(abs(case["inflow"]["velocity"][0]),
                math.sqrt(case["gas"]["gamma"] * case["inflow"]["pressure"] / rho))
    scales = (rho, rho * speed, rho * speed, rho * speed**2)
    normalized = max(value / scale * case["grid"]["radius"] / speed
                     for value, scale in zip(residuals.values(), scales, strict=True))
    axis_error = max(abs(row["actual"] / row["expected"] - 1) for row in axis)
    axis_velocity = max(abs(row["axis_velocity"]) for row in axis)
    result = dict(checkpoint_unchanged=True, iterations_advanced=0,
                  flow_storage=manifest.get("flow_storage", {"converted": 0}),
                  executable_sha256=hashlib.sha256(binary).hexdigest(),
                  checkpoint_hashes=before, residual_components=residuals,
                  normalized_residual=normalized,
                  shock_reconstruction_suppression=shock_suppression(cells, faces),
                  axis_regularity=dict(passed=axis_error < 1e-10 and axis_velocity == 0,
                                       cells=len(axis), relative_error=axis_error,
                                       maximum_axis_radial_velocity=axis_velocity),
                  wall_gradient_pipeline=dict(passed=wall_error is not None and wall_error < 1e-6,
                                              maximum_relative_error=wall_error),
                  spatial_accuracy_qualified=False, timing_qualified=False)
    if jacobian:
        probe = output / "trace-jacobian.csv"
        result["continuous_sensor_jacobian"] = (
            check_jacobian_blocking(probe) if probe.exists()
            else dict(passed=False, reason="No continuous-sensor derivative probe was produced"))
        result["continuous_sensor_jacobian"]["location"] = "outlet" if jacobian_outlet else "axis"
    if state_purity:
        previous = output / "state-purity-before.csv"
        current = output / "state-purity-after.csv"
        # Include gradients and residuals in the comparison of the repeated evaluation.
        read_trace_blocking(previous)
        read_trace_blocking(current)
        result["state_purity"] = dict(passed=previous.read_bytes() == current.read_bytes())
    if constant_reconstruction:
        result["constant_reconstruction"] = check_reconstruction_blocking(output)
    (output / "trace-result.json").write_text(json.dumps(result, indent=2))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--executable", required=True, type=Path)
    parser.add_argument("--installation", required=True, type=Path)
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--require-axis-regularity", action="store_true")
    parser.add_argument("--require-shock-suppression", action="store_true")
    parser.add_argument("--require-wall-gradient", action="store_true")
    parser.add_argument("--require-continuous-sensor-jacobian", action="store_true")
    parser.add_argument("--jacobian-outlet", action="store_true",
                        help="Probe an interior cell at the selected zero-normal outlet")
    parser.add_argument("--binary-flow", action="store_true",
                        help="Read copied snapshots as binary doubles, bypassing decimal parsing")
    parser.add_argument("--require-state-purity", action="store_true",
                        help="Require identical state and residual on repeated evaluation")
    parser.add_argument("--require-constant-reconstruction", action="store_true",
                        help="Require exact constant velocity and accurate affine reconstruction")
    args = parser.parse_args()
    result = trace_blocking(args.source.resolve(), args.output.resolve(),
                            args.executable.resolve(), args.installation.resolve(),
                            json.loads(args.case.read_text()),
                            jacobian=args.require_continuous_sensor_jacobian,
                            jacobian_outlet=args.jacobian_outlet, binary_flow=args.binary_flow,
                            state_purity=args.require_state_purity,
                            constant_reconstruction=args.require_constant_reconstruction)
    print(json.dumps(result))
    if args.require_axis_regularity and not result["axis_regularity"]["passed"]:
        raise RuntimeError("Eilmer failed the affine radial-velocity consistency check")
    if args.require_shock_suppression and not result["shock_reconstruction_suppression"]["passed"]:
        raise RuntimeError("Eilmer did not suppress reconstruction at flagged internal shock faces")
    if args.require_wall_gradient and not result["wall_gradient_pipeline"]["passed"]:
        raise RuntimeError("Eilmer failed the radial temperature-gradient boundary-pipeline check")
    if args.require_state_purity and not result["state_purity"]["passed"]:
        raise RuntimeError("Eilmer changed stored state or residual during repeated evaluation")
    if args.require_constant_reconstruction and not result["constant_reconstruction"]["passed"]:
        raise RuntimeError("Eilmer failed constant or affine velocity reconstruction")
    if ((args.require_continuous_sensor_jacobian or args.jacobian_outlet)
            and not result["continuous_sensor_jacobian"]["passed"]):
        raise RuntimeError("Eilmer failed the continuous-sensor derivative consistency check")


if __name__ == "__main__":
    main()
