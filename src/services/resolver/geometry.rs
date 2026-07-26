use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::Dimension;
use crate::domain::schema::{
    AircraftGeometry, FuselageGeometry, GeometryValue, GeometryValueProvenance, RawGeometry,
    RawTailGeometry, TailGeometry, Wing,
};
use crate::domain::topology::AircraftTopology;

use super::positive_quantity;

#[cfg(test)]
mod tests;

const CORRELATION_ID: &str = "ace_conventional_geometry";
const CORRELATION_VERSION: u32 = 1;

pub(super) fn resolve_geometry(
    raw: &RawGeometry,
    topology: &AircraftTopology,
    wing: &Wing,
    category: &str,
    propulsion_architecture: &str,
) -> AexResult<AircraftGeometry> {
    reject_unmatched_component(
        topology,
        "fuselage",
        raw.fuselage
            .as_ref()
            .is_some_and(|value| value.length.is_some() || value.diameter.is_some()),
        "aircraft.geometry.fuselage",
    )?;
    reject_unmatched_component(
        topology,
        "horizontal_tail",
        raw.horizontal_tail
            .as_ref()
            .is_some_and(|value| value.area.is_some() || value.arm.is_some()),
        "aircraft.geometry.horizontal_tail",
    )?;
    reject_unmatched_component(
        topology,
        "vertical_tail",
        raw.vertical_tail
            .as_ref()
            .is_some_and(|value| value.area.is_some() || value.arm.is_some()),
        "aircraft.geometry.vertical_tail",
    )?;
    let transport = category.contains("transport") || propulsion_architecture.contains("turbofan");
    let fuselage = topology
        .has_component_kind("fuselage")
        .then(|| resolve_fuselage(raw, wing, transport))
        .transpose()?;
    let horizontal_tail = topology
        .has_component_kind("horizontal_tail")
        .then(|| {
            resolve_tail(
                raw.horizontal_tail.as_ref(),
                wing,
                fuselage.as_ref(),
                transport,
                true,
            )
        })
        .transpose()?;
    let vertical_tail = topology
        .has_component_kind("vertical_tail")
        .then(|| {
            resolve_tail(
                raw.vertical_tail.as_ref(),
                wing,
                fuselage.as_ref(),
                transport,
                false,
            )
        })
        .transpose()?;
    Ok(AircraftGeometry {
        fuselage,
        horizontal_tail,
        vertical_tail,
    })
}

fn reject_unmatched_component(
    topology: &AircraftTopology,
    component: &str,
    has_explicit_block: bool,
    path: &str,
) -> AexResult<()> {
    if has_explicit_block && !topology.has_component_kind(component) {
        return Err(AexError::validation(
            "GEOMETRY_COMPONENT_MISMATCH",
            path,
            format!("geometry is declared without a {component} topology component"),
        ));
    }
    Ok(())
}

fn resolve_fuselage(
    raw: &RawGeometry,
    wing: &Wing,
    transport: bool,
) -> AexResult<FuselageGeometry> {
    let block = raw.fuselage.as_ref();
    let derived_length = wing.span_m * if transport { 1.15 } else { 0.75 };
    let derived_diameter = wing.area_m2.sqrt() / if transport { 5.0 } else { 3.0 };
    Ok(FuselageGeometry {
        length: resolve_value(
            block.and_then(|value| value.length.as_deref()),
            Dimension::Length,
            "aircraft.geometry.fuselage.length",
            derived_length,
            "m",
        )?,
        diameter: resolve_value(
            block.and_then(|value| value.diameter.as_deref()),
            Dimension::Length,
            "aircraft.geometry.fuselage.diameter",
            derived_diameter,
            "m",
        )?,
    })
}

fn resolve_tail(
    raw: Option<&RawTailGeometry>,
    wing: &Wing,
    fuselage: Option<&FuselageGeometry>,
    transport: bool,
    horizontal: bool,
) -> AexResult<TailGeometry> {
    let name = if horizontal {
        "horizontal_tail"
    } else {
        "vertical_tail"
    };
    let area_ratio = match (transport, horizontal) {
        (true, true) => 0.24,
        (true, false) => 0.12,
        (false, true) => 0.20,
        (false, false) => 0.10,
    };
    let fuselage_length = fuselage.map_or_else(
        || wing.span_m * if transport { 1.15 } else { 0.75 },
        |geometry| geometry.length.value,
    );
    let wing_x_ratio = if transport { 0.42 } else { 0.34 };
    let derived_arm = fuselage_length * (0.84 - wing_x_ratio);
    Ok(TailGeometry {
        area: resolve_value(
            raw.and_then(|value| value.area.as_deref()),
            Dimension::Area,
            &format!("aircraft.geometry.{name}.area"),
            wing.area_m2 * area_ratio,
            "m^2",
        )?,
        arm: resolve_value(
            raw.and_then(|value| value.arm.as_deref()),
            Dimension::Length,
            &format!("aircraft.geometry.{name}.arm"),
            derived_arm,
            "m",
        )?,
    })
}

fn resolve_value(
    raw: Option<&str>,
    dimension: Dimension,
    path: &str,
    derived: f64,
    unit: &'static str,
) -> AexResult<GeometryValue> {
    let (value, provenance) = match raw {
        Some(value) => (
            positive_quantity(value, dimension, path)?,
            explicit_provenance(),
        ),
        None => (derived, correlation_provenance()),
    };
    Ok(GeometryValue {
        value,
        unit,
        provenance,
    })
}

fn explicit_provenance() -> GeometryValueProvenance {
    GeometryValueProvenance {
        kind: "user_input",
        source: "aircraft document",
        correlation_id: None,
        correlation_version: None,
        explicitly_provided: true,
    }
}

fn correlation_provenance() -> GeometryValueProvenance {
    GeometryValueProvenance {
        kind: "statistical_correlation",
        source: "ACE conventional geometry correlation",
        correlation_id: Some(CORRELATION_ID),
        correlation_version: Some(CORRELATION_VERSION),
        explicitly_provided: false,
    }
}
