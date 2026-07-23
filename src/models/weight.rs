use crate::domain::diagnostic::AexError;

/// Inputs to the bounded conceptual takeoff-mass closure.
#[derive(Debug, Clone, Copy)]
pub struct WeightClosureInput {
    /// Payload mass in kilograms.
    pub payload_kg: f64,
    /// Mission and reserve fuel mass in kilograms.
    pub fuel_kg: f64,
    /// Coefficient in the fractional empty-mass model.
    pub empty_coefficient: f64,
    /// Exponent in the fractional empty-mass model.
    pub empty_exponent: f64,
    /// Lower solver bracket in kilograms.
    pub lower_mass_kg: f64,
    /// Upper solver bracket in kilograms.
    pub upper_mass_kg: f64,
}

/// Converged takeoff mass and the deterministic solver trace.
#[derive(Debug, Clone)]
pub struct WeightClosureResult {
    /// Converged takeoff mass in kilograms.
    pub takeoff_mass_kg: f64,
    /// Final scalar residual in kilograms.
    pub residual_kg: f64,
    /// Ordered `(trial mass, residual)` iteration trace.
    pub iterations: Vec<(f64, f64)>,
}

/// Solve the scalar fractional-weight closure using a bounded bisection method.
pub fn solve_weight_closure(input: WeightClosureInput) -> Result<WeightClosureResult, AexError> {
    let residual = |mass: f64| {
        input.empty_coefficient * mass.powf(input.empty_exponent) + input.payload_kg + input.fuel_kg
            - mass
    };
    let mut lower = input.lower_mass_kg;
    let mut upper = input.upper_mass_kg;
    let mut lower_value = residual(lower);
    let upper_value = residual(upper);
    if lower_value * upper_value > 0.0 {
        return Err(AexError::analysis(
            "NO_WEIGHT_CLOSURE_ROOT",
            format!("weight residual does not change sign between {lower} and {upper} kg"),
        ));
    }
    let mut trace = vec![(lower, lower_value), (upper, upper_value)];
    for _ in 0..100 {
        let midpoint = 0.5 * (lower + upper);
        let value = residual(midpoint);
        trace.push((midpoint, value));
        if value.abs() < 1.0e-6 {
            return Ok(WeightClosureResult {
                takeoff_mass_kg: midpoint,
                residual_kg: value,
                iterations: trace,
            });
        }
        if lower_value * value <= 0.0 {
            upper = midpoint;
        } else {
            lower = midpoint;
            lower_value = value;
        }
    }
    Err(AexError::analysis(
        "WEIGHT_CLOSURE_NON_CONVERGENCE",
        "bounded weight-closure solver exceeded 100 iterations",
    ))
}

#[cfg(test)]
mod tests;
