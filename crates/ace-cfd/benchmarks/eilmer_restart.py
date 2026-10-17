"""Prepare a snapshot restart that retains Eilmer's original residual reference."""
import hashlib
import json
import math
from pathlib import Path
import shutil

from eilmer_snapshot import validate_snapshot_blocking


def prepare_restart_blocking(source, destination, cfl_schedule=(), *, binary_flow=False):
    """Copy saved solver state, never reinitialize it through FlowSolution/prep.

    Optional (step, CFL) pairs change the iteration schedule only. Stopping
    tolerances retain their values; the saved reference file is copied without modification.
    """
    source, destination = Path(source), Path(destination)
    reference = source / "sphere-ref-residuals.saved"
    reference_bytes = reference.read_bytes()
    norm = float(reference_bytes.split()[0])
    if not math.isfinite(norm) or norm <= 0:
        raise ValueError("Positive finite original residual reference required")
    rows = [line.split() for line in (source / "config/sphere.times").read_text().splitlines()
            if line.strip() and not line.startswith("#")]
    # Snapshot slots can be reused; the largest saved step identifies the newest state.
    latest = max(rows, key=lambda row: int(row[4]))
    snapshot = int(latest[0])
    step = int(latest[4])
    if snapshot <= 0 or not (source / f"flow/t{snapshot:04d}").is_dir():
        raise ValueError("A saved noninitial flow snapshot is required")
    payload = validate_snapshot_blocking(source, snapshot, float(latest[1]))
    schedule = list(cfl_schedule)
    if any(i <= step or not math.isfinite(cfl) or cfl <= 0 for i, cfl in schedule):
        raise ValueError("CFL changes must be positive and follow the saved step")
    if any(b[0] <= a[0] for a, b in zip(schedule, schedule[1:])):
        raise ValueError("CFL schedule steps must be strictly increasing")
    # Copying the complete directory keeps grid, history, loads and time indices consistent.
    shutil.copytree(source, destination)
    copied_payload = validate_snapshot_blocking(destination, snapshot, float(latest[1]))
    if (copied_payload != payload
            or (destination / "sphere-ref-residuals.saved").read_bytes() != reference_bytes):
        raise RuntimeError("Restart inputs changed while the snapshot was being copied")
    # Derived results describe the source run, not this new execution.
    for name in ("convergence.json", "results.json", "metrics.json", "trace-result.json",
                 "repeatability.json", "ace-trace-only", "ace-trace-jacobian",
                 "ace-trace-jacobian-outlet", "ace-trace-state-purity",
                 "ace-trace-constant-reconstruction"):
        (destination / name).unlink(missing_ok=True)
    log = destination / "run.log"
    if log.exists():
        log.rename(destination / "restart-parent.log")
    control_path = destination / "config/sphere.control"
    control = json.loads(control_path.read_text())
    control["halt_now"] = 0
    if schedule:
        options = control["steady_state_solver_options"]
        options.update(residual_based_cfl_scheduling=False, cfl_schedule_length=len(schedule),
                       cfl_schedule_iter_list=[i for i, _ in schedule],
                       cfl_schedule_value_list=[cfl for _, cfl in schedule])
    control_path.write_text(json.dumps(control, indent=2))
    manifest = dict(source=str(source.resolve()), snapshot=snapshot, step=step,
                    reference_residual=norm, snapshot_payload=payload,
                    reference_sha256=hashlib.sha256(reference_bytes).hexdigest(),
                    cfl_schedule=schedule,
                    command_arguments=["--job=sphere", "--max-cpus=1", f"--snapshot-start={snapshot}"])
    if binary_flow:
        from eilmer_binary import convert_restart_blocking
        manifest["flow_storage"] = convert_restart_blocking(destination)
    (destination / "restart-input.json").write_text(json.dumps(manifest, indent=2))
    return manifest
