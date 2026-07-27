use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::domain::diagnostic::{AexError, AexResult};
use crate::domain::quantity::QuantityOutput;
use crate::domain::result::{MassPropertiesAnalysis, ResultProvenance};
use crate::domain::schema::{
    ComponentMass, ComponentMassStatement, MassPropertyProvenance, MassPropertyValue,
};

const CLOSURE_TOLERANCE: f64 = 1.0e-6;

mod conversion;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct MassPropertiesEvidence {
    pub(crate) statement: MassStatementEvidence,
    pub(crate) closure_error: QuantityOutput,
    pub(crate) states: Vec<MassStateEvidence>,
    pub(crate) minimum_center_of_gravity: QuantityOutput,
    pub(crate) maximum_center_of_gravity: QuantityOutput,
    pub(crate) neutral_point: Option<QuantityOutput>,
    pub(crate) reference_chord: QuantityOutput,
    pub(crate) minimum_static_margin: Option<f64>,
    pub(crate) maximum_static_margin: Option<f64>,
    pub(crate) stability_supported: bool,
    pub(crate) failed_constraints: Vec<String>,
    pub(crate) provenance: ResultProvenance,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct MassStatementEvidence {
    pub(crate) model_id: String,
    pub(crate) model_version: u32,
    pub(crate) operating_empty_mass: QuantityOutput,
    pub(crate) components: Vec<ComponentMassEvidence>,
    pub(crate) fuel_station: MassPropertyEvidence,
    pub(crate) payload_station: MassPropertyEvidence,
    pub(crate) closure_error_kg: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct ComponentMassEvidence {
    pub(crate) component_id: String,
    pub(crate) component_kind: String,
    pub(crate) count: u32,
    pub(crate) mass: MassPropertyEvidence,
    pub(crate) station: MassPropertyEvidence,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct MassPropertyEvidence {
    pub(crate) value: f64,
    pub(crate) unit: String,
    pub(crate) provenance: MassPropertyProvenanceEvidence,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct MassPropertyProvenanceEvidence {
    pub(crate) kind: String,
    pub(crate) source: String,
    pub(crate) correlation_id: Option<String>,
    pub(crate) correlation_version: Option<u32>,
    pub(crate) explicitly_provided: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct MassStateEvidence {
    pub(crate) id: String,
    pub(crate) total_mass: QuantityOutput,
    pub(crate) fuel_mass: QuantityOutput,
    pub(crate) payload_mass: QuantityOutput,
    pub(crate) center_of_gravity: QuantityOutput,
    pub(crate) static_margin: Option<f64>,
}

impl From<&MassPropertiesAnalysis> for MassPropertiesEvidence {
    fn from(value: &MassPropertiesAnalysis) -> Self {
        Self {
            statement: MassStatementEvidence::from(&value.statement),
            closure_error: value.closure_error.clone(),
            states: value.states.iter().map(MassStateEvidence::from).collect(),
            minimum_center_of_gravity: value.minimum_center_of_gravity.clone(),
            maximum_center_of_gravity: value.maximum_center_of_gravity.clone(),
            neutral_point: value.neutral_point.clone(),
            reference_chord: value.reference_chord.clone(),
            minimum_static_margin: value.minimum_static_margin,
            maximum_static_margin: value.maximum_static_margin,
            stability_supported: value.stability_supported,
            failed_constraints: value.failed_constraints.clone(),
            provenance: value.provenance.clone(),
        }
    }
}

impl From<&ComponentMassStatement> for MassStatementEvidence {
    fn from(value: &ComponentMassStatement) -> Self {
        let component_mass = value
            .components
            .iter()
            .map(|component| component.mass.value)
            .sum::<f64>();
        Self {
            model_id: value.model_id.to_owned(),
            model_version: value.model_version,
            operating_empty_mass: QuantityOutput::si(component_mass - value.closure_error_kg, "kg"),
            components: value
                .components
                .iter()
                .map(ComponentMassEvidence::from)
                .collect(),
            fuel_station: MassPropertyEvidence::from(&value.fuel_station),
            payload_station: MassPropertyEvidence::from(&value.payload_station),
            closure_error_kg: value.closure_error_kg,
        }
    }
}

impl From<&ComponentMass> for ComponentMassEvidence {
    fn from(value: &ComponentMass) -> Self {
        Self {
            component_id: value.component_id.clone(),
            component_kind: value.component_kind.clone(),
            count: value.count,
            mass: MassPropertyEvidence::from(&value.mass),
            station: MassPropertyEvidence::from(&value.station),
        }
    }
}

impl From<&MassPropertyValue> for MassPropertyEvidence {
    fn from(value: &MassPropertyValue) -> Self {
        Self {
            value: value.value,
            unit: value.unit.to_owned(),
            provenance: MassPropertyProvenanceEvidence::from(&value.provenance),
        }
    }
}

impl From<&MassPropertyProvenance> for MassPropertyProvenanceEvidence {
    fn from(value: &MassPropertyProvenance) -> Self {
        Self {
            kind: value.kind.to_owned(),
            source: value.source.clone(),
            correlation_id: value.correlation_id.map(str::to_owned),
            correlation_version: value.correlation_version,
            explicitly_provided: value.explicitly_provided,
        }
    }
}

impl MassPropertiesEvidence {
    pub(crate) fn validate(&self) -> AexResult<()> {
        validate_statement(&self.statement)?;
        validate_states(self)?;
        validate_bounds(self)?;
        validate_stability(self)?;
        validate_failed_constraints(&self.failed_constraints)?;
        validate_result_provenance(&self.provenance)?;
        super::validate_quantity(
            &self.closure_error,
            "evidence.results.mass_properties.closure_error",
        )?;
        if self.closure_error.unit != "kg"
            || !close(self.closure_error.value, self.statement.closure_error_kg)
            || self.statement.closure_error_kg.abs() > CLOSURE_TOLERANCE
        {
            return Err(invalid(
                "closure error must match the mass statement and be near zero",
            ));
        }
        Ok(())
    }
}

fn validate_statement(statement: &MassStatementEvidence) -> AexResult<()> {
    if statement.model_id.trim().is_empty()
        || statement.model_version == 0
        || statement.components.is_empty()
        || !statement.closure_error_kg.is_finite()
    {
        return Err(invalid(
            "mass statement identity and components are required",
        ));
    }
    validate_quantity_unit(&statement.operating_empty_mass, "kg", true)?;
    validate_property(&statement.fuel_station, "m")?;
    validate_property(&statement.payload_station, "m")?;
    let mut ids = BTreeSet::new();
    for component in &statement.components {
        validate_component(component, &mut ids)?;
    }
    let component_mass = statement
        .components
        .iter()
        .map(|component| component.mass.value)
        .sum::<f64>();
    if !close(
        component_mass - statement.operating_empty_mass.value,
        statement.closure_error_kg,
    ) {
        return Err(invalid(
            "component masses do not close to operating empty mass",
        ));
    }
    Ok(())
}

fn validate_component(
    component: &ComponentMassEvidence,
    ids: &mut BTreeSet<String>,
) -> AexResult<()> {
    if component.component_id.trim().is_empty()
        || component.component_kind.trim().is_empty()
        || component.count == 0
        || !ids.insert(component.component_id.clone())
    {
        return Err(invalid(
            "component identities must be unique and counts positive",
        ));
    }
    validate_property(&component.mass, "kg")?;
    validate_property(&component.station, "m")
}

fn validate_property(property: &MassPropertyEvidence, expected_unit: &str) -> AexResult<()> {
    if !property.value.is_finite()
        || property.value < 0.0
        || property.unit != expected_unit
        || property.provenance.kind.trim().is_empty()
        || property.provenance.source.trim().is_empty()
    {
        return Err(invalid(
            "mass-property values, units, and provenance must be valid",
        ));
    }
    let correlation = property.provenance.correlation_id.as_deref();
    let version = property.provenance.correlation_version;
    if (correlation.is_some() != version.is_some())
        || version.is_some_and(|item| item == 0)
        || correlation.is_some_and(str::is_empty)
        || !valid_source_kind(&property.provenance)
    {
        return Err(invalid(
            "mass-property source kind, correlation, and explicitness are inconsistent",
        ));
    }
    Ok(())
}

fn valid_source_kind(provenance: &MassPropertyProvenanceEvidence) -> bool {
    match provenance.kind.as_str() {
        "fixed" => provenance.explicitly_provided && provenance.correlation_id.is_none(),
        "profile" => !provenance.explicitly_provided && provenance.correlation_id.is_none(),
        "statistical" => !provenance.explicitly_provided && provenance.correlation_id.is_some(),
        _ => false,
    }
}

fn validate_states(evidence: &MassPropertiesEvidence) -> AexResult<()> {
    if evidence.states.is_empty() {
        return Err(invalid("at least one mission mass state is required"));
    }
    let empty_mass = evidence.statement.operating_empty_mass.value;
    let empty_moment = evidence
        .statement
        .components
        .iter()
        .map(|component| component.mass.value * component.station.value)
        .sum::<f64>();
    let mut ids = BTreeSet::new();
    for state in &evidence.states {
        validate_state(state, evidence, empty_mass, empty_moment, &mut ids)?;
    }
    Ok(())
}

fn validate_state(
    state: &MassStateEvidence,
    evidence: &MassPropertiesEvidence,
    empty_mass: f64,
    empty_moment: f64,
    ids: &mut BTreeSet<String>,
) -> AexResult<()> {
    if state.id.trim().is_empty() || !ids.insert(state.id.clone()) {
        return Err(invalid(
            "mission mass-state identities must be non-empty and unique",
        ));
    }
    validate_quantity_unit(&state.total_mass, "kg", true)?;
    validate_quantity_unit(&state.fuel_mass, "kg", false)?;
    validate_quantity_unit(&state.payload_mass, "kg", false)?;
    validate_quantity_unit(&state.center_of_gravity, "m", false)?;
    if !close(
        state.total_mass.value - state.fuel_mass.value - state.payload_mass.value,
        empty_mass,
    ) {
        return Err(invalid(
            "mission mass state does not close to operating empty mass",
        ));
    }
    let expected_moment = empty_moment
        + state.fuel_mass.value * evidence.statement.fuel_station.value
        + state.payload_mass.value * evidence.statement.payload_station.value;
    if !close(
        state.center_of_gravity.value,
        expected_moment / state.total_mass.value,
    ) || state.static_margin.is_some_and(|value| !value.is_finite())
    {
        return Err(invalid("mission CG or static margin is inconsistent"));
    }
    Ok(())
}

fn validate_bounds(evidence: &MassPropertiesEvidence) -> AexResult<()> {
    validate_quantity_unit(&evidence.minimum_center_of_gravity, "m", false)?;
    validate_quantity_unit(&evidence.maximum_center_of_gravity, "m", false)?;
    let minimum = evidence
        .states
        .iter()
        .map(|state| state.center_of_gravity.value)
        .reduce(f64::min)
        .ok_or_else(|| invalid("minimum CG requires a mass state"))?;
    let maximum = evidence
        .states
        .iter()
        .map(|state| state.center_of_gravity.value)
        .reduce(f64::max)
        .ok_or_else(|| invalid("maximum CG requires a mass state"))?;
    if !close(evidence.minimum_center_of_gravity.value, minimum)
        || !close(evidence.maximum_center_of_gravity.value, maximum)
    {
        return Err(invalid("reported CG bounds do not match mission states"));
    }
    Ok(())
}

fn validate_stability(evidence: &MassPropertiesEvidence) -> AexResult<()> {
    validate_quantity_unit(&evidence.reference_chord, "m", true)?;
    if let Some(neutral_point) = &evidence.neutral_point {
        validate_quantity_unit(neutral_point, "m", false)?;
    }
    let margins = evidence
        .states
        .iter()
        .filter_map(|state| state.static_margin)
        .collect::<Vec<_>>();
    if evidence.stability_supported {
        validate_supported_stability(evidence, &margins)
    } else if evidence.neutral_point.is_some()
        || evidence.minimum_static_margin.is_some()
        || evidence.maximum_static_margin.is_some()
        || !margins.is_empty()
    {
        Err(invalid(
            "unsupported stability cannot publish numeric values",
        ))
    } else {
        Ok(())
    }
}

fn validate_supported_stability(
    evidence: &MassPropertiesEvidence,
    margins: &[f64],
) -> AexResult<()> {
    let neutral_point = evidence
        .neutral_point
        .as_ref()
        .ok_or_else(|| invalid("supported stability requires a neutral point"))?
        .value;
    if evidence.states.iter().any(|state| {
        !state.static_margin.is_some_and(|margin| {
            close(
                margin,
                (neutral_point - state.center_of_gravity.value) / evidence.reference_chord.value,
            )
        })
    }) {
        return Err(invalid(
            "state static margin does not match neutral point, CG, and reference chord",
        ));
    }
    let Some((minimum, maximum)) = margin_bounds(margins) else {
        return Err(invalid(
            "supported stability requires every mission-state margin",
        ));
    };
    let valid = margins.len() == evidence.states.len()
        && evidence
            .minimum_static_margin
            .is_some_and(|value| close(value, minimum))
        && evidence
            .maximum_static_margin
            .is_some_and(|value| close(value, maximum));
    if !valid {
        return Err(invalid(
            "reported static-margin bounds do not match mission states",
        ));
    }
    validate_stability_failure(evidence, minimum)
}

fn margin_bounds(values: &[f64]) -> Option<(f64, f64)> {
    let mut values = values.iter().copied();
    let first = values.next()?;
    Some(values.fold((first, first), |(minimum, maximum), value| {
        (minimum.min(value), maximum.max(value))
    }))
}

fn validate_failed_constraints(constraints: &[String]) -> AexResult<()> {
    let mut unique = BTreeSet::new();
    if constraints
        .iter()
        .any(|item| item.trim().is_empty() || !unique.insert(item))
    {
        Err(invalid(
            "failed constraints must have unique non-empty identities",
        ))
    } else {
        Ok(())
    }
}

fn validate_stability_failure(
    evidence: &MassPropertiesEvidence,
    minimum_margin: f64,
) -> AexResult<()> {
    let declared = evidence
        .failed_constraints
        .iter()
        .any(|item| item == "stability.static_margin");
    if declared == (minimum_margin <= 0.0) {
        Ok(())
    } else {
        Err(invalid(
            "static-margin failure label does not match the minimum margin",
        ))
    }
}

fn validate_result_provenance(provenance: &ResultProvenance) -> AexResult<()> {
    if provenance.method.trim().is_empty()
        || provenance.backend.trim().is_empty()
        || provenance
            .units
            .iter()
            .any(|(name, unit)| name.trim().is_empty() || unit.trim().is_empty())
        || provenance
            .warnings
            .iter()
            .any(|warning| warning.code.trim().is_empty() || warning.message.trim().is_empty())
    {
        return Err(invalid("mass-properties provenance must be complete"));
    }
    for domain in &provenance.validity_domains {
        domain.validate()?;
    }
    Ok(())
}

fn validate_quantity_unit(
    quantity: &QuantityOutput,
    expected_unit: &str,
    positive: bool,
) -> AexResult<()> {
    super::validate_quantity(quantity, "evidence.results.mass_properties.quantity")?;
    let sign_valid = if positive {
        quantity.value > 0.0
    } else {
        quantity.value >= 0.0
    };
    if quantity.unit == expected_unit && sign_valid {
        Ok(())
    } else {
        Err(invalid(
            "mass-properties quantity has an invalid unit or sign",
        ))
    }
}

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() <= CLOSURE_TOLERANCE * left.abs().max(right.abs()).max(1.0)
}

fn invalid(message: &str) -> AexError {
    AexError::validation(
        "INVALID_EVIDENCE_MASS_PROPERTIES",
        "evidence.results.mass_properties",
        message,
    )
}
