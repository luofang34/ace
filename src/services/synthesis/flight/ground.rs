use super::parameters::{positive, read};
use ace_engine::{PhaseIncrement, PhaseOutput, SolverError};
use ace_model::{Quantity, Unit, VehicleState};
use std::collections::BTreeMap;

pub(super) fn evaluate(
    inputs: &BTreeMap<String, Quantity>,
    state: &VehicleState,
) -> Result<PhaseOutput, SolverError> {
    use Unit::*;
    let duration = positive(&state.conditions, "duration", Second)?;
    let mean_speed = read(&state.conditions, "mean_speed", MeterPerSecond)?;
    let altitude = read(&state.conditions, "target_altitude", Meter)?;
    let speed = read(&state.conditions, "target_speed", MeterPerSecond)?;
    let flow = read(inputs, "fuel_flow", KilogramPerSecond)?;
    let price = read(inputs, "fuel_price", DollarPerKilogram)?;
    if [mean_speed, altitude, speed, flow, price]
        .iter()
        .any(|value| *value < 0.0)
    {
        return Err(SolverError::Failed(
            "ground estimate requires nonnegative conditions and rates".into(),
        ));
    }
    let burn = flow * duration;
    Ok(PhaseOutput {
        increments: vec![PhaseIncrement {
            duration: Quantity::new(duration, Second)?,
            distance: Quantity::new(duration * mean_speed, Meter)?,
            fuel_burn: Quantity::new(burn, Kilogram)?,
            cost: Quantity::new(burn * price, Dollar)?,
            conditions: BTreeMap::from([
                ("altitude".into(), Quantity::new(altitude, Meter)?),
                ("speed".into(), Quantity::new(speed, MeterPerSecond)?),
            ]),
            details: serde_json::json!({"method":"prescribed_ground_estimate"}),
        }],
        approximate: true,
        artifacts: Vec::new(),
        details: serde_json::json!({"validity":"prescribed duration, mean ground speed, flow and terminal state; no runway, ground-force, takeoff or landing performance solution"}),
    })
}
