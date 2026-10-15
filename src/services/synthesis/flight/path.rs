use super::{
    external,
    parameters::{FlightModel, positive, read},
};
use crate::{
    domain::{quantity::GRAVITY_M_S2, result::AerodynamicState},
    models::{
        aerodynamics::{self, FlightCondition},
        atmosphere::Isa1976,
        propulsion,
    },
};
use ace_engine::{PhaseIncrement, PhaseOutput, RunControl, SolverError};
use ace_model::{Quantity, Unit, VehicleState};
use std::collections::BTreeMap;

struct Path {
    altitude: f64,
    speed: f64,
    target_altitude: f64,
    target_speed: f64,
    horizontal: f64,
    length: f64,
    sin_gamma: f64,
    cos_gamma: f64,
    idle_thrust: Option<f64>,
}

impl Path {
    fn new(state: &VehicleState) -> Result<Self, SolverError> {
        let values = &state.conditions;
        let altitude = read(values, "altitude", Unit::Meter)?;
        let speed = positive(values, "speed", Unit::MeterPerSecond)?;
        let target_altitude = read(values, "target_altitude", Unit::Meter)?;
        let target_speed = positive(values, "target_speed", Unit::MeterPerSecond)?;
        let horizontal = positive(values, "path_distance", Unit::Meter)?;
        let dh = target_altitude - altitude;
        let length = horizontal.hypot(dh);
        if !(0.0..=20_000.0).contains(&altitude)
            || !(0.0..=20_000.0).contains(&target_altitude)
            || dh.atan2(horizontal).abs() > 10.0_f64.to_radians()
        {
            return Err(SolverError::Failed(
                "energy path requires altitude 0..20000 m and flight-path angle within +/-10 deg"
                    .into(),
            ));
        }
        let idle_thrust = values
            .contains_key("idle_thrust")
            .then(|| read(values, "idle_thrust", Unit::Newton))
            .transpose()?;
        if idle_thrust.is_some_and(|idle| !idle.is_finite() || idle < 0.0) {
            return Err(SolverError::Failed(
                "idle_thrust must be nonnegative".into(),
            ));
        }
        Ok(Self {
            altitude,
            speed,
            target_altitude,
            target_speed,
            horizontal,
            length,
            sin_gamma: dh / length,
            cos_gamma: horizontal / length,
            idle_thrust,
        })
    }

    fn point(&self, fraction: f64) -> (f64, f64) {
        (
            self.altitude + fraction * (self.target_altitude - self.altitude),
            self.speed + fraction * (self.target_speed - self.speed),
        )
    }

    fn duration(&self, lower: f64, upper: f64) -> f64 {
        let start = self.point(lower).1;
        let dv = self.point(upper).1 - start;
        let distance = (upper - lower) * self.length;
        if dv.abs() < 1e-10 * start {
            distance / start
        } else {
            distance * (dv / start).ln_1p() / dv
        }
    }
}

struct Rate {
    burn_per_meter: f64,
    thrust: f64,
    engine_thrust: f64,
    aero: AerodynamicState,
    mass: f64,
    altitude: f64,
    speed: f64,
}

pub(super) fn evaluate(
    model: &FlightModel,
    state: &VehicleState,
    control: &RunControl,
) -> Result<PhaseOutput, SolverError> {
    let path = Path::new(state)?;
    let steps = (path.length / (path.speed.min(path.target_speed) * model.maximum_step))
        .ceil()
        .max(1.0);
    if steps > 100_000.0 {
        return Err(SolverError::Failed(
            "energy path exceeds 100000 integration steps".into(),
        ));
    }
    let mut mass = state.masses.values().try_fold(0.0, |sum, q| {
        Ok::<_, SolverError>(sum + q.to(Unit::Kilogram)?.value)
    })?;
    let mut increments = Vec::new();
    let mut maximum_drag_device = 0.0_f64;
    for step in 0..steps as u32 {
        control.checkpoint()?;
        let lower = f64::from(step) / steps;
        let upper = f64::from(step + 1) / steps;
        let increment = integrate(model, &path, mass, lower, upper)?;
        mass -= increment.fuel_burn.value;
        maximum_drag_device = increment.details["drag_device_n"]
            .as_f64()
            .map_or(maximum_drag_device, |drag| maximum_drag_device.max(drag));
        increments.push(increment);
    }
    Ok(PhaseOutput {
        increments,
        approximate: true,
        artifacts: Vec::new(),
        details: serde_json::json!({"method":"rk4_energy_path", "force_balance":"L=mg*cos(gamma); T=D+mg*sin(gamma)+m*V*dV/ds",
            "validity":"straight prescribed path; TAS linear along path; ISA; clean parabolic or tabulated induced drag; constant installed thrust and TSFC; no wind, turns or engine transients; below a declared idle_thrust the engine burns at idle and the surplus is drag_device_n, an unmodelled drag device; CL limit; tabulated polars follow polar_basis",
            "polar_table":model.aero.polar_table, "polar_basis":model.polar_basis,
            "path_angle_deg":path.sin_gamma.asin().to_degrees(), "idle_thrust_n":path.idle_thrust, "maximum_drag_device_n":maximum_drag_device, "maximum_step_s":model.maximum_step}),
    })
}

fn integrate(
    model: &FlightModel,
    path: &Path,
    mass: f64,
    lower: f64,
    upper: f64,
) -> Result<PhaseIncrement, SolverError> {
    let ds = path.length * (upper - lower);
    let mid = (lower + upper) * 0.5;
    let a = rate(model, path, mass, lower)?;
    let b = rate(model, path, mass - a.burn_per_meter * ds * 0.5, mid)?;
    let c = rate(model, path, mass - b.burn_per_meter * ds * 0.5, mid)?;
    let d = rate(model, path, mass - c.burn_per_meter * ds, upper)?;
    let burn = ds
        * (a.burn_per_meter + 2.0 * b.burn_per_meter + 2.0 * c.burn_per_meter + d.burn_per_meter)
        / 6.0;
    let (altitude, speed) = path.point(upper);
    Ok(PhaseIncrement {
        duration: Quantity::new(path.duration(lower, upper), Unit::Second)?,
        distance: Quantity::new(path.horizontal * (upper - lower), Unit::Meter)?,
        fuel_burn: Quantity::new(burn, Unit::Kilogram)?,
        cost: Quantity::new(burn * model.fuel_price, Unit::Dollar)?,
        conditions: BTreeMap::from([
            ("altitude".into(), Quantity::new(altitude, Unit::Meter)?),
            ("speed".into(), Quantity::new(speed, Unit::MeterPerSecond)?),
        ]),
        details: serde_json::json!({"start_mass_kg":mass,"end_mass_kg":mass-burn,
            "midpoint_mass_kg":c.mass,"midpoint_altitude_m":c.altitude,"midpoint_speed_m_s":c.speed,
            "required_thrust_n":c.thrust,"engine_thrust_n":c.engine_thrust,"drag_device_n":c.engine_thrust-c.thrust,"aerodynamics":c.aero,
            "specific_energy_change_j_kg":GRAVITY_M_S2*(altitude-path.point(lower).0)+0.5*(speed.powi(2)-path.point(lower).1.powi(2)),
            "specific_excess_work_j_kg":ds * ((a.thrust-a.aero.drag_n)/a.mass + 2.0*(b.thrust-b.aero.drag_n)/b.mass + 2.0*(c.thrust-c.aero.drag_n)/c.mass + (d.thrust-d.aero.drag_n)/d.mass)/6.0}),
    })
}

fn rate(model: &FlightModel, path: &Path, mass: f64, fraction: f64) -> Result<Rate, SolverError> {
    if mass <= 0.0 {
        return Err(SolverError::Failed("nonpositive energy-path mass".into()));
    }
    let (altitude, speed) = path.point(fraction);
    let atmosphere = Isa1976::new(0.0).evaluate(altitude).map_err(external)?;
    let aero = aerodynamics::evaluate_configuration(
        &model.aero,
        "clean",
        model.area,
        model.aspect_ratio,
        FlightCondition {
            density_kg_m3: atmosphere.density_kg_m3,
            speed_of_sound_m_s: atmosphere.speed_of_sound_m_s,
            true_airspeed_m_s: speed,
            mass_kg: mass * path.cos_gamma,
        },
    )
    .map_err(external)?;
    let acceleration = speed * (path.target_speed - path.speed) / path.length;
    let thrust = aero.drag_n + mass * (GRAVITY_M_S2 * path.sin_gamma + acceleration);
    let engine_thrust = path.idle_thrust.map_or(thrust, |idle| thrust.max(idle));
    if aero.lift_coefficient > model.aero.cl_max
        || aero.model.validity_status != "valid"
        || !(0.0..=model.available_thrust).contains(&engine_thrust)
    {
        return Err(SolverError::Failed(format!(
            "energy-path envelope exceeded at {altitude} m: CL {}, required thrust {thrust} N, available {} N, validity {}; thrust below zero needs a stage idle_thrust or a shallower path",
            aero.lift_coefficient, model.available_thrust, aero.model.validity_status
        )));
    }
    let flow = propulsion::fuel_flow_for_required_thrust(
        &model.engine,
        altitude,
        speed / atmosphere.speed_of_sound_m_s,
        if path.sin_gamma > 0.0 {
            propulsion::OperatingMode::Climb
        } else {
            propulsion::OperatingMode::Cruise
        },
        engine_thrust,
    )
    .map_err(external)?;
    Ok(Rate {
        burn_per_meter: flow.flow_kg_s / speed,
        thrust,
        engine_thrust,
        aero,
        mass,
        altitude,
        speed,
    })
}

#[cfg(test)]
mod tests;
