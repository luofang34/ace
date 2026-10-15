use super::{
    arrangement::{Page, heading},
    geometry::{DrawingGeometry, Frame},
    imported, panel,
};
use crate::{CurveRole, DrawingInput, DrawingResult, Scene, Stroke};
use ace_model::Unit;
use ace_sheet::Sheet;
use std::collections::BTreeMap;

pub(super) struct Curve {
    pub id: String,
    pub points: Vec<[f64; 2]>,
    pub evidence: String,
    pub envelope: bool,
}

pub(super) fn draw(
    scene: &mut Scene,
    sheet: &Sheet,
    input: &DrawingInput,
    page: &Page,
) -> DrawingResult<()> {
    let cg = imported_curves(
        input,
        &[CurveRole::CgTrajectory, CurveRole::CgEnvelope],
        [Unit::Meter, Unit::Kilogram],
    )?;
    let cg = if cg.is_empty() {
        native_cg(sheet, input)?
    } else {
        cg
    };
    plot(
        scene,
        &cg,
        [page.left[0], 123., page.left[1], 48.],
        "CG / MASS",
        ["CG x / m", "mass / kg"],
    );
    energy(scene, sheet, input, page)?;
    Ok(())
}

pub(super) fn area(
    scene: &mut Scene,
    input: &DrawingInput,
    geometry: &DrawingGeometry,
    page: &Page,
    frame: &Frame,
) -> DrawingResult<()> {
    let supplied = imported_curves(
        input,
        &[CurveRole::SectionArea],
        [Unit::Meter, Unit::SquareMeter],
    )?;
    let curves = if supplied.is_empty() {
        let body = super::geometry::sections::body_id(input, geometry)
            .and_then(|id| geometry.native.as_ref()?.objects.get(id));
        body.map(|body| {
            vec![Curve {
                id: "body-area".into(),
                envelope: false,
                evidence: format!("geometry:{}", body.id),
                points: (0..=100)
                    .map(|i| {
                        let x = body.minimum_m[0]
                            + (body.maximum_m[0] - body.minimum_m[0]) * f64::from(i) / 100.;
                        [x, ace_physics::concept_section_area(body, x)]
                    })
                    .collect(),
            }]
        })
        .unwrap_or_default()
    } else {
        supplied
    };
    let [y, h] = page.views.area;
    scene.text(
        [page.views.longitudinal[0], y + 3.],
        "CROSS-SECTIONAL AREA / m^2",
        "layout",
    );
    let maximum = curves
        .iter()
        .flat_map(|c| &c.points)
        .map(|p| p[1])
        .fold(0., f64::max);
    if maximum <= 0. {
        scene.text(
            [page.views.longitudinal[0], y + 10.],
            "Area curve N/C",
            "layout",
        );
        return Ok(());
    }
    for (index, curve) in curves.into_iter().enumerate() {
        scene.path(
            &curve.id,
            curve
                .points
                .iter()
                .map(|p| {
                    [
                        frame.x + p[0] * frame.scale,
                        y + h - p[1] / maximum * (h - 8.),
                    ]
                })
                .collect(),
            false,
            Stroke::Data,
            &curve.evidence,
        );
        if index == 0 {
            scene.text(
                [page.views.longitudinal[0] + 150., y + 3.],
                format!("max {maximum:.2} m^2"),
                &curve.evidence,
            );
        }
    }
    Ok(())
}

fn imported_curves(
    input: &DrawingInput,
    roles: &[CurveRole],
    units: [Unit; 2],
) -> DrawingResult<Vec<Curve>> {
    let mut result = Vec::new();
    for curve in input
        .data
        .iter()
        .flat_map(|d| &d.results.curves)
        .filter(|c| roles.contains(&c.role))
    {
        let mut points = Vec::new();
        for point in &curve.points {
            let x = crate::data::resolve(&point.x, units[0], input)?;
            let y = crate::data::resolve(&point.y, units[1], input)?;
            if let Some((x, y)) = x.zip(y) {
                points.push([x, y]);
            } else {
                points.clear();
                break;
            }
        }
        result.push(Curve {
            id: curve.id.clone(),
            points,
            evidence: format!("curve:{}", curve.id),
            envelope: curve.role == CurveRole::CgEnvelope,
        });
    }
    Ok(result)
}

fn native_cg(sheet: &Sheet, input: &DrawingInput) -> DrawingResult<Vec<Curve>> {
    let Some(panel) = panel(sheet, "vehicle.mission_cg_envelope", Some(&input.scenario)) else {
        return Ok(Vec::new());
    };
    let report: ace_physics::CgReport = serde_json::from_value(panel.details.clone())?;
    let key = format!("analysis:{}", panel.analysis);
    let mut paths: BTreeMap<String, Vec<[f64; 2]>> = BTreeMap::new();
    for p in &report.points {
        paths
            .entry(p.scenario.clone())
            .or_default()
            .push([p.cg_x_m, p.mass_kg]);
    }
    let mut result: Vec<_> = paths
        .into_iter()
        .map(|(id, points)| Curve {
            id,
            points,
            evidence: key.clone(),
            envelope: false,
        })
        .collect();
    let mut points: Vec<_> = report
        .envelope
        .iter()
        .map(|p| [p.minimum_m, p.mass_kg])
        .collect();
    points.extend(
        report
            .envelope
            .iter()
            .rev()
            .map(|p| [p.maximum_m, p.mass_kg]),
    );
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    result.push(Curve {
        id: "cg-envelope".into(),
        points,
        evidence: key,
        envelope: true,
    });
    Ok(result)
}

fn energy(
    scene: &mut Scene,
    sheet: &Sheet,
    input: &DrawingInput,
    page: &Page,
) -> DrawingResult<()> {
    let at = [page.left[0], 184., page.left[1], 48.];
    let supplied: Vec<_> = input
        .data
        .iter()
        .flat_map(|d| &d.results.curves)
        .filter(|c| matches!(c.role, CurveRole::Energy | CurveRole::General))
        .collect();
    if let Some(first) = supplied.first() {
        let units = [time_axis(first.role, first.x_axis.unit), first.y_axis.unit];
        let curves = imported_curves(input, &[first.role], units)?;
        display_units(scene, &curves, units);
        plot(
            scene,
            &curves,
            at,
            &first.title,
            [
                &format!("{} / {}", first.x_axis.label, imported::unit(units[0])),
                &format!("{} / {}", first.y_axis.label, imported::unit(units[1])),
            ],
        );
        let omitted = supplied.iter().filter(|c| c.role != first.role).count();
        let samples: usize = curves.iter().map(|c| c.points.len()).sum();
        scene.text(
            [at[0], at[1] + at[3] + 5.],
            format!("{samples} samples; {omitted} other plots in register"),
            "layout",
        );
        scene.text(
            [at[0], at[1] + at[3] + 10.],
            super::short(&imported::source(input, &first.evidence), 44),
            &format!("curve:{}", first.id),
        );
        return Ok(());
    }
    let Some(panel) = panel(sheet, "vehicle.mission_sequence", Some(&input.scenario)) else {
        plot(scene, &[], at, "ENERGY / FUEL", ["state", "quantity"]);
        return Ok(());
    };
    let report: ace_physics::MissionReport = serde_json::from_value(panel.details.clone())?;
    let key = format!("analysis:{}", panel.analysis);
    if let Some(evidence) = scene.evidence.get_mut(&key) {
        evidence.units = vec![Unit::Hour, Unit::Kilogram];
    }
    let mut curves = Vec::new();
    let mut count = 0;
    fuel_paths(&report, &key, &mut curves, &mut count)?;
    plot(
        scene,
        &curves,
        at,
        "FUEL / MISSION STATE",
        ["elapsed / h", "fuel / kg"],
    );
    scene.text(
        [at[0], at[1] + at[3] + 5.],
        format!("{count} stages; all samples; reserve separate"),
        &key,
    );
    Ok(())
}

fn fuel_paths(
    report: &ace_physics::MissionReport,
    key: &str,
    curves: &mut Vec<Curve>,
    count: &mut usize,
) -> DrawingResult<()> {
    let mut points = Vec::new();
    for stage in &report.stages {
        *count = count.wrapping_add(1);
        for sample in std::iter::once(&stage.start)
            .chain(stage.trajectory.iter())
            .chain(std::iter::once(&stage.end))
        {
            let fuel = sample
                .mass
                .items
                .values()
                .filter(|i| i.kind == ace_model::MassKind::Fuel)
                .map(|i| i.mass_kg)
                .sum();
            points.push([sample.state.time.to(Unit::Hour)?.value, fuel]);
        }
    }
    curves.push(Curve {
        id: format!("fuel:{}", report.mission),
        points,
        evidence: key.into(),
        envelope: false,
    });
    for branch in report.branches.values() {
        fuel_paths(branch, key, curves, count)?;
    }
    Ok(())
}

pub(super) fn plot(
    scene: &mut Scene,
    curves: &[Curve],
    bounds: [f64; 4],
    title: &str,
    labels: [&str; 2],
) {
    let [x, y, w, h] = bounds;
    heading(scene, [x, y], w, &super::short(title, 38), "layout");
    let mut range = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in curves.iter().flat_map(|c| &c.points) {
        range[0] = range[0].min(p[0]);
        range[1] = range[1].max(p[0]);
        range[2] = range[2].min(p[1]);
        range[3] = range[3].max(p[1]);
    }
    if !range.iter().all(|v| v.is_finite()) {
        scene.text([x, y + 12.], "N/C - no usable result curve", "layout");
        return;
    }
    for i in [0, 2] {
        let pad = ((range[i + 1] - range[i]) * 0.05).max(0.001);
        range[i] = if range[i] >= 0. {
            (range[i] - pad).max(0.)
        } else {
            range[i] - pad
        };
        range[i + 1] += pad;
    }
    let point = |p: [f64; 2]| {
        [
            x + 18. + (p[0] - range[0]) / (range[1] - range[0]) * (w - 21.),
            y + h - 9. - (p[1] - range[2]) / (range[3] - range[2]) * (h - 19.),
        ]
    };
    scene.path(
        "plot-axes",
        vec![
            [x + 18., y + 10.],
            [x + 18., y + h - 9.],
            [x + w - 3., y + h - 9.],
        ],
        false,
        Stroke::Datum,
        "layout",
    );
    for curve in curves {
        if curve.points.is_empty() {
            scene.text(
                [x + 18., y + 12.],
                format!("{} N/C", super::short(&curve.id, 25)),
                &curve.evidence,
            );
            continue;
        }
        scene.path(
            &curve.id,
            curve.points.iter().copied().map(point).collect(),
            false,
            if curve.envelope {
                Stroke::Envelope
            } else {
                Stroke::Data
            },
            &curve.evidence,
        );
    }
    let key = curves.first().map_or("layout", |c| c.evidence.as_str());
    scene.text([x, y + 9.], format!("{:.2}", range[3]), key);
    scene.text([x, y + h - 9.], format!("{:.2}", range[2]), key);
    scene.text([x + 18., y + h - 4.], format!("{:.2}", range[0]), key);
    scene.text([x + w - 20., y + h - 4.], format!("{:.2}", range[1]), key);
    scene.text([x + 18., y + h], super::short(labels[0], 40), key);
    scene.text([x + 28., y + 7.], super::short(labels[1], 30), key);
}

pub(super) fn time_axis(role: CurveRole, unit: Unit) -> Unit {
    if role == CurveRole::Energy && unit.compatible(Unit::Hour) {
        Unit::Hour
    } else {
        unit
    }
}

pub(super) fn display_units(scene: &mut Scene, curves: &[Curve], units: [Unit; 2]) {
    for curve in curves {
        if let Some(evidence) = scene.evidence.get_mut(&curve.evidence) {
            evidence.units = units.to_vec();
        }
    }
}
