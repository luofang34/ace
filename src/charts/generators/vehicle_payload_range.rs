use crate::charts::spec::{Annotation, AxisSpec, ChartSpec, SeriesSpec};
use crate::services::vehicle_payload_range::PayloadRange;

/// Payload against solved range at each loading corner, in the distance parameter's unit.
pub(crate) fn vehicle_payload_range(result: &PayloadRange) -> ChartSpec {
    let range_unit = result
        .points
        .first()
        .and_then(|point| serde_json::to_value(point.range.unit).ok())
        .and_then(|unit| unit.as_str().map(str::to_owned))
        .unwrap_or_else(|| "m".to_owned());
    let ranges = result
        .points
        .iter()
        .map(|point| point.range.value)
        .collect();
    let payloads = result
        .points
        .iter()
        .map(|point| point.payload.value)
        .collect();
    ChartSpec {
        chart_type: "line".to_owned(),
        title: format!("{}: payload-range", result.vehicle_id),
        x: AxisSpec {
            label: "Range".to_owned(),
            unit: range_unit,
            values: ranges,
        },
        y: AxisSpec {
            label: "Payload".to_owned(),
            unit: "kg".to_owned(),
            values: Vec::new(),
        },
        surface: None,
        series: vec![SeriesSpec {
            id: "payload".to_owned(),
            label: "Payload".to_owned(),
            unit: "kg".to_owned(),
            values: payloads,
        }],
        annotations: result
            .points
            .iter()
            .map(|point| Annotation {
                x: point.range.value,
                y: point.payload.value,
                label: point.id.to_owned(),
            })
            .collect(),
        warnings: Vec::new(),
    }
}
