"""Configure and measure Eilmer's serial Newton–Krylov steady solver."""
import hashlib
import json
import math
import re
import subprocess
import time

from eilmer_snapshot import validate_snapshot_blocking
from saved_state_residual import RELATIVE_RESIDUAL_TARGET, verify_saved_state_blocking


def configure(directory):
    with (directory / "sphere.lua").open("a") as stream:
        stream.write("""
config.extrema_clipping = false
config.flux_calculator = 'ausmdv'
config.do_shock_detect = true
SteadyStateSolver{
 precondition_matrix_type='ilu', ilu_fill=0, frozen_preconditioner_count=20,
 use_scaling=true, use_complex_matvec_eval=true, use_physicality_check=true,
 number_total_steps=3000, stop_on_relative_global_residual=$relative_target,
 max_outer_iterations=50, max_restarts=1,
 residual_based_cfl_scheduling=true, cfl_max=1.0e5,
 number_start_up_steps=50,
 cfl0=0.5, eta0=0.1, tau0=0.1, p0=0.7, sigma0=1.0e-30,
 cfl1=1.0, eta1=0.1, tau1=0.1, p1=0.7, sigma1=1.0e-30,
 snapshots_count=100, number_total_snapshots=30,
 write_diagnostics_count=1, write_loads_count=1
}
""".replace("$relative_target", str(RELATIVE_RESIDUAL_TARGET)))


def run(directory, executable, case, env, read_wall):
    start = time.perf_counter()
    with (directory / "run.log").open("w") as stream:
        result = subprocess.run([str(executable), "--job=sphere", "--max-cpus=1",
                                 "--max-wall-clock=1800", "--verbosity=1"],
                                cwd=directory, env=env, stdout=stream, stderr=subprocess.STDOUT)
    elapsed = time.perf_counter() - start
    if result.returncode:
        raise RuntimeError(f"Eilmer Newton–Krylov failed: {directory}/run.log")
    saved_state = verify_saved_state_blocking(directory, executable, case, env)
    return measure_blocking(directory, case, read_wall, elapsed, saved_state=saved_state)


def measure_blocking(directory, case, read_wall, elapsed, *, saved_state=None):
    """Recheck saved stopping evidence and three distinct wall-load increments."""
    log = (directory / "run.log").read_text()
    steps = re.findall(r"STEP=\s+(\d+)", log)
    residuals = {}
    for name in ("mass", "x-mom", "y-mom", "total-energy"):
        matches = re.findall(rf"^\s+{name}\s+([\deE+.\-]+)\s+", log, re.MULTILINE)
        residuals[name] = float(matches[-1]) if matches else math.inf
    rho = case["inflow"]["density"]
    speed = max(abs(case["inflow"]["velocity"][0]),
                math.sqrt(case["gas"]["gamma"] * case["inflow"]["pressure"] / rho))
    scales = (rho, rho * speed, rho * speed, rho * speed**2)
    normalized = max(value / scale * case["grid"]["radius"] / speed
                     for value, scale in zip(residuals.values(), scales, strict=True))
    files = sorted(directory.glob("loads/t*/*.loads.dat"))
    if len(files) < 4:
        raise RuntimeError(f"Missing wall-load convergence evidence in {directory}")
    walls = [read_wall(path) for path in files[-4:]]
    wall = walls[-1]
    peak = max(abs(row["heat_flux"]) for row in wall)
    change = max(abs(a["heat_flux"] - b["heat_flux"]) / peak
                 for previous, current in zip(walls[:-1], walls[1:], strict=True)
                 for a, b in zip(current, previous, strict=True))
    reference = re.findall(r"^GLOBAL:\s+([\deE+.\-]+)", log, re.MULTILINE)
    globals_ = re.findall(r"^\s+global\s+([\deE+.\-]+)\s+([\deE+.\-]+)", log, re.MULTILINE)
    global_absolute, global_relative = map(float, globals_[-1]) if globals_ else (math.inf, math.inf)
    norm_reference = float(reference[-1]) if reference else math.inf
    restart = directory / "restart-input.json"
    reference_bytes = (directory / "sphere-ref-residuals.saved").read_bytes()
    reference_hash = hashlib.sha256(reference_bytes).hexdigest()
    reference_preserved = (not restart.exists() or reference_hash ==
                           json.loads(restart.read_text()).get("reference_sha256"))
    reference_preserved = reference_preserved and math.isclose(
        norm_reference, float(reference_bytes.split()[0]), rel_tol=1e-11)
    control_path = directory / "config/sphere.control"
    control = json.loads(control_path.read_text()) if control_path.exists() else {}
    configured_target = control.get("steady_state_solver_options", {}).get(
        "stop_on_relative_global_residual", math.inf)
    rows = [line.split() for line in (directory / "config/sphere.times").read_text().splitlines()
            if line.strip() and not line.startswith("#")]
    latest = max(rows, key=lambda row: int(row[4]))
    payload = validate_snapshot_blocking(directory, int(latest[0]), float(latest[1]))
    last_step = int(steps[-1]) if steps else 0
    snapshot_matches_step = int(latest[4]) == last_step
    iteration_converged = (
        snapshot_matches_step and "relative global residual is below target" in log
        and math.isfinite(norm_reference) and reference_preserved
        and 0 < configured_target <= RELATIVE_RESIDUAL_TARGET
        and global_relative <= RELATIVE_RESIDUAL_TARGET
        and normalized < case["solver"]["residual_tolerance"]
        and change < case["solver"]["heat_flux_tolerance"])
    saved_state_qualified = (saved_state is not None and saved_state["qualified"]
                             and saved_state["reference_sha256"] == reference_hash
                             and all(saved_state["checkpoint_hashes"].get(item["path"]) == item["sha256"]
                                     for item in payload["files"]))
    evidence = dict(converged=iteration_converged and saved_state_qualified,
                    iteration_converged=iteration_converged,
                    saved_state_verification=saved_state, wall_seconds=elapsed,
                    steps=last_step, snapshot_matches_step=snapshot_matches_step,
                    snapshot_payload=payload,
                    normalized_residual=normalized, wall_change=change,
                    global_residual=global_absolute, relative_global_residual=global_relative,
                    reference_residual=norm_reference, reference_preserved=reference_preserved,
                    configured_relative_target=configured_target,
                    residual_components=residuals)
    (directory / "convergence.json").write_text(json.dumps(evidence, indent=2))
    return evidence
