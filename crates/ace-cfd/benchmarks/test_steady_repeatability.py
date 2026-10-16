import unittest

from steady_repeatability import compare_walls


class SteadyRepeatabilityTests(unittest.TestCase):
    def test_identical_integrals_do_not_hide_different_local_steady_heat_fluxes(self):
        base = [{"theta": 0.2, "area": 1.0, "heat_flux": 100.0},
                {"theta": 0.8, "area": 1.0, "heat_flux": 100.0}]
        other = [dict(row, heat_flux=q) for row, q in zip(base, [101.0, 99.0])]
        result = compare_walls([base, other], [True, True], 1e-5)
        self.assertFalse(result["qualified"])
        self.assertEqual(result["quantities"][0]["integrated_heating"],
                         result["quantities"][1]["integrated_heating"])
        self.assertAlmostEqual(result["wall_differences"][0]["wall_profile_linf"], 0.01)
        self.assertTrue(compare_walls([base, base], [True, True], 1e-5)["qualified"])
        self.assertFalse(compare_walls([base, base], [True, False], 1e-5)["qualified"])

    def test_changed_grids_and_nonfinite_heat_are_rejected(self):
        base = [{"theta": 0.2, "area": 1.0, "heat_flux": 100.0},
                {"theta": 0.8, "area": 1.0, "heat_flux": 100.0}]
        for key, value in [("theta", 0.3), ("area", 2.0), ("heat_flux", float("nan"))]:
            other = [dict(row) for row in base]
            other[0][key] = value
            with self.assertRaises(ValueError):
                compare_walls([base, other], [True, True], 1e-5)


if __name__ == "__main__":
    unittest.main()
