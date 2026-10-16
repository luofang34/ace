"""Check whether converged runs of one discrete problem agree across initializations."""
import argparse
import json
import math
from pathlib import Path

def compare_walls(walls, converged, tolerance):
    """Small signed integrals cannot excuse differing local steady wall loads."""
    from compare import wall_difference, wall_metrics
    if len(walls) < 2 or len(walls) != len(converged):
        raise ValueError("At least two independently converged solutions are required")
    if not math.isfinite(tolerance) or tolerance <= 0:
        raise ValueError("A positive finite wall convergence tolerance is required")
    base = walls[0]
    if len(base) < 2:
        raise ValueError("At least two wall faces are required")
    for wall in walls:
        if len(wall) != len(base):
            raise ValueError("Repeatability requires identical wall grids")
        for a, b in zip(base, wall, strict=True):
            if (not all(math.isfinite(b[key]) for key in ("theta", "area", "heat_flux"))
                    or b["area"] <= 0
                    or any(not math.isclose(a[key], b[key], rel_tol=1e-12, abs_tol=1e-15)
                           for key in ("theta", "area"))):
                raise ValueError("Finite wall data on identical grids are required")
    differences = [wall_difference(wall, base) for wall in walls[1:]]
    return dict(qualified=all(converged) and all(
        difference["wall_profile_linf"] <= tolerance for difference in differences),
        iterative_convergence=all(converged), tolerance=tolerance,
        wall_differences=differences, quantities=[wall_metrics(wall) for wall in walls],
        timing_qualified=False)


def repeat_eilmer_blocking(directory, case, vertices, prior, shared, executable, env):
    """Use a converged coarse-grid initialization to check the finest cold-start result."""
    from compare import execute, load_wall
    from eilmer_case import prepare
    import eilmer_nk
    prepare(directory, case, case["solver"]["max_steps"], vertices)
    eilmer_nk.configure(directory)
    lua = directory / "sphere.lua"
    source = lua.read_text().replace("block = FluidBlock:new",
        "initial = FlowSolution:new{jobName='sphere',dir="
        + json.dumps(str(prior.resolve()))
        + ",tindx='last',nBlocks=1}\nblock = FluidBlock:new")
    lua.write_text(source.replace("initialState=inflow", "initialState=initial"))
    execute(directory, "prep", [str(shared), "--prep", "--job=sphere"], env)
    evidence = eilmer_nk.run(directory, executable, case, env, load_wall)
    wall = load_wall(sorted(directory.glob("loads/t*/*.loads.dat"))[-1])
    return evidence, wall


def main():
    from compare import load_wall
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directories", nargs="+", required=True, type=Path)
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    # Iteration controls may differ; the spatial operator and mesh must not.
    configurations = [json.loads((d / "config/sphere.config").read_text())
                      for d in args.directories]
    grids = [(d / "mesh.vtk").read_bytes() for d in args.directories]
    gases = [(d / "gas.lua").read_bytes() for d in args.directories]
    if any(config != configurations[0] for config in configurations[1:]) or any(
            grid != grids[0] for grid in grids[1:]) or any(gas != gases[0] for gas in gases[1:]):
        raise ValueError("Identical Eilmer spatial configurations, gas data and meshes required")
    walls = [load_wall(sorted(d.glob("loads/t*/*.loads.dat"))[-1]) for d in args.directories]
    converged = [json.loads((d / "convergence.json").read_text())["converged"]
                 for d in args.directories]
    tolerance = json.loads(args.case.read_text())["solver"]["heat_flux_tolerance"]
    result = compare_walls(walls, converged, tolerance)
    result["sources"] = [str(d.resolve()) for d in args.directories]
    args.output.write_text(json.dumps(result, indent=2))
    print(json.dumps(result))
    if not result["qualified"]:
        raise RuntimeError("Steady solutions disagree; their grid convergence cannot qualify timing")


if __name__ == "__main__":
    main()
