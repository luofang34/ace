"""Guard against mistaking cancellation of signed functional errors for high accuracy."""
import unittest

from heat_convergence import diagnose, heat_changes


class HeatConvergenceTests(unittest.TestCase):
    def test_opposite_second_order_errors_do_not_qualify_a_fourth_order_integral(self):
        def wall(h):
            return [{"theta": 0.2, "area": 1.0, "heat_flux": 10 - h*h + h**4},
                    {"theta": 0.8, "area": 1.0, "heat_flux": 10 + h*h + h**4}]
        result = diagnose([wall(h) for h in (0.2, 0.1, 0.05)], sectors=2)
        self.assertFalse(result["integral_richardson"]["qualified"])
        self.assertAlmostEqual(result["integral_richardson"]["order"], 4.0, places=8)
        self.assertAlmostEqual(result["absolute_change_order"], 2.0, places=8)
        self.assertGreater(result["changes"][1]["cancellation_fraction"], 0.98)
        self.assertFalse(result["timing_qualified"])

    def test_nested_sums_preserve_total_power_without_interpolation(self):
        coarse = [{"theta": 0.2, "area": 2.0, "heat_flux": 3.0},
                  {"theta": 0.8, "area": 2.0, "heat_flux": 1.0}]
        fine = [{"theta": i/4, "area": 1.0, "heat_flux": q}
                for i, q in enumerate([2.0, 3.0, 1.0, 4.0])]
        result = heat_changes(coarse, fine, sectors=2)
        self.assertEqual(result["sector_changes"], [-1.0, 3.0])
        self.assertEqual(result["net_change"], 2.0)
        self.assertEqual(result["absolute_change"], 4.0)
        self.assertEqual(result["cancellation_fraction"], 0.5)
        with self.assertRaises(ValueError):
            heat_changes(coarse, fine[:-1], sectors=2)


if __name__ == "__main__":
    unittest.main()
