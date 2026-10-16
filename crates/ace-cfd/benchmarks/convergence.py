"""Solution verification gates for monotonically refined wall quantities."""
import math


def richardson(coarse, medium, fine, ratio=2.0):
    """Reject oscillatory/non-asymptotic triplets instead of inventing an error bar."""
    a, b = coarse - medium, medium - fine
    if not all(math.isfinite(v) for v in (coarse, medium, fine, ratio)) or ratio <= 1:
        raise ValueError("Finite values and a refinement ratio above one are required")
    if a * b <= 0 or abs(a) <= abs(b) or fine == 0:
        return {"qualified": False, "reason": "non-monotonic or non-contracting refinement"}
    order = math.log(abs(a / b)) / math.log(ratio)
    if not 0.2 <= order <= 3.0:
        return {"qualified": False, "reason": "observed order outside credible asymptotic range", "order": order}
    correction = (fine - medium) / (ratio**order - 1)
    return {"qualified": True, "order": order, "extrapolated": fine + correction,
            "fine_gci": 1.25 * abs(correction / fine)}


def compare_refinement(runs, tolerance=0.05):
    evidence = {}
    for solver in ("rust", "eilmer"):
        series = sorted((r for r in runs if r["solver"] == solver), key=lambda r: r["resolution"])
        if len(series) < 3:
            return {"qualified": False, "reason": "three resolutions required for each solver"}
        coarse, medium, fine = series[-3:]
        ratio = medium["resolution"] / coarse["resolution"]
        if not math.isclose(ratio, fine["resolution"] / medium["resolution"]):
            raise ValueError("This estimator requires uniform refinement ratios")
        evidence[solver] = {key: richardson(coarse[key], medium[key], fine[key], ratio)
                            for key in ("nose_heat_flux", "integrated_heating")}
        evidence[solver]["iterative_convergence"] = all(r["converged"] for r in series[-3:])
    eligible = all(evidence[s]["iterative_convergence"] and all(
        evidence[s][q].get("qualified", False) and evidence[s][q]["fine_gci"] <= tolerance
        for q in ("nose_heat_flux", "integrated_heating")) for s in evidence)
    differences = {}
    rust_fine = max((r for r in runs if r["solver"] == "rust"), key=lambda r: r["resolution"])
    profile = {key: rust_fine.get(key) for key in ("wall_profile_l2", "wall_profile_linf")}
    if eligible:
        for key in ("nose_heat_flux", "integrated_heating"):
            reference = evidence["eilmer"][key]["extrapolated"]
            differences[key] = abs(evidence["rust"][key]["extrapolated"] / reference - 1)
        eligible = (all(v <= tolerance for v in differences.values())
                    and all(v is not None and math.isfinite(v) for v in profile.values())
                    and profile["wall_profile_l2"] <= tolerance
                    and profile["wall_profile_linf"] <= 2 * tolerance)
    return {"qualified": eligible, "tolerance": tolerance, "evidence": evidence,
            "extrapolated_relative_difference": differences, "finest_wall_profile_difference": profile}


def time_to_tolerance(runs, refinement):
    """Choose the least costly measured grid meeting the same two wall-metric bounds."""
    if not refinement["qualified"]:
        return {"qualified": False, "reason": "accuracy gates failed"}
    if not refinement.get("directional_verification", {}).get("qualified", False):
        return {"qualified": False, "reason": "both solvers require directional and isotropic verification"}
    if not refinement.get("boundary_verification", {}).get("qualified", False):
        return {"qualified": False, "reason": "equivalent physical boundary conditions must be verified"}
    selected = {}
    for solver in ("rust", "eilmer"):
        eligible = []
        extrapolated = {key: refinement["evidence"][solver][key]["extrapolated"]
                        for key in ("nose_heat_flux", "integrated_heating")}
        for run in runs:
            if run["solver"] != solver or not run["converged"]:
                continue
            bounds = {key: 1.25 * abs(run[key] - value) / abs(run[key])
                      for key, value in extrapolated.items()}
            if all(value <= refinement["tolerance"] for value in bounds.values()):
                eligible.append(dict(resolution=run["resolution"],
                                     seconds=run.get("total_seconds", run["wall_seconds"]),
                                     estimated_relative_bounds=bounds))
        if not eligible:
            return {"qualified": False, "reason": f"no measured {solver} grid meets tolerance"}
        selected[solver] = min(eligible, key=lambda run: run["seconds"])
    return {"qualified": True, "selected": selected,
            "rust_to_eilmer_seconds": selected["rust"]["seconds"] / selected["eilmer"]["seconds"],
            "scope": "one timing sample per grid; two wall-metric bounds, no repeatability claim"}
