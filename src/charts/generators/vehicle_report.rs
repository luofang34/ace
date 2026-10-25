use std::collections::BTreeMap;

use ace_engine::{Margin, MarginStatus};

use crate::charts::spec::{Annotation, AxisSpec, ChartSpec, SeriesSpec};

/// Percentage slack of every requirement; unknown and inapplicable margins plot at zero
/// and say so in their label rather than borrowing a value.
pub(crate) fn vehicle_requirement_margins(
    name: &str,
    margins: &BTreeMap<String, Margin>,
) -> ChartSpec {
    let plotted = |margin: &Margin| match margin.status {
        MarginStatus::Unknown | MarginStatus::NotApplicable => 0.0,
        _ => margin.fraction.map_or(0.0, |fraction| fraction * 100.0),
    };
    ChartSpec {
        chart_type: "bar".to_owned(),
        title: format!("{name}: requirement margins"),
        x: AxisSpec {
            label: "Requirement".to_owned(),
            unit: "index".to_owned(),
            values: (0..margins.len()).map(|index| index as f64).collect(),
        },
        y: AxisSpec {
            label: "Percentage margin".to_owned(),
            unit: "%".to_owned(),
            values: Vec::new(),
        },
        surface: None,
        series: vec![SeriesSpec {
            id: "margin".to_owned(),
            label: "Requirement margin".to_owned(),
            unit: "%".to_owned(),
            values: margins.values().map(plotted).collect(),
        }],
        annotations: margins
            .iter()
            .enumerate()
            .map(|(index, (id, margin))| Annotation {
                x: index as f64,
                y: plotted(margin),
                label: match margin.status {
                    MarginStatus::Unknown => format!("{id} (unknown)"),
                    MarginStatus::NotApplicable => format!("{id} (not applicable)"),
                    _ => id.clone(),
                },
            })
            .collect(),
        warnings: Vec::new(),
    }
}

/// Total mass at each normal-path stage boundary of a recorded mission report, with
/// payload releases visible as steps; `None` when the report has no completed stage.
pub(crate) fn vehicle_mission_mass(name: &str, report: &serde_json::Value) -> Option<ChartSpec> {
    let stages = report["stages"].as_array()?;
    let mass = |point: &serde_json::Value| point["mass"]["total_mass_kg"].as_f64();
    let mut masses = vec![mass(&stages.first()?["start"])?];
    let mut labels = vec!["start".to_owned()];
    for stage in stages {
        masses.push(mass(&stage["end"])?);
        labels.push(stage["id"].as_str().unwrap_or("stage").to_owned());
    }
    let boundaries: Vec<f64> = (0..masses.len()).map(|index| index as f64).collect();
    Some(ChartSpec {
        chart_type: "line".to_owned(),
        title: format!("{name}: mission mass"),
        x: AxisSpec {
            label: "Stage boundary".to_owned(),
            unit: "index".to_owned(),
            values: boundaries.clone(),
        },
        y: AxisSpec {
            label: "Mass".to_owned(),
            unit: "kg".to_owned(),
            values: Vec::new(),
        },
        surface: None,
        series: vec![SeriesSpec {
            id: "mass".to_owned(),
            label: "Total mass".to_owned(),
            unit: "kg".to_owned(),
            values: masses.clone(),
        }],
        annotations: boundaries
            .into_iter()
            .zip(masses)
            .zip(labels)
            .map(|((x, y), label)| Annotation { x, y, label })
            .collect(),
        warnings: Vec::new(),
    })
}
