"""Verify solved spherical-shell conduction against its analytic heat transfer."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path

from compare import execute, freeze


def case(resolution):
    """A motionless gas with fixed temperatures on concentric spherical boundaries."""
    return {
        "grid": dict(radius=0.01, outer_radius=0.02, radial_cells=resolution,
                     angular_cells=resolution, stretching=0.0),
        "gas": dict(gamma=1.4, gas_constant=287.05, viscosity=1.8e-5, conductivity=1.0),
        "inflow": dict(density=1000.0 / (287.05 * 300.0), pressure=1000.0,
                       velocity=[0.0, 0.0, 0.0]),
        "wall_temperature": 400.0,
        "convection": "ausmdv",
        "solver": dict(cfl=0.5, max_steps=1000000, first_order_steps=0, check_interval=200,
                       residual_tolerance=1e-8, heat_flux_tolerance=1e-7),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--curved-geometry", action="store_true")
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    executable = freeze(args.rust.resolve(), root / "sphere-executable")
    report = {"qualified": False, "runs": [], "curved_geometry": args.curved_geometry,
              "rust_sha256": hashlib.sha256(executable.read_bytes()).hexdigest()}
    for n in (8, 16, 32):
        directory = root / str(n)
        directory.mkdir()
        data = case(n)
        data["curved_geometry"] = args.curved_geometry
        (directory / "input.json").write_text(json.dumps(data, indent=2))
        execute(directory, "run", [str(executable), "input.json", "result.json"], os.environ)
        solution = json.loads((directory / "result.json").read_text())["solution"]
        wall = solution["wall"]
        average = sum(w["heat_flux"] * w["area"] for w in wall) / sum(w["area"] for w in wall)
        radius, outer = data["grid"]["radius"], data["grid"]["outer_radius"]
        exact = (data["gas"]["conductivity"] * (300.0 - data["wall_temperature"])
                 / (radius * (1.0 - radius / outer)))
        row = dict(resolution=n, converged=solution["converged"], steps=solution["steps"],
                   average_heat_flux=average, exact_heat_flux=exact,
                   relative_error=abs(average / exact - 1.0), last_check=solution["history"][-1])
        report["runs"].append(row)
        (root / "results.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(row), flush=True)
    errors = [r["relative_error"] for r in report["runs"]]
    report["qualified"] = (all(r["converged"] for r in report["runs"])
                           and all(math.isfinite(e) and e > 0.0 for e in errors)
                           and errors[2] < 0.005
                           and errors[0] / errors[1] > 3.0 and errors[1] / errors[2] > 3.0)
    (root / "results.json").write_text(json.dumps(report, indent=2))
    if not report["qualified"]:
        raise RuntimeError("Solved conduction did not meet analytic-error and refinement gates")


if __name__ == "__main__":
    main()
