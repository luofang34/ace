use super::{AreaStation, ClearanceResult, Envelope, SectionResult, SectionSlice};
use crate::bindings::scalar;
use crate::{PhysicsError, PhysicsResult};
use ace_model::{AnalysisResources, Quantity, Unit};
use std::collections::BTreeMap;

pub(super) fn critical(
    resources: &AnalysisResources,
    envelopes: &BTreeMap<String, Envelope>,
    clearances: &BTreeMap<String, ClearanceResult>,
    values: &BTreeMap<String, Quantity>,
) -> PhysicsResult<Vec<SectionResult>> {
    let mut ranked: Vec<_> = clearances.iter().collect();
    ranked.sort_by(|(id_a, a), (id_b, b)| a.margin_m.total_cmp(&b.margin_m).then(id_a.cmp(id_b)));
    let mut results = Vec::new();
    for (id, clearance) in ranked {
        let axis = (0_u8..3)
            .find(|axis| {
                let index = usize::from(*axis);
                clearance.first_point_m[index] == clearance.second_point_m[index]
            })
            .unwrap_or(0);
        let index = usize::from(axis);
        let station = (clearance.first_point_m[index] + clearance.second_point_m[index]) * 0.5;
        results.push(SectionResult {
            id: format!("clearance.{id}"),
            axis,
            station_m: station,
            reason: format!(
                "{}–{} clearance, margin {:.4} m",
                clearance.first, clearance.second, clearance.margin_m
            ),
            clearance: Some(id.clone()),
            slices: slices(
                envelopes,
                axis,
                station,
                &[clearance.first.clone(), clearance.second.clone()],
            )?,
        });
    }
    for (id, section) in &resources.sections {
        let station = scalar(&section.station, values, Unit::Meter)?;
        results.push(SectionResult {
            id: id.clone(),
            axis: section.axis,
            station_m: station,
            reason: section.reason.clone(),
            clearance: None,
            slices: slices(envelopes, section.axis, station, &section.geometry)?,
        });
    }
    results.extend(volume_sections(envelopes)?);
    Ok(results)
}

fn volume_sections(envelopes: &BTreeMap<String, Envelope>) -> PhysicsResult<Vec<SectionResult>> {
    let mut result = Vec::new();
    for (id, envelope) in envelopes {
        if envelope.usable_volume_m3.is_none() {
            continue;
        }
        // The longest box dimension has the smallest transverse envelope area.
        let axis = (0_u8..3)
            .max_by(|a, b| {
                let extent = |axis: u8| {
                    let index = usize::from(axis);
                    envelope.maximum_m[index] - envelope.minimum_m[index]
                };
                extent(*a).total_cmp(&extent(*b))
            })
            .unwrap_or(0);
        let index = usize::from(axis);
        let station = (envelope.minimum_m[index] + envelope.maximum_m[index]) * 0.5;
        result.push(SectionResult {
            id: format!("volume.{id}"),
            axis,
            station_m: station,
            reason: format!("Usable-volume envelope: {id}"),
            clearance: None,
            slices: slices(envelopes, axis, station, std::slice::from_ref(id))?,
        });
    }
    Ok(result)
}

fn slices(
    envelopes: &BTreeMap<String, Envelope>,
    axis: u8,
    station: f64,
    ids: &[String],
) -> PhysicsResult<Vec<SectionSlice>> {
    let (normal, horizontal, vertical) = match axis {
        0 => (0, 1, 2),
        1 => (1, 0, 2),
        2 => (2, 0, 1),
        _ => return Err(PhysicsError::invalid("section", "axis must be 0, 1, or 2")),
    };
    let mut result = Vec::new();
    for id in ids {
        let envelope = envelopes
            .get(id)
            .ok_or_else(|| PhysicsError::invalid(id, "section geometry unavailable"))?;
        if station < envelope.minimum_m[normal] || station > envelope.maximum_m[normal] {
            continue;
        }
        result.push(SectionSlice {
            geometry: id.clone(),
            minimum_m: [envelope.minimum_m[horizontal], envelope.minimum_m[vertical]],
            maximum_m: [envelope.maximum_m[horizontal], envelope.maximum_m[vertical]],
        });
    }
    Ok(result)
}

pub(super) fn area_distribution(envelopes: &BTreeMap<String, Envelope>) -> Vec<AreaStation> {
    let mut stations: Vec<_> = envelopes
        .values()
        .flat_map(|item| {
            [
                item.minimum_m[0],
                (item.minimum_m[0] + item.maximum_m[0]) * 0.5,
                item.maximum_m[0],
            ]
        })
        .collect();
    stations.sort_by(f64::total_cmp);
    stations.dedup();
    stations
        .into_iter()
        .map(|station| {
            let rectangles: Vec<_> = envelopes
                .values()
                .filter(|item| item.minimum_m[0] <= station && item.maximum_m[0] >= station)
                .map(|item| {
                    (
                        [item.minimum_m[1], item.minimum_m[2]],
                        [item.maximum_m[1], item.maximum_m[2]],
                    )
                })
                .collect();
            AreaStation {
                station_m: station,
                envelope_area_m2: union_area(&rectangles),
            }
        })
        .collect()
}

fn union_area(rectangles: &[([f64; 2], [f64; 2])]) -> f64 {
    let mut edges: Vec<_> = rectangles
        .iter()
        .flat_map(|(low, high)| [low[0], high[0]])
        .collect();
    edges.sort_by(f64::total_cmp);
    edges.dedup();
    let mut area = 0.0;
    for pair in edges.windows(2) {
        if let [left, right] = pair {
            let middle = (left + right) * 0.5;
            let mut intervals: Vec<_> = rectangles
                .iter()
                .filter(|(low, high)| low[0] <= middle && high[0] >= middle)
                .map(|(low, high)| (low[1], high[1]))
                .collect();
            intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut covered = 0.0;
            let mut end = f64::NEG_INFINITY;
            for (low, high) in intervals {
                if high > end {
                    covered += high - low.max(end);
                    end = high;
                }
            }
            area += (right - left) * covered;
        }
    }
    area
}
