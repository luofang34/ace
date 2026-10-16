"""Exercise error-estimation decisions, including failure to reach the asymptotic range."""
import unittest
import math
from convergence import richardson, time_to_tolerance
from compare import wall_difference, wall_metrics


class RichardsonTests(unittest.TestCase):
    def test_second_order_sequence(self):
        result = richardson(1.16, 1.04, 1.01)
        self.assertTrue(result["qualified"])
        self.assertAlmostEqual(result["order"], 2.0)
        self.assertAlmostEqual(result["extrapolated"], 1.0)

    def test_oscillations_and_divergence_are_rejected(self):
        self.assertFalse(richardson(1.1, 0.95, 1.02)["qualified"])
        self.assertFalse(richardson(1.1, 1.2, 1.4)["qualified"])

    def test_false_high_order_is_rejected(self):
        self.assertFalse(richardson(3.0, 1.02, 1.01)["qualified"])

    def test_nonfinite_input_is_rejected(self):
        with self.assertRaises(ValueError):
            richardson(1.0, float("nan"), 1.0)

    def test_wall_profile_detects_errors_hidden_by_an_integral(self):
        reference = [{"heat_flux": 1.0, "area": 1.0}] * 2
        candidate = [{"heat_flux": 0.0, "area": 1.0}, {"heat_flux": 2.0, "area": 1.0}]
        difference = wall_difference(candidate, reference)
        self.assertEqual(difference["wall_profile_l2"], 1.0)
        self.assertEqual(difference["wall_profile_linf"], 1.0)

    def test_axis_extrapolation_uses_area_moments_for_averaged_wall_fluxes(self):
        wall = []
        for lo, hi in ((0.0, 0.2), (0.2, 0.4)):
            area = math.cos(lo) - math.cos(hi)
            primitive = lambda t: -t*t*math.cos(t) + 2*t*math.sin(t) + 2*math.cos(t)
            moment = (primitive(hi) - primitive(lo)) / area
            x = -0.5 * (math.sin(hi)**2 - math.sin(lo)**2) / area
            y = ((hi-lo)/2 - (math.sin(2*hi)-math.sin(2*lo))/4) / area
            wall.append(dict(theta=math.atan2(y, -x), theta_squared_mean=moment,
                             heat_flux=10000.0 - 5000.0 * moment, area=area))
        self.assertAlmostEqual(wall_metrics(wall)["nose_heat_flux"], 10000.0, places=9)

    def test_isotropic_accuracy_alone_does_not_qualify_timing(self):
        result = time_to_tolerance([], {"qualified": True})
        self.assertFalse(result["qualified"])
        self.assertIn("directional", result["reason"])

    def test_boundary_equivalence_is_required_for_timing(self):
        refinement = {"qualified": True, "directional_verification": {"qualified": True}}
        for boundary in (None, {"qualified": False}):
            if boundary is not None:
                refinement["boundary_verification"] = boundary
            result = time_to_tolerance([], refinement)
            self.assertFalse(result["qualified"])
            self.assertIn("boundary", result["reason"])


if __name__ == "__main__":
    unittest.main()
