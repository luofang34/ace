use crate::{PhysicsError, PhysicsResult};
use ace_engine::{
    AnalysisContext, AnalysisPlugin, EngineResult, Fidelity, PluginIdentity, PluginOutput,
    PluginSpec, Registry, SolverError,
};
use ace_model::{Quantity, Unit};
use std::collections::BTreeMap;
use std::sync::Arc;

mod evaluation;

#[derive(Clone, Copy)]
enum Kind {
    Geometry,
    Mass,
    Mission,
    Operations,
    Phase,
    Volume,
    WettedArea,
    FuelSizing,
    CgEnvelope,
    ConceptGeometry,
}

struct Native {
    kind: Kind,
    spec: PluginSpec,
}

/// Install deterministic geometry, mass, mission, phase and economics plugins.
pub fn register(registry: &mut Registry) -> EngineResult<()> {
    let revision = source_revision();
    for kind in [
        Kind::Geometry,
        Kind::Mass,
        Kind::Mission,
        Kind::Operations,
        Kind::Phase,
        Kind::Volume,
        Kind::WettedArea,
        Kind::FuelSizing,
        Kind::CgEnvelope,
        Kind::ConceptGeometry,
    ] {
        registry.register(Arc::new(Native {
            kind,
            spec: specification(kind, &revision),
        }))?;
    }
    Ok(())
}

fn specification(kind: Kind, revision: &str) -> PluginSpec {
    use Unit::*;
    let (id, inputs, outputs): (&str, &[_], &[_]) = match kind {
        Kind::ConceptGeometry => (
            "vehicle.concept_geometry",
            &[],
            &[("part_area", SquareMeter)],
        ),
        Kind::CgEnvelope => ("vehicle.mission_cg_envelope", &[], &[("margin", Meter)]),
        Kind::FuelSizing => (
            "vehicle.mission_fuel_requirement",
            &[("minimum_remaining", Kilogram), ("tolerance", Kilogram)],
            &[("required_fuel", Kilogram)],
        ),
        Kind::Volume => ("vehicle.usable_volume", &[], &[("volume", CubicMeter)]),
        Kind::WettedArea => ("vehicle.wetted_area", &[], &[("area", SquareMeter)]),
        Kind::Geometry => (
            "vehicle.geometry_envelopes",
            &[],
            &[("minimum_clearance", Meter), ("minimum_margin", Meter)],
        ),
        Kind::Mass => (
            "vehicle.mass_properties",
            &[],
            &[
                ("mass", Kilogram),
                ("cg_x", Meter),
                ("cg_y", Meter),
                ("cg_z", Meter),
                ("minimum_cg_x", Meter),
                ("maximum_cg_x", Meter),
            ],
        ),
        Kind::Mission => (
            "vehicle.mission_sequence",
            &[],
            &[
                ("duration", Second),
                ("distance", Meter),
                ("block_fuel", Kilogram),
                ("reserve_fuel", Kilogram),
                ("remaining_fuel", Kilogram),
                ("final_mass", Kilogram),
                ("cost", Dollar),
            ],
        ),
        Kind::Operations => (
            "vehicle.operations",
            &[],
            &[
                ("trip_cost", Dollar),
                ("casm", DollarPerNauticalMile),
                ("network_compatible", Ratio),
            ],
        ),
        Kind::Phase => (
            "vehicle.constant_rate_phase",
            &[
                ("duration", Second),
                ("speed", MeterPerSecond),
                ("fuel_flow", KilogramPerSecond),
                ("cost_rate", DollarPerSecond),
            ],
            &[
                ("duration", Second),
                ("distance", Meter),
                ("fuel_burn", Kilogram),
                ("cost", Dollar),
            ],
        ),
    };
    let ports = |items: &[(&str, Unit)]| {
        items
            .iter()
            .map(|(name, unit)| ((*name).into(), *unit))
            .collect()
    };
    PluginSpec {
        identity: PluginIdentity {
            id: id.into(),
            version: "1".into(),
            fidelity: Fidelity::Conceptual,
            code_revision: revision.into(),
        },
        inputs: ports(inputs),
        outputs: ports(outputs),
    }
}

fn source_revision() -> String {
    ace_engine::implementation_identity().digest
}

impl AnalysisPlugin for Native {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }
    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        context: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        let mut output = evaluation::evaluate(self.kind, inputs, context).map_err(|source| {
            SolverError::External {
                solver: self.spec.identity.id.clone(),
                source: Box::new(source),
            }
        })?;
        if matches!(self.kind, Kind::Geometry | Kind::ConceptGeometry) && output.failure.is_none() {
            let text =
                serde_json::to_string(&output.details).map_err(|source| SolverError::External {
                    solver: self.spec.identity.id.clone(),
                    source: Box::new(source),
                })?;
            output.artifacts.push(ace_model::Artifact::from_text(
                text,
                "application/json",
                if matches!(self.kind, Kind::ConceptGeometry) {
                    "ace.geometry.concept/1"
                } else {
                    "ace.geometry.envelopes/1"
                },
                self.spec.identity.generator(),
            ));
        }
        Ok(output)
    }
}

fn result<T: serde::Serialize>(
    details: &T,
    values: &[(&str, f64, Unit)],
    failure: Option<String>,
    approximate: bool,
) -> PhysicsResult<PluginOutput> {
    let values = if failure.is_some() {
        BTreeMap::new()
    } else {
        values
            .iter()
            .map(|(name, value, unit)| Ok(((*name).into(), Quantity::new(*value, *unit)?)))
            .collect::<PhysicsResult<_>>()?
    };
    Ok(PluginOutput {
        values,
        approximate,
        artifacts: Vec::new(),
        details: serde_json::to_value(details)?,
        failure,
    })
}

fn input(values: &BTreeMap<String, Quantity>, port: &str, unit: Unit) -> PhysicsResult<f64> {
    let value = values
        .get(port)
        .ok_or_else(|| PhysicsError::invalid(port, "missing input port"))?;
    crate::bindings::nonnegative(port, value.to(unit)?.value)
}

#[cfg(test)]
mod tests;
