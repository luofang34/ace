"""Solve nine grids to separate radial and angular wall-load uncertainty."""
import argparse
import hashlib
import json
import os
from pathlib import Path

from compare import execute, freeze, wall_metrics
from directional_convergence import tensor_refinement


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--case", type=Path, default=Path(__file__).parents[1] / "cases/sphere-viscous.json")
    parser.add_argument("--resolutions", nargs=3, type=int, default=[48, 96, 192])
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    executable = freeze(args.rust.resolve(), root / "sphere-executable")
    template = json.loads(args.case.read_text())
    if template.get("mesh_vertices") is not None:
        raise ValueError("A tensor study needs a grid generator, not one fixed vertex array")
    report = {"qualified": False, "runs": [], "timing_qualified": False,
              "rust_sha256": hashlib.sha256(executable.read_bytes()).hexdigest()}
    for nr in args.resolutions:
        for nt in args.resolutions:
            directory = root / f"{nr}x{nt}"
            directory.mkdir()
            case = json.loads(json.dumps(template))
            case["grid"].update(radial_cells=nr, angular_cells=nt)
            (directory / "input.json").write_text(json.dumps(case, indent=2))
            execute(directory, "run", [str(executable), "input.json", "result.json"], os.environ)
            solution = json.loads((directory / "result.json").read_text())["solution"]
            row = dict(radial_cells=nr, angular_cells=nt, converged=solution["converged"],
                       **wall_metrics(solution["wall"]))
            report["runs"].append(row)
            (root / "results.json").write_text(json.dumps(report, indent=2))
            print(json.dumps(row), flush=True)
    report["refinement"] = tensor_refinement(report["runs"])
    report["qualified"] = report["refinement"]["qualified"]
    (root / "results.json").write_text(json.dumps(report, indent=2))
    if not report["qualified"]:
        raise RuntimeError("Directional grid uncertainty did not qualify; inspect results.json")


if __name__ == "__main__":
    main()
