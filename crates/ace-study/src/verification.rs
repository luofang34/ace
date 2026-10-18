use crate::{StudyError, StudyResult, VariantOption};
use ace_engine::{PluginSpec, Registry, Session, Snapshot};
use ace_model::{Quantity, Tolerance, VehicleModel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Provenance category of an independently supplied numerical reference.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    /// Closed-form or independently hand-calculated result.
    Analytic,
    /// Independent implementation of the declared equations.
    IndependentCode,
    /// Published data with its assumptions and uncertainty stated in the citation.
    PublishedData,
}

/// Scope of a reference comparison; passing never globally qualifies a solver.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationReference {
    /// How the reference quantities were obtained.
    pub kind: ReferenceKind,
    /// Formula, publication or immutable dataset identity.
    pub citation: String,
    /// Conditions under which the comparison is meaningful.
    pub assumptions: Vec<String>,
}

/// Independently supplied expected quantity and numerical error allowance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceValue {
    /// Reference magnitude and units.
    pub value: Quantity,
    /// Absolute plus relative allowance, measured against the reference magnitude.
    pub tolerance: Tolerance,
}

/// Executable numerical verification case applied to an authoritative baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationCase {
    /// Case schema; must be one.
    pub schema_version: u32,
    /// Stable human-readable case identifier.
    pub name: String,
    /// Source and scope of the independent reference.
    pub reference: VerificationReference,
    /// Typed configuration changes.
    pub configuration: VariantOption,
    /// Canonical parameter identifiers and independent expected results.
    pub expected: BTreeMap<String, ReferenceValue>,
}

/// Observed comparison; absent output cannot pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationCheck {
    /// Actual value in the reference unit, if usable.
    pub actual: Option<Quantity>,
    /// Absolute numerical difference from the reference, if usable.
    pub absolute_error: Option<Quantity>,
    /// Whether the usable result satisfies the absolute-plus-relative allowance.
    pub passed: bool,
}

/// Content-addressed case, resolved model, contracts and observed execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRecord {
    /// SHA-256 identity of this record with this field empty.
    pub id: String,
    /// Exact case definition and reference values.
    pub case: VerificationCase,
    /// Frozen resolved inputs; replay never reads an external baseline.
    pub model: VehicleModel,
    /// Exact installed analysis contracts, including nested phase definitions.
    pub contracts: BTreeMap<String, PluginSpec>,
    /// Build identity and immutable calculation evidence.
    pub snapshot: Snapshot,
    /// Per-quantity observations.
    pub checks: BTreeMap<String, VerificationCheck>,
}

impl VerificationCase {
    /// Execute a fresh session and record comparisons without changing the baseline.
    pub fn run(&self, baseline: &Session) -> StudyResult<VerificationRecord> {
        self.validate()?;
        let session = self.configuration.instantiate(baseline)?;
        record(self.clone(), session)
    }

    fn validate(&self) -> StudyResult<()> {
        if self.schema_version != 1
            || self.name.trim().is_empty()
            || self.expected.is_empty()
            || self.reference.citation.trim().is_empty()
            || self.reference.assumptions.is_empty()
            || self
                .reference
                .assumptions
                .iter()
                .any(|text| text.trim().is_empty())
        {
            return Err(StudyError::invalid(
                "verification",
                "requires schema 1, name, reference scope and expected quantities",
            ));
        }
        for (name, expected) in &self.expected {
            let absolute = expected.tolerance.absolute.to(expected.value.unit)?.value;
            expected.value.to(expected.value.unit)?;
            if name.trim().is_empty()
                || absolute <= 0.0
                || !expected.tolerance.relative.is_finite()
                || expected.tolerance.relative < 0.0
            {
                return Err(StudyError::invalid(name, "invalid reference tolerance"));
            }
        }
        Ok(())
    }
}

impl VerificationRecord {
    /// All requested comparisons passed and every declared output/closure completed.
    /// This is numerical case verification, not experimental or certification approval.
    pub fn passed(&self) -> bool {
        self.verify_integrity().is_ok()
            && self.snapshot.complete()
            && !self.checks.is_empty()
            && self.checks.values().all(|check| check.passed)
    }

    /// Detect changed input, reference, contract, output or evidence bytes after serialization.
    pub fn verify_integrity(&self) -> StudyResult<()> {
        let mut copy = self.clone();
        copy.id.clear();
        if crate::record::digest(&copy)? != self.id {
            return Err(StudyError::invalid(
                &self.case.name,
                "verification evidence digest mismatch",
            ));
        }
        Ok(())
    }

    /// Re-execute frozen inputs using identical contracts and build; reject silent substitutions.
    pub fn replay(&self, registry: Registry) -> StudyResult<Self> {
        self.verify_integrity()?;
        self.case.validate()?;
        if registry.contracts(&self.model) != self.contracts
            || ace_engine::implementation_identity() != self.snapshot.build
        {
            return Err(StudyError::invalid(
                &self.case.name,
                "replay requires the recorded build and solver contracts",
            ));
        }
        let replay = record(
            self.case.clone(),
            Session::new(self.model.clone(), registry)?,
        )?;
        if replay.id != self.id {
            return Err(StudyError::invalid(
                &self.case.name,
                "replay produced different calculation evidence",
            ));
        }
        Ok(replay)
    }
}

fn record(case: VerificationCase, mut session: Session) -> StudyResult<VerificationRecord> {
    for name in case.expected.keys() {
        session.model().parameter(name)?;
    }
    let contracts = session.contracts();
    let snapshot = session.run()?;
    let checks = case
        .expected
        .iter()
        .map(|(name, expected)| {
            let actual = snapshot
                .values
                .get(name)
                .filter(|value| value.usable())
                .and_then(|value| value.quantity)
                .map(|q| q.to(expected.value.unit))
                .transpose()?;
            let absolute_error = actual
                .map(|q| Quantity::new((q.value - expected.value.value).abs(), q.unit))
                .transpose()?;
            let tolerance = expected.tolerance.absolute.to(expected.value.unit)?.value
                + expected.tolerance.relative * expected.value.value.abs();
            if !tolerance.is_finite() {
                return Err(StudyError::invalid(name, "reference tolerance overflow"));
            }
            Ok((
                name.clone(),
                VerificationCheck {
                    actual,
                    absolute_error,
                    passed: absolute_error.is_some_and(|error| error.value <= tolerance),
                },
            ))
        })
        .collect::<StudyResult<_>>()?;
    let mut record = VerificationRecord {
        id: String::new(),
        case,
        model: session.model().clone(),
        contracts,
        snapshot,
        checks,
    };
    record.id = crate::record::digest(&record)?;
    Ok(record)
}
