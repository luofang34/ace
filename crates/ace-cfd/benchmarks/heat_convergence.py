"""Diagnose signed heat-integral cancellation without granting accuracy qualification."""
import argparse
import json
import math
from pathlib import Path

from convergence import richardson


def heat_changes(coarse, fine, sectors=6):
    """Compare conservative sector sums on nested, uniformly spaced polar grids.

    Actual faceted areas are retained. This is an error-cancellation diagnostic,
    not a conversion to an analytic sphere or a pointwise interpolation.
    Equal angular counts support radial-only refinement.
    """
    if not coarse or not fine or sectors <= 0 or len(coarse) % sectors or len(fine) % len(coarse):
        raise ValueError("Nested complete polar grids and equal-width sectors are required")
    for wall in (coarse, fine):
        if any(not all(math.isfinite(w[key]) for key in ("heat_flux", "area", "theta"))
               or w["area"] <= 0 or not 0 <= w["theta"] <= math.pi/2 for w in wall):
            raise ValueError("Finite wall samples, positive areas and forebody angles are required")
        if any(b["theta"] <= a["theta"] for a, b in zip(wall, wall[1:])):
            raise ValueError("Wall samples must be ordered by increasing polar angle")
    ratio = len(fine) // len(coarse)
    coarse_power = [w["area"] * w["heat_flux"] for w in coarse]
    fine_power = [w["area"] * w["heat_flux"] for w in fine]
    changes = [math.fsum(fine_power[ratio*i:ratio*(i+1)]) - q
               for i, q in enumerate(coarse_power)]
    net = math.fsum(changes)
    absolute = math.fsum(abs(d) for d in changes)
    width = len(coarse) // sectors
    return dict(net_change=net, positive_change=math.fsum(d for d in changes if d > 0),
                negative_change=math.fsum(d for d in changes if d < 0),
                absolute_change=absolute,
                cancellation_fraction=1 - abs(net) / absolute if absolute else 0.0,
                sector_changes=[math.fsum(changes[i*width:(i+1)*width]) for i in range(sectors)])


def diagnose(walls, ratio=2.0, sectors=6):
    """Retain the original Richardson gate and expose cancellation in its input."""
    if len(walls) != 3 or not math.isfinite(ratio) or ratio <= 1:
        raise ValueError("Three uniformly refined solutions are required")
    changes = [heat_changes(a, b, sectors) for a, b in zip(walls, walls[1:])]
    totals = [math.fsum(w["area"] * w["heat_flux"] for w in wall) for wall in walls]
    absolute = [change["absolute_change"] for change in changes]
    sectors_check = []
    for sector in range(sectors):
        quantities = []
        for wall in walls:
            width = len(wall) // sectors
            quantities.append(math.fsum(w["area"] * w["heat_flux"]
                                        for w in wall[sector*width:(sector+1)*width]))
        sectors_check.append(dict(heating=quantities, richardson=richardson(*quantities, ratio)))
    return dict(diagnostic_only=True, timing_qualified=False, heating=totals,
                integral_richardson=richardson(*totals, ratio), changes=changes,
                absolute_change_order=(math.log(absolute[0] / absolute[1]) / math.log(ratio)
                                       if min(absolute) > 0 else None), sectors=sectors_check)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--solutions", required=True, type=Path, nargs=3)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    solutions = [json.loads(path.read_text())["solution"] for path in args.solutions]
    if not all(s["converged"] for s in solutions):
        raise ValueError("Iterative convergence is required before a grid study")
    report = diagnose([s["wall"] for s in solutions])
    report["sources"] = [str(path.resolve()) for path in args.solutions]
    args.output.write_text(json.dumps(report, indent=2))
    print(json.dumps({k: report[k] for k in ("integral_richardson", "absolute_change_order")}))


if __name__ == "__main__":
    main()
