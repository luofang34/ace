"""Run serial forebody refinements; retain raw results and reject unconverged comparisons."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

from eilmer_case import prepare
from eilmer_run import run as run_eilmer
from convergence import compare_refinement, time_to_tolerance
from sphere_grid import nested_case
from steady_repeatability import compare_walls, repeat_eilmer_blocking
import eilmer_nk


def execute(directory, name, arguments, env):
    start = time.perf_counter()
    with (directory / f"{name}.log").open("w") as log:
        result = subprocess.run(arguments, cwd=directory, env=env, stdout=log, stderr=subprocess.STDOUT)
    seconds = time.perf_counter() - start
    if result.returncode:
        raise RuntimeError(f"{name} exited {result.returncode}; inspect {directory}/{name}.log")
    return seconds


def freeze(executable, destination):
    shutil.copyfile(executable, destination)
    destination.chmod(0o755)
    return destination


def load_wall(path):
    lines = path.read_text().splitlines()
    columns = {name: int(index) - 1 for index, name in re.findall(r"(\d+):([^\s]+)", lines[1])}
    rows = []
    for line in lines[2:]:
        if not line.strip() or line.startswith("#"):
            continue
        values = list(map(float, line.split()))
        get = lambda key: values[columns[key]]
        rows.append({"theta": math.atan2(get("pos.y"), -get("pos.x")),
                     "heat_flux": -get("q_total") * get("outsign"), "pressure": get("p"),
                     "area": get("area") * math.tau})
    return sorted(rows, key=lambda row: row["theta"])


def wall_metrics(wall):
    # Curved faces carry area means. Point samples (including native Eilmer
    # loads) use theta²; averages require the corresponding area moment.
    a, b = wall[:2]
    x, y = (w.get("theta_squared_mean", w["theta"]**2) for w in (a, b))
    nose = (a["heat_flux"] * y - b["heat_flux"] * x) / (y - x)
    return {"nose_heat_flux": nose, "integrated_heating": sum(w["heat_flux"] * w["area"] for w in wall)}


def wall_difference(candidate, reference):
    pairs = list(zip(candidate, reference, strict=True))
    square = sum((a["heat_flux"] - b["heat_flux"])**2 * a["area"] for a, b in pairs)
    scale = sum(b["heat_flux"]**2 * a["area"] for a, b in pairs)
    peak = max(abs(b["heat_flux"]) for b in reference)
    if scale == 0 or peak == 0:
        raise ValueError("Nonzero reference wall heating required for relative comparison")
    return {"wall_profile_l2": math.sqrt(square / scale),
            "wall_profile_linf": max(abs(a["heat_flux"] - b["heat_flux"]) for a, b in pairs) / peak}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust", required=True, type=Path)
    parser.add_argument("--eilmer", required=True, type=Path)
    parser.add_argument("--eilmer-nk", type=Path, help="Use this Newton–Krylov executable")
    parser.add_argument("--eilmer-outlet", choices=("native", "zero-normal-gradient"),
                        default="native", help="Select the reference viscous outlet condition")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--resolutions", nargs="+", type=int, default=[48, 96, 192])
    parser.add_argument("--steps", type=int, default=200000)
    parser.add_argument("--verification-only", action="store_true",
                        help="Check accuracy without qualifying a timing comparison")
    parser.add_argument("--case", type=Path, default=Path(__file__).resolve().parents[1] / "cases/sphere-viscous.json")
    args = parser.parse_args()
    args.rust, args.eilmer, args.output = args.rust.resolve(), args.eilmer.resolve(), args.output.resolve()
    if args.eilmer_nk:
        args.eilmer_nk = args.eilmer_nk.resolve()
    template = json.loads(args.case.read_text())
    if (args.eilmer_outlet == "zero-normal-gradient"
            and template.get("outflow", "zero_gradient") != "zero_gradient_cell_flux"):
        raise ValueError("The zero-normal Eilmer comparison requires Rust outflow=zero_gradient_cell_flux")
    cases = [nested_case(template, n, n) for n in args.resolutions]
    args.output.mkdir(parents=True, exist_ok=False)
    args.rust = freeze(args.rust, args.output / "sphere-executable")
    shared = freeze(args.eilmer / "bin/e4shared", args.output / "eilmer-shared-executable")
    # Eilmer locates its Lua entry scripts relative to the executable.
    for script in (args.eilmer / "bin").glob("*.lua"):
        shutil.copyfile(script, args.output / script.name)
    if args.eilmer_nk:
        args.eilmer_nk = freeze(args.eilmer_nk, args.output / "eilmer-nk-executable")
    env = dict(os.environ, DGD=str(args.eilmer), DGD_LUA_PATH=str(args.eilmer / "lib/?.lua"),
               DGD_LUA_CPATH=str(args.eilmer / "lib/?.so"))
    env["PATH"] = str(args.eilmer / "bin") + os.pathsep + env.get("PATH", "")
    report = {"machine": list(os.uname()), "runs": [], "comparison_qualified": False,
              "rust_sha256": hashlib.sha256(args.rust.read_bytes()).hexdigest(),
              "eilmer_sha256": hashlib.sha256((args.eilmer_nk or shared).read_bytes()).hexdigest(),
              "eilmer_method": "newton_krylov" if args.eilmer_nk else "explicit_local_time",
              "eilmer_outlet": args.eilmer_outlet,
              "timing_scope": "serial solver process including I/O; preparation reported separately",
              "case": template,
              "validation_scope": "constant-property ideal-gas cross-code verification, not experimental validation"}
    for n, case in zip(args.resolutions, cases, strict=True):
        case["solver"]["max_steps"] = args.steps
        rd = args.output / f"rust-{n}"
        rd.mkdir()
        (rd / "input.json").write_text(json.dumps(case, indent=2))
        wall_seconds = execute(rd, "run", [str(args.rust), "input.json", "result.json"], env)
        result = json.loads((rd / "result.json").read_text())
        solution = result["solution"]
        report["runs"].append(dict(solver="rust", resolution=n, wall_seconds=wall_seconds,
                                  total_seconds=wall_seconds,
                                  prep_seconds=result["preparation_seconds"], steps=solution["steps"],
                                  converged=solution["converged"], **wall_metrics(solution["wall"])))
        ed = args.output / f"eilmer-{n}"
        prepare(ed, case, args.steps, result.get("mesh_vertices"), outlet=args.eilmer_outlet)
        if args.eilmer_nk:
            eilmer_nk.configure(ed)
        prep = execute(ed, "prep", [str(shared), "--prep", "--job=sphere"], env)
        convergence = (eilmer_nk.run(ed, args.eilmer_nk, case, env, load_wall) if args.eilmer_nk
                       else run_eilmer(ed, shared, case, env, load_wall))
        files = sorted(ed.glob("loads/t*/*.loads.dat"))
        if len(files) < 4:
            raise RuntimeError(f"Insufficient wall snapshots in {ed}")
        wall = load_wall(files[-1])
        report["runs"][-1].update(wall_difference(solution["wall"], wall))
        peak = max(abs(w["heat_flux"]) for w in wall)
        changes = [max(abs(a["heat_flux"] - b["heat_flux"]) / peak
                       for a, b in zip(wall, load_wall(path), strict=True)) for path in files[-4:-1]]
        report["runs"].append(dict(solver="eilmer", resolution=n, wall_seconds=convergence["wall_seconds"],
                                  total_seconds=prep + convergence["wall_seconds"],
                                  prep_seconds=prep, steps=convergence["steps"], wall_change=convergence.get("wall_change", max(changes)),
                                  converged=convergence["converged"],
                                  thermal_stationary=convergence.get("wall_change", max(changes)) < template["solver"]["heat_flux_tolerance"],
                                  **wall_metrics(wall)))
        (args.output / "results.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(report["runs"][-2:]), flush=True)
    report["refinement"] = compare_refinement(report["runs"])
    report["refinement"]["boundary_verification"] = {
        "qualified": False,
        "reason": (("Native Eilmer SimpleFlux copies interior cell viscous gradients; "
                   "Rust outlet modes do not impose the same viscous boundary condition. "
                   "A controlled common boundary condition must be verified before timing.")
                   if args.eilmer_outlet == "native" else
                   ("Zero-normal outlet selection alone does not verify the executing operators. "
                    "Both executables require independent fixed-state boundary verification before timing."))}
    if args.eilmer_nk:
        repeat, repeat_wall = repeat_eilmer_blocking(
            args.output / "eilmer-repeat", case, result.get("mesh_vertices"),
            args.output / f"eilmer-{args.resolutions[0]}", shared, args.eilmer_nk, env,
            outlet=args.eilmer_outlet)
        report["reference_repeatability"] = compare_walls(
            [wall, repeat_wall], [convergence["converged"], repeat["converged"]],
            case["solver"]["heat_flux_tolerance"])
        report["reference_repeatability"]["repeat_run"] = repeat
        report["refinement"]["qualified"] = (report["refinement"]["qualified"]
            and report["reference_repeatability"]["qualified"])
    report["comparison_qualified"] = (report["refinement"]["qualified"]
        and report["refinement"]["boundary_verification"]["qualified"])
    report["time_to_tolerance"] = (
        {"qualified": False, "reason": "verification-only run"} if args.verification_only
        else time_to_tolerance(report["runs"], report["refinement"]))
    (args.output / "results.json").write_text(json.dumps(report, indent=2))
    if not report["comparison_qualified"]:
        raise RuntimeError("Accuracy gates did not qualify a same-accuracy timing comparison; inspect results.json")


if __name__ == "__main__":
    main()
