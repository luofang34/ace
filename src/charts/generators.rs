use crate::charts::spec::{Annotation, AxisSpec, ChartSpec, SeriesSpec};
use crate::domain::diagnostic::{AexError, AexResult, Diagnostic};
use crate::domain::quantity::KNOT_M_S;
use crate::domain::result::{
    ConstraintResult, MissionResult, PayloadRangeResult, RequirementEvaluation, RequirementStatus,
    SweepResult,
};
use crate::domain::schema::{EngineProfile, ResolvedScenario};
use crate::domain::warning::WarningCode;
use crate::models::aerodynamics::coefficient_evaluation_at_mach;
use crate::models::aerodynamics::stall_speed_m_s;
use crate::models::atmosphere::Isa1976;
use crate::models::performance::PointAnalyzer;

pub(crate) fn drag_polar(scenario: &ResolvedScenario) -> AexResult<ChartSpec> {
    let config = &scenario.aircraft.aerodynamics.clean;
    let evaluation = coefficient_evaluation_at_mach(&scenario.aircraft, "clean", 0.0)?;
    let polar = evaluation.coefficients;
    let lift_coefficients: Vec<f64> = (0..80)
        .map(|index| polar.cl_max * f64::from(index) / 79.0)
        .collect();
    let drag_coefficients = lift_coefficients
        .iter()
        .map(|lift| polar.cd0 + config.additional_cd + polar.induced_drag_factor * lift.powi(2))
        .collect();
    let mut warnings = scenario.warnings.clone();
    warnings.extend(evaluation.warnings);
    Ok(ChartSpec {
        chart_type: "line".to_owned(),
        title: format!("{}: drag polar", scenario.name),
        x: AxisSpec {
            label: "Lift coefficient".to_owned(),
            unit: "1".to_owned(),
            values: lift_coefficients,
        },
        y: AxisSpec {
            label: "Drag coefficient".to_owned(),
            unit: "1".to_owned(),
            values: Vec::new(),
        },
        series: vec![SeriesSpec {
            id: "drag_coefficient".to_owned(),
            label: "CD".to_owned(),
            unit: "1".to_owned(),
            values: drag_coefficients,
        }],
        annotations: Vec::new(),
        warnings,
    })
}

pub(crate) fn performance_curves(
    scenario: &ResolvedScenario,
    altitude_m: f64,
) -> AexResult<ChartSpec> {
    let analyzer = PointAnalyzer::new(scenario.clone());
    let atmosphere = Isa1976::new(0.0).evaluate(altitude_m)?;
    let mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;
    let reference = coefficient_evaluation_at_mach(&scenario.aircraft, "clean", 0.0)?;
    let stall = stall_speed_m_s(&scenario.aircraft, "clean", mass, atmosphere.density_kg_m3)?;
    let upper = scenario
        .aircraft
        .limits
        .maximum_operating_speed_m_s
        .unwrap_or_else(|| {
            scenario
                .aircraft
                .limits
                .maximum_operating_mach
                .unwrap_or(0.90)
                * atmosphere.speed_of_sound_m_s
        });
    let speeds: Vec<f64> = (0..80)
        .map(|index| stall * 1.05 + f64::from(index) * (upper - stall * 1.05) / 79.0)
        .collect();
    let curve = curve_values(scenario, &analyzer, altitude_m, mass, &speeds)?;
    let mut warnings = scenario.warnings.clone();
    extend_unique_diagnostics(&mut warnings, reference.warnings);
    extend_unique_diagnostics(&mut warnings, curve.warnings);
    Ok(ChartSpec {
        chart_type: "line".to_owned(),
        title: format!("{}: required and available", scenario.name),
        x: AxisSpec {
            label: "True airspeed".to_owned(),
            unit: "kt".to_owned(),
            values: speeds.iter().map(|speed| speed / KNOT_M_S).collect(),
        },
        y: AxisSpec {
            label: curve.y_label.to_owned(),
            unit: curve.y_unit.to_owned(),
            values: Vec::new(),
        },
        series: vec![
            SeriesSpec {
                id: "required".to_owned(),
                label: format!("{} required", curve.y_label),
                unit: curve.y_unit.to_owned(),
                values: curve.required,
            },
            SeriesSpec {
                id: "available".to_owned(),
                label: format!("{} available", curve.y_label),
                unit: curve.y_unit.to_owned(),
                values: curve.available,
            },
        ],
        annotations: Vec::new(),
        warnings,
    })
}

struct CurveValues {
    required: Vec<f64>,
    available: Vec<f64>,
    y_label: &'static str,
    y_unit: &'static str,
    warnings: Vec<Diagnostic>,
}

fn curve_values(
    scenario: &ResolvedScenario,
    analyzer: &PointAnalyzer,
    altitude_m: f64,
    mass_kg: f64,
    speeds: &[f64],
) -> AexResult<CurveValues> {
    let points = speeds
        .iter()
        .map(|speed| analyzer.point(altitude_m, *speed, mass_kg, "clean"))
        .collect::<AexResult<Vec<_>>>()?;
    let mut warnings = Vec::new();
    for point in &points {
        extend_unique_diagnostics(&mut warnings, point.warnings.clone());
    }
    let (required, available, y_label, y_unit) = match scenario.engine {
        EngineProfile::Piston(_) => (
            points.iter().map(|point| point.power_required_w).collect(),
            points
                .iter()
                .map(|point| point.propulsion.propulsive_power_available_w.unwrap_or(0.0))
                .collect(),
            "Power",
            "W",
        ),
        EngineProfile::Turbofan(_) => (
            points.iter().map(|point| point.thrust_required_n).collect(),
            points
                .iter()
                .map(|point| point.propulsion.thrust_available_n.unwrap_or(0.0))
                .collect(),
            "Thrust",
            "N",
        ),
    };
    Ok(CurveValues {
        required,
        available,
        y_label,
        y_unit,
        warnings,
    })
}

pub(crate) fn climb_envelope(scenario: &ResolvedScenario) -> AexResult<ChartSpec> {
    let analyzer = PointAnalyzer::new(scenario.clone());
    let mass = scenario.aircraft.mass.maximum_takeoff_mass_kg;
    let reference = coefficient_evaluation_at_mach(&scenario.aircraft, "clean", 0.0)?;
    let maximum_altitude = scenario
        .aircraft
        .limits
        .maximum_operating_altitude_m
        .unwrap_or(15_000.0)
        .min(19_900.0);
    let altitudes: Vec<f64> = (0..60)
        .map(|index| f64::from(index) * maximum_altitude / 59.0)
        .collect();
    let evaluations = altitudes
        .iter()
        .map(|altitude| analyzer.maximum_rate_of_climb_with_diagnostics(*altitude, mass))
        .collect::<AexResult<Vec<_>>>()?;
    let rates = evaluations
        .iter()
        .map(|evaluation| evaluation.maximum_rate_m_s.max(0.0))
        .collect();
    let mut warnings = scenario.warnings.clone();
    extend_unique_diagnostics(&mut warnings, reference.warnings);
    for evaluation in evaluations {
        extend_unique_diagnostics(&mut warnings, evaluation.warnings);
    }
    Ok(ChartSpec {
        chart_type: "line".to_owned(),
        title: format!("{}: climb envelope", scenario.name),
        x: AxisSpec {
            label: "Altitude".to_owned(),
            unit: "m".to_owned(),
            values: altitudes,
        },
        y: AxisSpec {
            label: "Maximum rate of climb".to_owned(),
            unit: "m/s".to_owned(),
            values: Vec::new(),
        },
        series: vec![SeriesSpec {
            id: "maximum_rate_of_climb".to_owned(),
            label: "Maximum rate of climb".to_owned(),
            unit: "m/s".to_owned(),
            values: rates,
        }],
        annotations: Vec::new(),
        warnings,
    })
}

pub(crate) fn payload_range(scenario: &ResolvedScenario, result: &PayloadRangeResult) -> ChartSpec {
    let x_values = result
        .points
        .iter()
        .map(|point| point.range.display_value)
        .collect();
    let payloads = result.points.iter().map(|point| point.payload_kg).collect();
    ChartSpec {
        chart_type: "line".to_owned(),
        title: format!("{}: payload-range", scenario.name),
        x: AxisSpec {
            label: "Range".to_owned(),
            unit: "nmi".to_owned(),
            values: x_values,
        },
        y: AxisSpec {
            label: "Payload".to_owned(),
            unit: "kg".to_owned(),
            values: Vec::new(),
        },
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
                x: point.range.display_value,
                y: point.payload_kg,
                label: point.id.clone(),
            })
            .collect(),
        warnings: result.warnings.clone(),
    }
}

pub(crate) fn constraints(scenario: &ResolvedScenario, result: &ConstraintResult) -> ChartSpec {
    ChartSpec {
        chart_type: "line".to_owned(),
        title: format!("{}: constraint diagram", scenario.name),
        x: AxisSpec {
            label: "Wing loading".to_owned(),
            unit: "N/m^2".to_owned(),
            values: result.wing_loading_n_m2.clone(),
        },
        y: AxisSpec {
            label: result.y_axis.clone(),
            unit: result.y_axis.clone(),
            values: Vec::new(),
        },
        series: result
            .constraints
            .iter()
            .map(|(id, values)| SeriesSpec {
                id: id.clone(),
                label: id.replace('_', " "),
                unit: result.y_axis.clone(),
                values: values.clone(),
            })
            .collect(),
        annotations: vec![Annotation {
            x: result.selected_wing_loading_n_m2,
            y: result.selected_loading,
            label: "selected design".to_owned(),
        }],
        warnings: result.warnings.clone(),
    }
}

pub(crate) fn mission_mass(scenario: &ResolvedScenario, result: &MissionResult) -> ChartSpec {
    let x_values: Vec<f64> = (0..=result.segments.len())
        .map(|index| index as f64)
        .collect();
    let mut masses = Vec::with_capacity(result.segments.len().saturating_add(1));
    if let Some(first) = result.segments.first() {
        masses.push(first.start_mass_kg);
    }
    masses.extend(result.segments.iter().map(|segment| segment.end_mass_kg));
    ChartSpec {
        chart_type: "line".to_owned(),
        title: format!("{}: mission mass", scenario.name),
        x: AxisSpec {
            label: "Segment boundary".to_owned(),
            unit: "index".to_owned(),
            values: x_values,
        },
        y: AxisSpec {
            label: "Aircraft mass".to_owned(),
            unit: "kg".to_owned(),
            values: Vec::new(),
        },
        series: vec![SeriesSpec {
            id: "mass".to_owned(),
            label: "Aircraft mass".to_owned(),
            unit: "kg".to_owned(),
            values: masses,
        }],
        annotations: Vec::new(),
        warnings: result.warnings.clone(),
    }
}

pub(crate) fn sweep(result: &SweepResult, metric: &str) -> AexResult<ChartSpec> {
    let first_path = result
        .rows
        .first()
        .and_then(|row| row.variables.keys().next())
        .ok_or_else(|| AexError::analysis("EMPTY_SWEEP", "sweep has no rows"))?
        .clone();
    let x_values = result
        .rows
        .iter()
        .map(|row| {
            row.variables
                .get(&first_path)
                .and_then(serde_json::Value::as_str)
                .and_then(|value| value.split_whitespace().next())
                .and_then(|number| number.parse::<f64>().ok())
                .ok_or_else(|| AexError::analysis("INVALID_SWEEP_ROW", "variable is not numeric"))
        })
        .collect::<AexResult<Vec<_>>>()?;
    let y_values = result
        .rows
        .iter()
        .map(|row| {
            row.metrics.get(metric).copied().ok_or_else(|| {
                AexError::validation("MISSING_SWEEP_METRIC", metric, "metric not present")
            })
        })
        .collect::<AexResult<Vec<_>>>()?;
    Ok(ChartSpec {
        chart_type: if result
            .rows
            .first()
            .is_some_and(|row| row.variables.len() == 2)
        {
            "heatmap"
        } else {
            "line"
        }
        .to_owned(),
        title: format!("Parameter sweep: {metric}"),
        x: AxisSpec {
            label: first_path,
            unit: "input".to_owned(),
            values: x_values,
        },
        y: AxisSpec {
            label: metric.to_owned(),
            unit: "SI".to_owned(),
            values: Vec::new(),
        },
        series: vec![SeriesSpec {
            id: metric.to_owned(),
            label: metric.to_owned(),
            unit: "SI".to_owned(),
            values: y_values,
        }],
        annotations: Vec::new(),
        warnings: result.warnings.clone(),
    })
}

pub(crate) fn requirement_margins(
    scenario: &ResolvedScenario,
    evaluations: &[RequirementEvaluation],
) -> ChartSpec {
    let mut warnings = scenario.warnings.clone();
    if evaluations
        .iter()
        .any(|item| item.resolved_status() == RequirementStatus::Indeterminate)
    {
        warnings.push(Diagnostic::warning(
            WarningCode::IndeterminateRequirement,
            "Boundary-limited requirement margins are omitted from the chart.",
            "requirements",
        ));
    }
    ChartSpec {
        chart_type: "bar".to_owned(),
        title: format!("{}: requirement margins", scenario.name),
        x: AxisSpec {
            label: "Requirement".to_owned(),
            unit: "index".to_owned(),
            values: (0..evaluations.len()).map(|index| index as f64).collect(),
        },
        y: AxisSpec {
            label: "Percentage margin".to_owned(),
            unit: "%".to_owned(),
            values: Vec::new(),
        },
        series: vec![SeriesSpec {
            id: "margin".to_owned(),
            label: "Requirement margin".to_owned(),
            unit: "%".to_owned(),
            values: evaluations.iter().map(chart_margin).collect(),
        }],
        annotations: evaluations
            .iter()
            .enumerate()
            .map(|(index, item)| Annotation {
                x: index as f64,
                y: chart_margin(item),
                label: if item.resolved_status() == RequirementStatus::Indeterminate {
                    format!("{} (indeterminate)", item.id)
                } else {
                    item.id.clone()
                },
            })
            .collect(),
        warnings,
    }
}

fn chart_margin(item: &RequirementEvaluation) -> f64 {
    if item.resolved_status() == RequirementStatus::Indeterminate {
        0.0
    } else {
        item.percentage_margin.unwrap_or(0.0)
    }
}

fn extend_unique_diagnostics(target: &mut Vec<Diagnostic>, diagnostics: Vec<Diagnostic>) {
    for diagnostic in diagnostics {
        if !target
            .iter()
            .any(|existing| existing.code == diagnostic.code && existing.path == diagnostic.path)
        {
            target.push(diagnostic);
        }
    }
}

#[cfg(test)]
mod tests;
