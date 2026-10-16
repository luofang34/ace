"""Check Eilmer's actual derivative against a recomputed-sensor residual derivative."""
from array import array
import csv
import math


def check_jacobian_blocking(path):
    """Compare complex steps with centered differences at the same saved state.

    The reference explicitly recomputes the sensor at each perturbation. Both
    perturbation sizes must agree; a frozen sensor cannot pass by differentiating
    the same incomplete operator on both sides of the comparison.
    """
    groups = {}
    selected = set()
    with path.open() as stream:
        for row in csv.DictReader(stream):
            selected.add(int(row["selected"]))
            key = (float(row["epsilon"]), int(row["complex_step"]),
                   int(row["updated_sensor"]), int(float(row["sign"])))
            values = groups.setdefault(key, array("d"))
            values.extend(float(row[k]) for k in ("mass", "momx", "momy", "energy"))
    if len(selected) != 1 or len(groups) != 24:
        raise ValueError("One selected cell and complete perturbation pairs required")
    counts = {len(values) for values in groups.values()}
    if len(counts) != 1 or not next(iter(counts)):
        raise ValueError("Perturbed residuals must contain equal nonempty cell arrays")
    if any(not math.isfinite(v) for values in groups.values() for v in values):
        raise ValueError("Nonfinite Jacobian probe")

    def relative_error(candidate, reference):
        square = sum(value * value for value in reference)
        if square == 0.0:
            raise ValueError("A nonzero reference derivative is required")
        return math.sqrt(sum((a - b)**2 for a, b in zip(candidate, reference, strict=True)) / square)

    results = []
    for epsilon in sorted({key[0] for key in groups}, reverse=True):
        derivatives = {}
        for imaginary in (0, 1):
            for updated in (0, 1):
                plus, minus = (groups[epsilon, imaginary, updated, sign] for sign in (1, -1))
                scale = 2.0 * (1.0 if imaginary else epsilon)
                derivatives[imaginary, updated] = array(
                    "d", ((p - m) / scale for p, m in zip(plus, minus, strict=True)))
        reference = derivatives[0, 1]
        results.append(dict(
            epsilon=epsilon,
            actual_complex_vs_full_finite_difference=relative_error(derivatives[1, 0], reference),
            recomputed_complex_vs_full_finite_difference=relative_error(derivatives[1, 1], reference),
            actual_vs_recomputed_complex=relative_error(derivatives[1, 0], derivatives[1, 1]),
            components={name: dict(
                actual_complex_vs_full_finite_difference=relative_error(
                    derivatives[1, 0][i::4], reference[i::4]),
                recomputed_complex_vs_full_finite_difference=relative_error(
                    derivatives[1, 1][i::4], reference[i::4]),
                actual_vs_recomputed_complex=relative_error(
                    derivatives[1, 0][i::4], derivatives[1, 1][i::4]))
                for i, name in enumerate(("mass", "momx", "momy", "energy"))}))
    passed = all(component["actual_complex_vs_full_finite_difference"] < 1e-6
                 and component["recomputed_complex_vs_full_finite_difference"] < 1e-6
                 and component["actual_vs_recomputed_complex"] < 1e-12
                 for row in results[1:] for component in row["components"].values())
    return dict(passed=passed, selected_cell=next(iter(selected)), perturbations=results,
                scope="one cell momentum direction; local derivative consistency, not spatial convergence")
