use crate::{DrawingInput, DrawingResult, Evidence, ResultStatus, ResultValue};
use ace_model::{Binding, Unit};
use std::collections::BTreeMap;

pub(super) fn evidence(
    result: &mut BTreeMap<String, Evidence>,
    input: &DrawingInput,
    base: &Evidence,
) {
    let Some(data) = &input.data else { return };
    let mut add = |key: String, source: &str, units: Vec<Unit>| {
        if let Some(record) = data.evidence.get(source) {
            result.insert(
                key.clone(),
                Evidence {
                    source: key,
                    imported: Some(record.clone()),
                    units,
                    scenario: record.scenario.clone(),
                    quality: format!("{:?}; {:?}", record.kind, record.maturity),
                    ..base.clone()
                },
            );
        }
    };
    for path in &data.geometry.paths {
        add(
            format!("path:{}", path.id),
            &path.evidence,
            vec![Unit::Meter],
        );
    }
    for cut in &data.geometry.sections {
        add(format!("cut:{}", cut.id), &cut.evidence, vec![Unit::Meter]);
    }
    for curve in &data.results.curves {
        add(
            format!("curve:{}", curve.id),
            &curve.evidence,
            vec![curve.x_axis.unit, curve.y_axis.unit],
        );
    }
    for callout in &data.results.callouts {
        add(
            format!("callout:{}", callout.id),
            &callout.result.evidence,
            vec![callout.result.unit],
        );
    }
    for (i, value) in data.results.statements.iter().enumerate() {
        add(format!("statement:{i}"), &value.evidence, vec![value.unit]);
    }
    for fact in &data.facts {
        add(format!("fact:{}", fact.id), &fact.evidence, vec![]);
    }
    for key in data.evidence.keys() {
        add(format!("source:{key}"), key, vec![]);
    }
}

pub(super) fn coordinate(value: &Binding, input: &DrawingInput) -> DrawingResult<f64> {
    crate::data::resolve(value, Unit::Meter, input)?
        .ok_or_else(|| crate::DrawingError::Invalid("unavailable coordinate".into()))
}

pub(super) fn position(point: &[Binding; 3], input: &DrawingInput) -> DrawingResult<[f64; 3]> {
    Ok([
        coordinate(&point[0], input)?,
        coordinate(&point[1], input)?,
        coordinate(&point[2], input)?,
    ])
}

pub(super) fn unit(unit: Unit) -> String {
    serde_json::to_value(unit)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "?".into())
}

pub(super) fn value(result: &ResultValue, input: &DrawingInput) -> DrawingResult<String> {
    let number = result
        .value
        .as_ref()
        .map(|v| crate::data::resolve(v, result.unit, input))
        .transpose()?
        .flatten();
    let value = number
        .map(|v| format!("{v:.3} {}", unit(result.unit)))
        .unwrap_or_else(|| "N/C".into());
    let check = result
        .check
        .as_ref()
        .map(|check| {
            let status = if number.is_none() {
                "UNKNOWN"
            } else {
                match check.status {
                    ResultStatus::Pass => "PASS",
                    ResultStatus::Warn => "WARN",
                    ResultStatus::Fail => "FAIL",
                    ResultStatus::Unknown => "UNKNOWN",
                    ResultStatus::NotApplicable => "N/A",
                }
            };
            let margin = check
                .margin
                .map(|q| format!("; margin {:+.3} {}", q.value, unit(q.unit)))
                .unwrap_or_default();
            format!(
                " {status}{} / {:?}{margin}",
                if check.provisional { " [PROV]" } else { "" },
                check.priority
            )
        })
        .unwrap_or_default();
    Ok(format!("{value}{check}"))
}

pub(super) fn source(input: &DrawingInput, key: &str) -> String {
    input
        .data
        .as_ref()
        .and_then(|d| d.evidence.get(key))
        .map(|s| {
            format!(
                "{:?} / {:?}; {}{} [{}]",
                s.kind,
                s.maturity,
                s.scenario,
                s.load_case
                    .as_ref()
                    .map(|c| format!(" / {c}"))
                    .unwrap_or_default(),
                s.result_id
            )
        })
        .unwrap_or_else(|| "Unknown source".into())
}
