use super::{WeightClosureInput, solve_weight_closure};

#[test]
fn bounded_weight_closure_converges() {
    let result = solve_weight_closure(WeightClosureInput {
        payload_kg: 200.0,
        fuel_kg: 100.0,
        empty_coefficient: 0.55,
        empty_exponent: 1.0,
        lower_mass_kg: 500.0,
        upper_mass_kg: 1000.0,
    });
    assert!(result.is_ok());
    if let Ok(solution) = result {
        assert!(solution.takeoff_mass_kg > 0.0);
        assert!(solution.residual_kg.abs() < 1.0e-5);
        assert!(!solution.iterations.is_empty());
    }
}
