use super::{
    external,
    parameters::{FlightModel, ground_speed, positive, read},
    powerplant::Operating,
};
use crate::domain::result::{AerodynamicState, AtmosphereState};
use crate::models::{
    aerodynamics::{self, FlightCondition},
    atmosphere::Isa1976,
    propulsion::OperatingMode,
};
use ace_engine::{PhaseIncrement, PhaseOutput, SolverError};
use ace_model::{Quantity, Unit, VehicleState};
use std::collections::BTreeMap;

pub(super) fn evaluate(
    model: &FlightModel,
    state: &VehicleState,
    control: &ace_engine::RunControl,
) -> Result<PhaseOutput, SolverError> {
    let speed = positive(&state.conditions, "speed", Unit::MeterPerSecond)?;
    let duration = read(&state.conditions, "duration", Unit::Second)?;
    let altitude = read(&state.conditions, "altitude", Unit::Meter)?;
    let ground_speed = ground_speed(&state.conditions, speed)?;
    let steps = (duration / model.maximum_step).ceil().max(1.0);
    if duration < 0.0 || steps > 100_000.0 || !(0.0..=20_000.0).contains(&altitude) {
        return Err(SolverError::Failed(
            "level-flight duration, integration size or altitude is outside the declared domain"
                .into(),
        ));
    }
    let atmosphere = Isa1976::new(0.0).evaluate(altitude).map_err(external)?;
    let mut mass = 0.0;
    for value in state.masses.values() {
        mass += value.to(Unit::Kilogram)?.value;
    }
    let dt = duration / steps;
    let mut increments = Vec::new();
    for _ in 0..steps as u32 {
        control.checkpoint()?;
        let (flow, aero) = rate(model, &atmosphere, altitude, speed, mass)?;
        let midpoint = rate(model, &atmosphere, altitude, speed, mass - flow * dt * 0.5)?.0;
        let midpoint2 = rate(
            model,
            &atmosphere,
            altitude,
            speed,
            mass - midpoint * dt * 0.5,
        )?
        .0;
        let end = rate(model, &atmosphere, altitude, speed, mass - midpoint2 * dt)?.0;
        let burn = dt * (flow + 2.0 * midpoint + 2.0 * midpoint2 + end) / 6.0;
        increments.push(PhaseIncrement {
            duration: Quantity::new(dt, Unit::Second)?,
            distance: Quantity::new(ground_speed * dt, Unit::Meter)?,
            fuel_burn: Quantity::new(burn, Unit::Kilogram)?,
            cost: Quantity::new(burn * model.fuel_price, Unit::Dollar)?,
            conditions: BTreeMap::new(),
            details: serde_json::json!({"start_mass_kg":mass,
                "end_mass_kg":mass-burn,"fuel_flow_kg_s":flow,"aerodynamics":aero}),
        });
        mass -= burn;
    }
    Ok(PhaseOutput {
        increments,
        approximate: true,
        artifacts: Vec::new(),
        details: serde_json::json!({"method":"rk4_level_flight", "force_balance":"lift=weight; thrust=drag",
            "validity":"parabolic or tabulated induced drag; constant flight condition; distance at TAS minus declared headwind; CL and powerplant thrust within limit; tabulated polars follow polar_basis", "powerplant":model.powerplant.basis(), "polar_table":model.aero.polar_table, "polar_basis":model.polar_basis, "maximum_step_s":model.maximum_step}),
    })
}

fn rate(
    model: &FlightModel,
    atmosphere: &AtmosphereState,
    altitude: f64,
    speed: f64,
    mass: f64,
) -> Result<(f64, AerodynamicState), SolverError> {
    if mass <= 0.0 {
        return Err(SolverError::Failed("nonpositive integration mass".into()));
    }
    let aero = aerodynamics::evaluate_configuration(
        &model.aero,
        "clean",
        model.area,
        model.aspect_ratio,
        FlightCondition {
            density_kg_m3: atmosphere.density_kg_m3,
            speed_of_sound_m_s: atmosphere.speed_of_sound_m_s,
            true_airspeed_m_s: speed,
            mass_kg: mass,
        },
    )
    .map_err(external)?;
    let available = model
        .powerplant
        .available_thrust(atmosphere.density_kg_m3, speed);
    if aero.lift_coefficient > model.aero.cl_max
        || aero.drag_n > available
        || aero.model.validity_status != "valid"
    {
        return Err(SolverError::Failed(format!(
            "level-flight envelope exceeded: CL {}, drag {} N, available {available} N, validity {}",
            aero.lift_coefficient, aero.drag_n, aero.model.validity_status
        )));
    }
    let flow = model.powerplant.fuel_flow(
        aero.drag_n,
        &Operating {
            altitude,
            speed,
            mach: speed / atmosphere.speed_of_sound_m_s,
            mode: OperatingMode::Cruise,
        },
    )?;
    Ok((flow, aero))
}
