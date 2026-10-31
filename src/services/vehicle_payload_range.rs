//! Payload-range corners flown on the vehicle's own mission rather than a representative cruise.

use std::collections::BTreeMap;
use std::path::Path;

use ace_engine::{Session, Snapshot};
use ace_model::{Binding, MassKind, Quantity, Unit, VehicleModel};
use serde::Serialize;

use crate::domain::diagnostic::{AexError, AexResult};

mod solve;

/// Parameters that give the payload-range problem its meaning in one vehicle file.
pub(crate) struct PayloadRangeRoles<'a> {
    /// Route distance the mission flies; the solved variable.
    pub distance: &'a str,
    /// Fuel left on the most demanding normal or reserve path; zero at the range limit.
    pub remaining: &'a str,
    pub maximum_takeoff_mass: &'a str,
    pub maximum_payload: &'a str,
}

/// Range at each loading corner, with the mission evidence at the last feasible distance.
#[derive(Debug, Serialize)]
pub(crate) struct PayloadRange {
    pub vehicle_id: String,
    pub distance_parameter: String,
    pub payload_parameter: String,
    pub fuel_parameter: String,
    /// Operating mass without payload or fuel, from the remaining mass items.
    pub empty_mass: Quantity,
    pub points: Vec<RangePoint>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RangePoint {
    pub id: &'static str,
    pub payload: Quantity,
    pub fuel: Quantity,
    pub takeoff_mass: Quantity,
    /// Longest route distance whose mission completes with nonnegative remaining fuel.
    pub range: Quantity,
    /// Remaining fuel at `range`; approaches zero as the bracket closes.
    pub remaining_fuel: Quantity,
    /// Width of the final feasible/shortfall distance bracket.
    pub bracket_width: Quantity,
    pub missions_flown: u32,
    /// Why the range is zero without flying, if it is.
    pub note: Option<String>,
}

/// Mass parameters and limits shared by every corner.
struct Loading {
    payload: String,
    fuel: String,
    empty_kg: f64,
    capacity_kg: f64,
    maximum_takeoff_kg: f64,
    maximum_payload_kg: f64,
}

/// Fly the mission at the design, maximum-payload, maximum-fuel and ferry loadings.
pub(crate) fn compute_blocking(
    path: &Path,
    roles: &PayloadRangeRoles<'_>,
) -> AexResult<PayloadRange> {
    let mut session = super::synthesis::open_blocking(path, &BTreeMap::new())?;
    let snapshot = session.run()?;
    let loading = loading(session.model(), &snapshot, roles)?;
    let design_payload = kilograms(&snapshot, &loading.payload)?;
    let design_fuel = kilograms(&snapshot, &loading.fuel)?;
    let free = loading.maximum_takeoff_kg - loading.empty_kg;
    let corners = [
        ("design", design_payload, design_fuel),
        (
            "maximum_payload",
            loading.maximum_payload_kg,
            (free - loading.maximum_payload_kg).clamp(0.0, loading.capacity_kg),
        ),
        (
            "maximum_fuel",
            (free - loading.capacity_kg).clamp(0.0, loading.maximum_payload_kg),
            loading.capacity_kg,
        ),
        ("ferry", 0.0, loading.capacity_kg),
    ];
    let distance_unit = session
        .model()
        .parameter(roles.distance)
        .map_err(model_error)?
        .unit;
    let points = corners
        .into_iter()
        .map(|(id, payload, fuel)| {
            corner(
                &session,
                &snapshot,
                &loading,
                roles,
                (id, payload, fuel),
                distance_unit,
            )
        })
        .collect::<AexResult<Vec<_>>>()?;
    Ok(PayloadRange {
        vehicle_id: session.model().id.clone(),
        distance_parameter: roles.distance.to_owned(),
        payload_parameter: loading.payload,
        fuel_parameter: loading.fuel,
        empty_mass: Quantity::new(loading.empty_kg, Unit::Kilogram).map_err(model_error)?,
        points,
    })
}

fn corner(
    session: &Session,
    snapshot: &Snapshot,
    loading: &Loading,
    roles: &PayloadRangeRoles<'_>,
    (id, payload, fuel): (&'static str, f64, f64),
    distance_unit: Unit,
) -> AexResult<RangePoint> {
    let kg = |value: f64| Quantity::new(value, Unit::Kilogram).map_err(model_error);
    let length = |meters: f64| {
        Quantity::new(meters, Unit::Meter)
            .and_then(|value| value.to(distance_unit))
            .map_err(model_error)
    };
    let (range, remaining, width, flown, note) = if fuel <= 0.0 {
        let note = format!("{id} loading leaves no fuel below maximum takeoff mass");
        (0.0, 0.0, 0.0, 0, Some(note))
    } else {
        let mut fork = session.fork();
        fork.edit(
            &BTreeMap::from([
                (loading.payload.clone(), kg(payload)?),
                (loading.fuel.clone(), kg(fuel)?),
            ]),
            solve::source(id),
        )?;
        let start = meters(snapshot, roles.distance)?;
        let solved = solve::range(&mut fork, roles, start)?;
        (
            solved.range_m,
            solved.remaining_kg,
            solved.width_m,
            solved.flown,
            None,
        )
    };
    Ok(RangePoint {
        id,
        payload: kg(payload)?,
        fuel: kg(fuel)?,
        takeoff_mass: kg(loading.empty_kg + payload + fuel)?,
        range: length(range)?,
        remaining_fuel: kg(remaining)?,
        bracket_width: length(width)?,
        missions_flown: flown,
        note,
    })
}

fn loading(
    model: &VehicleModel,
    snapshot: &Snapshot,
    roles: &PayloadRangeRoles<'_>,
) -> AexResult<Loading> {
    let single = |kind: MassKind, name: &str| {
        let mut items = model.mass_items.values().filter(|item| item.kind == kind);
        match (items.next(), items.next()) {
            (Some(item), None) => Ok(item),
            _ => Err(AexError::validation(
                "INVALID_PAYLOAD_RANGE_LOADING",
                "mass_items",
                format!("payload-range needs exactly one {name} mass item"),
            )),
        }
    };
    let payload_item = single(MassKind::Payload, "payload")?;
    let fuel_item = single(MassKind::Fuel, "fuel")?;
    let editable = |binding: &Binding, name: &str| match binding {
        Binding::Parameter(parameter) => Ok(parameter.clone()),
        Binding::Constant(_) => Err(AexError::validation(
            "INVALID_PAYLOAD_RANGE_LOADING",
            "mass_items",
            format!("the {name} mass item must be bound to a parameter"),
        )),
    };
    let capacity = fuel_item.capacity.as_ref().ok_or_else(|| {
        AexError::validation(
            "INVALID_PAYLOAD_RANGE_LOADING",
            "mass_items",
            "the fuel mass item must declare its capacity",
        )
    })?;
    let mut empty_kg = 0.0;
    for item in model.mass_items.values() {
        if !matches!(item.kind, MassKind::Payload | MassKind::Fuel) {
            empty_kg += bound_kilograms(snapshot, &item.mass)?;
        }
    }
    Ok(Loading {
        payload: editable(&payload_item.mass, "payload")?,
        fuel: editable(&fuel_item.mass, "fuel")?,
        empty_kg,
        capacity_kg: bound_kilograms(snapshot, capacity)?,
        maximum_takeoff_kg: kilograms(snapshot, roles.maximum_takeoff_mass)?,
        maximum_payload_kg: kilograms(snapshot, roles.maximum_payload)?,
    })
}

/// Model unit and lookup errors surface through the engine's error type.
fn model_error(source: ace_model::ModelError) -> AexError {
    ace_engine::EngineError::from(source).into()
}

fn bound_kilograms(snapshot: &Snapshot, binding: &Binding) -> AexResult<f64> {
    match binding {
        Binding::Parameter(name) => kilograms(snapshot, name),
        Binding::Constant(quantity) => Ok(quantity.to(Unit::Kilogram).map_err(model_error)?.value),
    }
}

fn kilograms(snapshot: &Snapshot, name: &str) -> AexResult<f64> {
    Ok(current(snapshot, name)?
        .to(Unit::Kilogram)
        .map_err(model_error)?
        .value)
}

fn meters(snapshot: &Snapshot, name: &str) -> AexResult<f64> {
    Ok(current(snapshot, name)?
        .to(Unit::Meter)
        .map_err(model_error)?
        .value)
}

fn current(snapshot: &Snapshot, name: &str) -> AexResult<Quantity> {
    snapshot
        .values
        .get(name)
        .and_then(|record| record.quantity)
        .ok_or_else(|| {
            AexError::validation(
                "UNAVAILABLE_PAYLOAD_RANGE_VALUE",
                name.to_owned(),
                "the parameter has no current value",
            )
        })
}

#[cfg(test)]
mod tests;
