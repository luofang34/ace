use std::collections::{BTreeMap, BTreeSet};

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::{Dimension, parse_quantity};
use crate::domain::schema::{
    AircraftGeometry, ComponentMass, ComponentMassStatement, EngineProfile, MassProperties,
    MassPropertyProvenance, MassPropertyValue, RawComponentMass, RawMass, Wing,
};
use crate::domain::topology::{AircraftComponent, AircraftTopology};

const MODEL_ID: &str = "mass.component_buildup";
const MODEL_VERSION: u32 = 1;
const MASS_CORRELATION: &str = "mass.component_fraction";
const STATION_CORRELATION: &str = "mass.component_station";

pub(super) fn resolve(
    raw: &RawMass,
    topology: &AircraftTopology,
    geometry: &AircraftGeometry,
    wing: &Wing,
    category: &str,
) -> AexResult<MassProperties> {
    let operating_empty_mass_kg = positive_mass(
        &raw.operating_empty_mass,
        "aircraft.mass.operating_empty_mass",
    )?;
    let mut result = MassProperties {
        maximum_takeoff_mass_kg: positive_mass(
            &raw.maximum_takeoff_mass,
            "aircraft.mass.maximum_takeoff_mass",
        )?,
        operating_empty_mass_kg,
        maximum_payload_mass_kg: positive_mass(
            &raw.maximum_payload_mass,
            "aircraft.mass.maximum_payload_mass",
        )?,
        maximum_fuel_mass_kg: positive_mass(
            &raw.maximum_fuel_mass,
            "aircraft.mass.maximum_fuel_mass",
        )?,
        statement: resolve_statement(raw, topology, geometry, wing, category)?,
    };
    if result.operating_empty_mass_kg >= result.maximum_takeoff_mass_kg {
        return Err(AexError::validation(
            "INVALID_MASS_LIMIT",
            "aircraft.mass.operating_empty_mass",
            "operating empty mass must be below maximum takeoff mass",
        ));
    }
    normalize_statistical_residual(&mut result)?;
    Ok(result)
}

pub(super) fn finalize(
    mass: &mut MassProperties,
    engine: &EngineProfile,
    sizing_factor: f64,
) -> AexResult<()> {
    for component in &mut mass.statement.components {
        if component.component_kind == "engine" && component.mass.provenance.kind == "statistical" {
            component.mass = MassPropertyValue {
                value: engine.dry_mass_kg() * f64::from(component.count) * sizing_factor,
                unit: "kg",
                provenance: MassPropertyProvenance {
                    kind: "profile",
                    source: format!("engine profile {}", engine.profile_id()),
                    correlation_id: None,
                    correlation_version: None,
                    explicitly_provided: false,
                },
            };
        }
    }
    normalize_statistical_residual(mass)
}

fn resolve_statement(
    raw: &RawMass,
    topology: &AircraftTopology,
    geometry: &AircraftGeometry,
    wing: &Wing,
    category: &str,
) -> AexResult<ComponentMassStatement> {
    let declared = declared_components(&raw.components, topology)?;
    let components = topology
        .components
        .iter()
        .map(|component| {
            component_mass(
                component,
                declared.get(&component.id).copied(),
                geometry,
                wing,
                category,
            )
        })
        .collect::<AexResult<Vec<_>>>()?;
    Ok(ComponentMassStatement {
        model_id: MODEL_ID,
        model_version: MODEL_VERSION,
        components,
        fuel_station: station_value(
            raw.fuel_station.as_deref(),
            fuel_station_m(geometry, wing, category),
            "aircraft.mass.fuel_station",
            "fuel centroid correlation",
        )?,
        payload_station: station_value(
            raw.payload_station.as_deref(),
            payload_station_m(geometry, wing),
            "aircraft.mass.payload_station",
            "payload centroid correlation",
        )?,
        closure_error_kg: 0.0,
    })
}

fn declared_components<'a>(
    raw: &'a [RawComponentMass],
    topology: &AircraftTopology,
) -> AexResult<BTreeMap<String, &'a RawComponentMass>> {
    let valid = topology
        .components
        .iter()
        .map(|component| component.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut result = BTreeMap::new();
    for item in raw {
        if !valid.contains(item.id.as_str()) {
            return Err(AexError::validation(
                "UNKNOWN_MASS_COMPONENT",
                "aircraft.mass.components",
                format!(
                    "mass statement references unknown topology component {}",
                    item.id
                ),
            ));
        }
        if result.insert(item.id.clone(), item).is_some() {
            return Err(AexError::validation(
                "DUPLICATE_MASS_COMPONENT",
                "aircraft.mass.components",
                format!("component {} appears more than once", item.id),
            ));
        }
    }
    Ok(result)
}

fn component_mass(
    component: &AircraftComponent,
    declared: Option<&RawComponentMass>,
    geometry: &AircraftGeometry,
    wing: &Wing,
    category: &str,
) -> AexResult<ComponentMass> {
    let path = format!("aircraft.mass.components.{}", component.id);
    let mass = match declared.and_then(|item| item.mass.as_deref()) {
        Some(value) => fixed_value(value, Dimension::Mass, "kg", &format!("{path}.mass"))?,
        None => statistical_mass(component, category),
    };
    let station = station_value(
        declared.and_then(|item| item.station.as_deref()),
        component_station_m(component, geometry, wing, category),
        &format!("{path}.station"),
        &format!("{} centroid correlation", component.kind),
    )?;
    Ok(ComponentMass {
        component_id: component.id.clone(),
        component_kind: component.kind.clone(),
        count: component.count,
        mass,
        station,
    })
}

fn statistical_mass(component: &AircraftComponent, category: &str) -> MassPropertyValue {
    MassPropertyValue {
        value: component_weight(&component.kind, category) * f64::from(component.count),
        unit: "kg",
        provenance: MassPropertyProvenance {
            kind: "statistical",
            source: "normalized conceptual component fraction".to_owned(),
            correlation_id: Some(MASS_CORRELATION),
            correlation_version: Some(MODEL_VERSION),
            explicitly_provided: false,
        },
    }
}

fn normalize_statistical_residual(mass: &mut MassProperties) -> AexResult<()> {
    let fixed_sum = mass
        .statement
        .components
        .iter()
        .filter(|item| item.mass.provenance.kind != "statistical")
        .map(|item| item.mass.value)
        .sum::<f64>();
    let residual = mass.operating_empty_mass_kg - fixed_sum;
    if residual < -1.0e-9 {
        return Err(AexError::validation(
            "FIXED_MASS_EXCEEDS_OEW",
            "aircraft.mass.components",
            format!(
                "fixed and profile component mass {fixed_sum:.3} kg exceeds operating empty mass {:.3} kg",
                mass.operating_empty_mass_kg
            ),
        ));
    }
    let statistical_weight = mass
        .statement
        .components
        .iter()
        .filter(|item| item.mass.provenance.kind == "statistical")
        .map(|item| item.mass.value)
        .sum::<f64>();
    if statistical_weight <= 0.0 && residual > 1.0e-6 {
        return Err(AexError::validation(
            "MASS_STATEMENT_CANNOT_CLOSE",
            "aircraft.mass.components",
            format!("no statistical component can absorb the {residual:.3} kg OEW residual"),
        ));
    }
    for component in &mut mass.statement.components {
        if component.mass.provenance.kind == "statistical" {
            component.mass.value = residual * component.mass.value / statistical_weight;
        }
    }
    let resolved = mass
        .statement
        .components
        .iter()
        .map(|item| item.mass.value)
        .sum::<f64>();
    mass.statement.closure_error_kg = resolved - mass.operating_empty_mass_kg;
    Ok(())
}

fn station_value(
    raw: Option<&str>,
    correlated: f64,
    path: &str,
    source: &str,
) -> AexResult<MassPropertyValue> {
    match raw {
        Some(value) => fixed_value(value, Dimension::Length, "m", path),
        None => Ok(MassPropertyValue {
            value: correlated,
            unit: "m",
            provenance: MassPropertyProvenance {
                kind: "statistical",
                source: source.to_owned(),
                correlation_id: Some(STATION_CORRELATION),
                correlation_version: Some(MODEL_VERSION),
                explicitly_provided: false,
            },
        }),
    }
}

fn fixed_value(
    raw: &str,
    dimension: Dimension,
    unit: &'static str,
    path: &str,
) -> AexResult<MassPropertyValue> {
    let value = parse_quantity(raw, dimension)?;
    if !value.is_finite() || value < 0.0 {
        return Err(AexError::validation(
            "INVALID_MASS_PROPERTY",
            path,
            "mass properties must be finite and non-negative",
        ));
    }
    Ok(MassPropertyValue {
        value,
        unit,
        provenance: MassPropertyProvenance {
            kind: "fixed",
            source: "aircraft document".to_owned(),
            correlation_id: None,
            correlation_version: None,
            explicitly_provided: true,
        },
    })
}

fn positive_mass(raw: &str, path: &str) -> AexResult<f64> {
    let value = parse_quantity(raw, Dimension::Mass)?;
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(AexError::validation(
            "INVALID_QUANTITY",
            path,
            "value must be finite and positive",
        ))
    }
}

fn component_station_m(
    component: &AircraftComponent,
    geometry: &AircraftGeometry,
    wing: &Wing,
    category: &str,
) -> f64 {
    let length = reference_length_m(geometry, wing);
    let chord = wing.area_m2 / wing.span_m;
    let wing_le = wing_leading_edge_m(length, category);
    match component.kind.as_str() {
        "fuselage" | "lifting_body" => 0.45 * length,
        "wing" | "fuel_system" => wing_le + 0.40 * chord,
        "horizontal_tail" => geometry
            .horizontal_tail
            .as_ref()
            .map_or(0.82 * length, |tail| wing_le + tail.arm.value),
        "vertical_tail" => geometry
            .vertical_tail
            .as_ref()
            .map_or(0.80 * length, |tail| wing_le + tail.arm.value),
        "engine" if component.count == 1 => 0.10 * length,
        "engine" => wing_le + 0.20 * chord,
        "propeller" => 0.02 * length,
        "canard" => 0.15 * length,
        "boom" => 0.55 * length,
        "payload" => 0.45 * length,
        _ => 0.45 * length,
    }
}

fn fuel_station_m(geometry: &AircraftGeometry, wing: &Wing, category: &str) -> f64 {
    wing_leading_edge_m(reference_length_m(geometry, wing), category)
        + 0.40 * wing.area_m2 / wing.span_m
}

fn payload_station_m(geometry: &AircraftGeometry, wing: &Wing) -> f64 {
    0.45 * reference_length_m(geometry, wing)
}

fn reference_length_m(geometry: &AircraftGeometry, wing: &Wing) -> f64 {
    geometry
        .fuselage
        .as_ref()
        .map_or(2.5 * wing.area_m2 / wing.span_m, |fuselage| {
            fuselage.length.value
        })
}

fn wing_leading_edge_m(length: f64, category: &str) -> f64 {
    if category.contains("transport") {
        0.42 * length
    } else {
        0.34 * length
    }
}

fn component_weight(kind: &str, category: &str) -> f64 {
    let transport = category.contains("transport");
    match (kind, transport) {
        ("fuselage", true) => 0.30,
        ("wing", true) => 0.28,
        ("horizontal_tail", true) => 0.04,
        ("vertical_tail", true) => 0.03,
        ("fuel_system", true) => 0.10,
        ("payload", true) => 0.03,
        ("fuselage", false) => 0.35,
        ("wing", false) => 0.30,
        ("horizontal_tail", false) => 0.05,
        ("vertical_tail", false) => 0.04,
        ("canard", _) => 0.04,
        ("lifting_body", _) => 0.65,
        ("boom", _) => 0.10,
        ("engine", _) => 0.20,
        ("propeller", _) => 0.04,
        ("fuel_system", _) => 0.08,
        ("payload", _) => 0.02,
        _ => 0.01,
    }
}

#[cfg(test)]
mod tests;
