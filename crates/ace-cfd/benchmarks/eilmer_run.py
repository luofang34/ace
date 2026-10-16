"""Stop Eilmer on measured residual and wall-load convergence, using progress events."""
import json
import math
from pathlib import Path
import re
import subprocess
import time


def residual_measure(directory, case):
    path = directory / "config/sphere-residuals.txt"
    if not path.exists():
        return math.inf
    lines = path.read_text().splitlines()
    if len(lines) < 2:
        return math.inf
    row = list(map(float, lines[-1].split()))
    rho = case["inflow"]["density"]
    speed = max(abs(case["inflow"]["velocity"][0]),
                math.sqrt(case["gas"]["gamma"] * case["inflow"]["pressure"] / rho))
    scales = [rho, rho * speed, rho * speed, rho * speed, rho * speed * speed]
    values = [abs(value) / scale * case["grid"]["radius"] / speed
              for value, scale in zip(row[3:8], scales, strict=True)]
    return max(values) if all(math.isfinite(v) for v in values) else math.inf


def run(directory, executable, case, env, read_wall, start_index=None):
    start = time.perf_counter()
    history, previous, accepted, step = [], None, 0, 0
    previous_path = None
    interval = case["solver"]["check_interval"]
    arguments = [str(executable), "--run", "--job=sphere", "--max-cpus=1", "--report-residuals"]
    if start_index is not None:
        arguments.append(f"--tindx-start={start_index}")
    with (directory / "run.log").open("w") as log:
        with subprocess.Popen(arguments, cwd=directory, env=env, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT) as process:
            for line in process.stdout:
                log.write(line)
                log.flush()
                match = re.match(r"Step=\s+(\d+)\s+t=", line)
                if not match or int(match[1]) % interval:
                    continue
                step = int(match[1])
                files = sorted(directory.glob("loads/t*/*.loads.dat"))
                if len(files) < 2:
                    continue
                if files[-1] == previous_path:
                    continue
                previous_path = files[-1]
                wall = read_wall(files[-1])
                peak = max(abs(w["heat_flux"]) for w in wall)
                if previous is None or peak == 0:
                    previous = wall
                    continue
                change = max(abs(a["heat_flux"] - b["heat_flux"]) / peak
                             for a, b in zip(wall, previous, strict=True))
                residual = residual_measure(directory, case)
                history.append(dict(step=step, residual_linf=residual, wall_change=change))
                passed = residual < case["solver"]["residual_tolerance"] and change < case["solver"]["heat_flux_tolerance"]
                accepted = accepted + 1 if passed else 0
                previous = wall
                if accepted == 3:
                    control = directory / "config/sphere.control"
                    data = json.loads(control.read_text())
                    data["halt_now"] = 1
                    temporary = control.with_suffix(".pending")
                    temporary.write_text(json.dumps(data))
                    temporary.replace(control)
            returncode = process.wait()
    if returncode:
        raise RuntimeError(f"Eilmer failed with exit {returncode}: {directory}/run.log")
    evidence = {"converged": accepted >= 3, "history": history, "steps": step,
                "wall_seconds": time.perf_counter() - start}
    (directory / "convergence.json").write_text(json.dumps(evidence, indent=2))
    return evidence
