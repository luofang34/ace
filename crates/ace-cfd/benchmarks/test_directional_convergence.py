"""Exercise error cancellation with analytic two-coordinate truncation errors."""
import unittest

from convergence import richardson
from directional_convergence import compare_tensor_reference, tensor_refinement


def family(amplitude=0.04):
    return [dict(radial_cells=nr, angular_cells=nt, converged=True,
                 nose_heat_flux=1 + amplitude / nr**2 - amplitude / nt**2 + 0.01 / (nr * nt)**2,
                 integrated_heating=2 * (1 + amplitude / nr**2 - amplitude / nt**2
                                        + 0.01 / (nr * nt)**2))
            for nr in (1, 2, 4) for nt in (1, 2, 4)]


class DirectionalRefinementTests(unittest.TestCase):
    def test_cancelling_errors_require_additional_directional_evidence(self):
        runs = family()
        diagonal = [r["nose_heat_flux"] for r in runs if r["radial_cells"] == r["angular_cells"]]
        self.assertFalse(richardson(*diagonal)["qualified"])
        result = tensor_refinement(runs)
        self.assertTrue(result["qualified"])
        estimate = result["quantities"]["nose_heat_flux"]
        self.assertAlmostEqual(estimate["extrapolated"], 1.0)
        self.assertGreater(estimate["estimated_relative_bound"], 0.006)

    def test_large_hidden_errors_cannot_pass_by_cancellation(self):
        self.assertFalse(tensor_refinement(family(1.0))["qualified"])

    def test_unqualified_directional_orders_remain_rejected(self):
        runs = family()
        for run in runs:
            run["nose_heat_flux"] = 1 + 0.01 / run["radial_cells"]**4 + 0.02 / run["angular_cells"]**2
        self.assertFalse(tensor_refinement(runs)["qualified"])

    def test_cross_code_and_profile_gates_are_preserved(self):
        candidate = family()
        reference = family()
        for run in candidate + reference:
            run["nose_heat_flux"] += 0.02 / run["angular_cells"]**2
            run["integrated_heating"] += 0.04 / run["angular_cells"]**2
        profile = dict(wall_profile_l2=0.01, wall_profile_linf=0.01)
        self.assertTrue(compare_tensor_reference(candidate, reference, profile)["qualified"])
        self.assertFalse(compare_tensor_reference(family(1.0), reference, profile)["qualified"])
        profile["wall_profile_linf"] = 0.11
        self.assertFalse(compare_tensor_reference(candidate, reference, profile)["qualified"])
        profile["wall_profile_linf"] = 0.01
        for run in reference:
            run["nose_heat_flux"] *= 1.2
        self.assertFalse(compare_tensor_reference(candidate, reference, profile)["qualified"])

    def test_directional_success_cannot_bypass_isotropic_rejection(self):
        profile = dict(wall_profile_l2=0.0, wall_profile_linf=0.0)
        self.assertTrue(tensor_refinement(family())["qualified"])
        self.assertFalse(compare_tensor_reference(family(), family(), profile)["qualified"])

    def test_reference_requires_its_own_complete_tensor_study(self):
        diagonal = [r for r in family() if r["radial_cells"] == r["angular_cells"]]
        with self.assertRaises(ValueError):
            compare_tensor_reference(family(), diagonal, dict(wall_profile_l2=0.0, wall_profile_linf=0.0))

    def test_oscillating_direction_is_rejected(self):
        runs = family()
        runs[7]["nose_heat_flux"] += 0.1
        self.assertFalse(tensor_refinement(runs)["qualified"])

    def test_all_nine_runs_must_converge(self):
        runs = family()
        runs[0]["converged"] = False
        self.assertFalse(tensor_refinement(runs)["qualified"])

    def test_incomplete_or_duplicate_grid_families_are_rejected(self):
        with self.assertRaises(ValueError):
            tensor_refinement(family()[:-1])
        runs = family()
        runs[0] = runs[1]
        with self.assertRaises(ValueError):
            tensor_refinement(runs)


if __name__ == "__main__":
    unittest.main()
