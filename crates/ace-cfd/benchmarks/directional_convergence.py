"""Estimate directional grid errors without cancelling radial and angular contributions."""
import math

from convergence import richardson


def tensor_refinement(runs, tolerance=0.05):
    """Require a complete 3×3 refinement family and convergent fine/medium grid lines.

    The sum of absolute directional corrections includes the mixed correction.
    Orders above the formal second order cannot reduce this uncertainty estimate.
    This additional evidence does not alter the isotropic Richardson rejection rules.
    """
    radial = sorted({run["radial_cells"] for run in runs})
    angular = sorted({run["angular_cells"] for run in runs})
    grid = {(run["radial_cells"], run["angular_cells"]): run for run in runs}
    if len(radial) != 3 or len(angular) != 3 or len(grid) != 9 or len(runs) != 9:
        raise ValueError("Nine distinct runs forming a complete 3×3 family are required")
    if any(n <= 0 for n in radial + angular):
        raise ValueError("Positive grid resolutions are required")
    ratios = [values[1] / values[0] for values in (radial, angular)]
    if any(r <= 1 or not math.isclose(r, values[2] / values[1])
           for r, values in zip(ratios, (radial, angular), strict=True)):
        raise ValueError("Each coordinate direction requires uniform refinement ratios")
    fine = grid[radial[-1], angular[-1]]
    report = {"qualified": all(run["converged"] for run in runs), "tolerance": tolerance,
              "iterative_convergence": all(run["converged"] for run in runs), "quantities": {}}
    for key in ("nose_heat_flux", "integrated_heating"):
        lines = {
            "radial_medium": richardson(*(grid[n, angular[1]][key] for n in radial), ratios[0]),
            "radial_fine": richardson(*(grid[n, angular[2]][key] for n in radial), ratios[0]),
            "angular_medium": richardson(*(grid[radial[1], n][key] for n in angular), ratios[1]),
            "angular_fine": richardson(*(grid[radial[2], n][key] for n in angular), ratios[1]),
        }
        result = {"qualified": False, "line_checks": lines}
        if all(line["qualified"] for line in lines.values()):
            orders = [min(2.0, lines[f"{direction}_medium"]["order"],
                          lines[f"{direction}_fine"]["order"]) for direction in ("radial", "angular")]
            denominators = [r**p - 1.0 for r, p in zip(ratios, orders, strict=True)]
            q, qr, qt, qrt = (fine[key], grid[radial[1], angular[2]][key],
                              grid[radial[2], angular[1]][key], grid[radial[1], angular[1]][key])
            corrections = [(q - qr) / denominators[0], (q - qt) / denominators[1],
                           (q - qr - qt + qrt) / math.prod(denominators)]
            bound = 1.25 * sum(abs(c) for c in corrections) / abs(q)
            result.update(qualified=bound <= tolerance, effective_orders=orders,
                          corrections=corrections, extrapolated=q + sum(corrections),
                          estimated_relative_bound=bound)
        report["quantities"][key] = result
        report["qualified"] = report["qualified"] and result["qualified"]
    return report


def compare_tensor_reference(candidate, reference_runs, profile, tolerance=0.05):
    """Require both solvers' directional and isotropic gates before cross-code acceptance."""
    families = {"candidate": candidate, "reference": reference_runs}
    tensors = {name: tensor_refinement(runs, tolerance) for name, runs in families.items()}
    isotropic = {}
    for name, runs in families.items():
        radial = sorted({run["radial_cells"] for run in runs})
        angular = sorted({run["angular_cells"] for run in runs})
        ratio = radial[1] / radial[0]
        if not math.isclose(ratio, angular[1] / angular[0]):
            raise ValueError("Isotropic paths require the same refinement ratio in both directions")
        grid = {(run["radial_cells"], run["angular_cells"]): run for run in runs}
        path = [grid[nr, nt] for nr, nt in zip(radial, angular, strict=True)]
        isotropic[name] = {key: richardson(*(r[key] for r in path), ratio)
                           for key in ("nose_heat_flux", "integrated_heating")}
    eligible = (all(t["qualified"] for t in tensors.values())
                and all(e["qualified"] and e["fine_gci"] <= tolerance
                        for estimates in isotropic.values() for e in estimates.values()))
    differences, isotropic_differences = {}, {}
    if eligible:
        differences = {key: abs(tensors["candidate"]["quantities"][key]["extrapolated"]
                                / tensors["reference"]["quantities"][key]["extrapolated"] - 1)
                       for key in ("nose_heat_flux", "integrated_heating")}
        isotropic_differences = {key: abs(isotropic["candidate"][key]["extrapolated"]
                                          / isotropic["reference"][key]["extrapolated"] - 1)
                                 for key in differences}
        eligible = (all(value <= tolerance for value in
                        [*differences.values(), *isotropic_differences.values()])
                    and all(math.isfinite(profile[key]) and profile[key] >= 0.0
                            for key in ("wall_profile_l2", "wall_profile_linf"))
                    and profile["wall_profile_l2"] <= tolerance
                    and profile["wall_profile_linf"] <= 2 * tolerance)
    return dict(qualified=eligible, tolerance=tolerance, candidate=tensors["candidate"],
                reference=tensors["reference"], isotropic=isotropic,
                extrapolated_relative_difference=differences,
                isotropic_extrapolated_relative_difference=isotropic_differences,
                finest_wall_profile_difference=profile, timing_qualified=False)
