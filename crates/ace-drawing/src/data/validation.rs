use super::*;
use crate::{DrawingError, DrawingInput, DrawingResult};
use std::collections::BTreeSet;

pub(crate) fn resolve(
    binding: &Binding,
    unit: Unit,
    input: &DrawingInput,
) -> DrawingResult<Option<f64>> {
    let quantity = match binding {
        Binding::Constant(value) => Some(*value),
        Binding::Parameter(id) => {
            let parameter = input
                .model
                .parameters
                .get(id)
                .ok_or_else(|| invalid(format!("unknown drawing parameter {id}")))?;
            if !parameter.unit.compatible(unit) {
                return Err(invalid(format!(
                    "incompatible drawing unit for {id}: {:?} versus {unit:?}",
                    parameter.unit
                )));
            }
            input
                .snapshot
                .values
                .get(id)
                .filter(|v| v.usable())
                .and_then(|v| v.quantity)
        }
    };
    Ok(quantity
        .map(|value| value.to(unit))
        .transpose()?
        .map(|q| q.value))
}

pub(crate) fn validate(data: &DrawingData, input: &DrawingInput) -> DrawingResult<()> {
    text(&data.document_reference, true)?;
    for source in data.evidence.values() {
        validate_source(source, input)?;
    }
    let mut ids = BTreeSet::new();
    super::figures::validate(data, &mut ids)?;
    validate_geometry(data, input, &mut ids)?;
    validate_results(data, input, &mut ids)
}

fn validate_geometry(
    data: &DrawingData,
    input: &DrawingInput,
    ids: &mut BTreeSet<String>,
) -> DrawingResult<()> {
    for path in &data.geometry.paths {
        identifier(&path.id, ids)?;
        if input.model.geometry.contains_key(&path.id) {
            return Err(invalid("supplied path shadows canonical geometry"));
        }
        text(&path.label, false)?;
        evidence(data, &path.evidence)?;
        if path.points.len() < 2 || path.points.len() > 100_000 {
            return Err(invalid("geometry paths require 2..100000 points"));
        }
        for point in &path.points {
            position(point, input)?;
        }
        if path.role == PathRole::SectionMaterial {
            let first = path
                .points
                .first()
                .ok_or_else(|| invalid("empty material polygon"))?;
            let station = required(&first[0], Unit::Meter, input)?;
            if !path.closed
                || path.points.len() < 3
                || path.points.iter().any(|p| {
                    required(&p[0], Unit::Meter, input).map_or(true, |x| (x - station).abs() > 1e-8)
                })
            {
                return Err(invalid(
                    "section material requires a closed polygon in one x-normal plane",
                ));
            }
        }
    }
    for section in &data.geometry.sections {
        identifier(&section.id, ids)?;
        evidence(data, &section.evidence)?;
        required(&section.station, Unit::Meter, input)?;
        for id in &section.objects {
            if !input.model.geometry.contains_key(id)
                && !data.geometry.paths.iter().any(|p| &p.id == id)
            {
                return Err(invalid(format!("unknown section surface {id}")));
            }
        }
    }
    Ok(())
}

fn validate_results(
    data: &DrawingData,
    input: &DrawingInput,
    ids: &mut BTreeSet<String>,
) -> DrawingResult<()> {
    for curve in &data.results.curves {
        validate_curve(curve, data, input, ids)?;
    }
    for callout in &data.results.callouts {
        identifier(&callout.id, ids)?;
        if !input.model.geometry.contains_key(&callout.target)
            && !data.geometry.paths.iter().any(|p| p.id == callout.target)
        {
            return Err(invalid(format!(
                "callout {} has unknown target {}",
                callout.id, callout.target
            )));
        }
        position(&callout.anchor, input)?;
        validate_value(&callout.result, data, input)?;
    }
    for value in &data.results.statements {
        validate_value(value, data, input)?;
    }
    for id in &data.configuration_parameters {
        if !input.model.parameters.contains_key(id) {
            return Err(invalid(format!("unknown configuration parameter {id}")));
        }
    }
    Ok(())
}

fn validate_source(source: &SourceEvidence, input: &DrawingInput) -> DrawingResult<()> {
    text(&source.reference, false)?;
    text(&source.result_id, false)?;
    text(&source.scenario, false)?;
    for value in [&source.state, &source.load_case].into_iter().flatten() {
        text(value, false)?;
    }
    match &source.scope {
        EvidenceScope::CurrentModel
            if !matches!(
                source.kind,
                EvidenceKind::Definition | EvidenceKind::Unknown
            ) || source.maturity != Maturity::Unknown =>
        {
            return Err(invalid(
                "measured, simulated, target or readiness claims require a frozen configuration binding",
            ));
        }
        EvidenceScope::Frozen {
            model_digest,
            revision,
        } if model_digest != &input.snapshot.model_digest || *revision != input.model.revision => {
            return Err(invalid(format!(
                "stale or mixed-revision imported evidence {}",
                source.result_id
            )));
        }
        _ => (),
    }
    Ok(())
}

fn validate_curve(
    curve: &ResultCurve,
    data: &DrawingData,
    input: &DrawingInput,
    ids: &mut BTreeSet<String>,
) -> DrawingResult<()> {
    identifier(&curve.id, ids)?;
    text(&curve.title, false)?;
    text(&curve.x_axis.label, false)?;
    text(&curve.y_axis.label, false)?;
    evidence(data, &curve.evidence)?;
    if curve.points.len() < 2 || curve.points.len() > 100_000 {
        return Err(invalid("curves require 2..100000 ordered points"));
    }
    let expected = match curve.role {
        CurveRole::SectionArea => Some((Unit::Meter, Unit::SquareMeter)),
        CurveRole::CgTrajectory | CurveRole::CgEnvelope => Some((Unit::Meter, Unit::Kilogram)),
        _ => None,
    };
    if expected
        .is_some_and(|(x, y)| !curve.x_axis.unit.compatible(x) || !curve.y_axis.unit.compatible(y))
    {
        return Err(invalid(format!(
            "curve {} has incompatible role/axes",
            curve.id
        )));
    }
    for point in &curve.points {
        text(&point.state, false)?;
        resolve(&point.x, curve.x_axis.unit, input)?;
        resolve(&point.y, curve.y_axis.unit, input)?;
    }
    Ok(())
}

fn validate_value(
    value: &ResultValue,
    data: &DrawingData,
    input: &DrawingInput,
) -> DrawingResult<()> {
    text(&value.label, false)?;
    evidence(data, &value.evidence)?;
    if let Some(binding) = &value.value {
        resolve(binding, value.unit, input)?;
    }
    if let Some(equation) = &value.equation {
        text(equation, false)?;
    }
    if let Some(check) = &value.check {
        if value.value.is_none()
            && matches!(
                check.status,
                ResultStatus::Pass | ResultStatus::Warn | ResultStatus::Fail
            )
        {
            return Err(invalid(
                "missing supplied value cannot carry a computed pass/warn/fail disposition",
            ));
        }
        if let Some(margin) = check.margin {
            margin.to(margin.unit)?;
        }
    }
    Ok(())
}

fn position(point: &[Binding; 3], input: &DrawingInput) -> DrawingResult<()> {
    for coordinate in point {
        required(coordinate, Unit::Meter, input)?;
    }
    Ok(())
}

fn required(binding: &Binding, unit: Unit, input: &DrawingInput) -> DrawingResult<f64> {
    resolve(binding, unit, input)?.ok_or_else(|| invalid("unavailable geometry/station input"))
}

pub(super) fn evidence(data: &DrawingData, key: &str) -> DrawingResult<()> {
    if !data.evidence.contains_key(key) {
        return Err(invalid(format!("unknown drawing evidence {key}")));
    }
    Ok(())
}

pub(super) fn identifier(id: &str, ids: &mut BTreeSet<String>) -> DrawingResult<()> {
    text(id, false)?;
    if !ids.insert(id.into()) {
        return Err(invalid(format!("duplicate drawing entity {id}")));
    }
    Ok(())
}

pub(super) fn text(value: &str, empty: bool) -> DrawingResult<()> {
    if (!empty && value.is_empty())
        || value.len() > 500
        || !value.bytes().all(|c| (32..=126).contains(&c))
    {
        return Err(invalid(
            "drawing imports require English ASCII single-line text of at most 500 characters",
        ));
    }
    Ok(())
}

pub(super) fn invalid(message: impl Into<String>) -> DrawingError {
    DrawingError::Invalid(message.into())
}
