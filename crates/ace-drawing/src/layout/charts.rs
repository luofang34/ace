use super::panel;
use crate::{DrawingInput, DrawingResult, Evidence, Scene, Stroke};
use ace_sheet::Sheet;

pub(super) fn draw(scene: &mut Scene, sheet: &Sheet, input: &DrawingInput) -> DrawingResult<()> {
    cg(scene, sheet, &input.scenario)?;
    studies(scene, input);
    Ok(())
}

fn cg(scene: &mut Scene, sheet: &Sheet, scenario: &str) -> DrawingResult<()> {
    scene.panel(
        [8.0, 161.0, 80.0, 58.0],
        "06 / E3 CG - WEIGHT ENVELOPE",
        "layout",
    );
    let Some(panel) = panel(sheet, "vehicle.mission_cg_envelope", Some(scenario)) else {
        scene.text([10.0, 181.0], "CG envelope NOT CALCULATED", "layout");
        return Ok(());
    };
    let report: ace_physics::CgReport = serde_json::from_value(panel.details.clone())?;
    let evidence = format!("analysis:{}", panel.analysis);
    if report.points.is_empty() {
        return Ok(());
    }
    let low = report
        .points
        .iter()
        .map(|p| p.mass_kg)
        .fold(f64::INFINITY, f64::min)
        - 500.0;
    let high = report
        .points
        .iter()
        .map(|p| p.mass_kg)
        .fold(f64::NEG_INFINITY, f64::max)
        + 500.0;
    let min_x = report
        .envelope
        .iter()
        .map(|p| p.minimum_m)
        .fold(f64::INFINITY, f64::min);
    let max_x = report
        .envelope
        .iter()
        .map(|p| p.maximum_m)
        .fold(f64::NEG_INFINITY, f64::max);
    if !min_x.is_finite() || max_x <= min_x {
        scene.text([10.0, 181.0], "CG bounds unavailable", &evidence);
        return Ok(());
    }
    let frame = Plot {
        bounds: [23.0, 174.0, 61.0, 30.0],
        domain: [min_x - 0.1, max_x + 0.1, low, high],
    };
    frame.axes(scene, &evidence);
    limits(scene, &report, &frame, &evidence);
    let mut scenarios = std::collections::BTreeMap::<_, Vec<_>>::new();
    for p in &report.points {
        scenarios
            .entry(&p.scenario)
            .or_default()
            .push(frame.point([p.cg_x_m, p.mass_kg]));
    }
    for (scenario, points) in scenarios {
        scene.path(scenario, points, false, Stroke::Data, &evidence);
    }
    scene.text([10.0, 174.0], format!("{:.1} t", high / 1000.0), &evidence);
    scene.text([10.0, 204.0], format!("{:.1} t", low / 1000.0), &evidence);
    scene.text([23.0, 210.0], format!("{min_x:.1}"), &evidence);
    scene.text([59.0, 210.0], format!("{max_x:.1}  CG x / m"), &evidence);
    scene.text(
        [10.0, 216.0],
        "Sampled trajectory incl. reserve; limits dashed",
        &evidence,
    );
    Ok(())
}

fn studies(scene: &mut Scene, input: &DrawingInput) {
    scene.panel(
        [324.0, 194.0, 88.0, 85.0],
        "08 / BOUNDED DESIGN STUDY",
        "layout",
    );
    let Some(study) = &input.study else {
        scene.text([326.0, 208.0], "Study NOT CALCULATED", "layout");
        scene.text(
            [326.0, 214.0],
            "Drawing shows supplied design only.",
            "layout",
        );
        return;
    };
    scene.text(
        [326.0, 204.0],
        format!(
            "{} samples; x selected; no boundary",
            study.candidates.len()
        ),
        "study",
    );
    for (index, (metric, title, divisor)) in [
        ("payload.mass", "Payload / t", 1000.0),
        ("mission.block_fuel", "Block fuel / t", 1000.0),
        ("economics.casm", "CASM / USD/(seat nmi)", 1.0),
    ]
    .iter()
    .enumerate()
    {
        let samples = samples(scene, study, metric, *divisor);
        let y = 212.0 + index as f64 * 24.0;
        scene.text([326.0, y - 3.0], *title, "study");
        if samples.is_empty() {
            scene.text([326.0, y + 5.0], "NOT CALCULATED", "study");
            continue;
        }
        let domain = range(samples.iter().map(|(p, _, _)| *p));
        let frame = Plot {
            bounds: [342.0, y, 65.0, 13.0],
            domain,
        };
        frame.axes(scene, "study");
        for (p, key, feasible) in &samples {
            let [x, y] = frame.point(*p);
            if input
                .candidate
                .as_deref()
                .is_some_and(|id| *key == format!("candidate:{id}:{metric}"))
            {
                scene.line([x - 1.1, y - 1.1], [x + 1.1, y + 1.1], Stroke::Outline, key);
                scene.line([x - 1.1, y + 1.1], [x + 1.1, y - 1.1], Stroke::Outline, key);
            }
            let stroke = match feasible {
                ace_engine::Feasibility::Violated => Stroke::Failure,
                ace_engine::Feasibility::Provisional => Stroke::Envelope,
                ace_engine::Feasibility::Unknown => Stroke::Datum,
                ace_engine::Feasibility::Satisfied => Stroke::Data,
            };
            scene.path(
                "study-sample",
                vec![[x, y - 0.7], [x + 0.7, y], [x, y + 0.7], [x - 0.7, y]],
                true,
                stroke,
                key,
            );
        }
        scene.text([326.0, y + 3.0], format!("{:.3}", domain[3]), "study");
        scene.text([326.0, y + 13.0], format!("{:.3}", domain[2]), "study");
        scene.text([342.0, y + 17.0], format!("{:.1}", domain[0]), "study");
        scene.text([350.0, y + 17.0], "range / 1000 nmi", "study");
        scene.text([402.0, y + 17.0], format!("{:.1}", domain[1]), "study");
    }
}

struct Plot {
    bounds: [f64; 4],
    domain: [f64; 4],
}

impl Plot {
    fn point(&self, p: [f64; 2]) -> [f64; 2] {
        let [x, y, w, h] = self.bounds;
        let [a, b, c, d] = self.domain;
        [
            x + (p[0] - a) / (b - a) * w,
            y + h - (p[1] - c) / (d - c) * h,
        ]
    }
    fn axes(&self, scene: &mut Scene, evidence: &str) {
        let [x, y, w, h] = self.bounds;
        scene.path(
            "axes",
            vec![[x, y], [x, y + h], [x + w, y + h]],
            false,
            Stroke::Datum,
            evidence,
        );
    }
}

fn range(points: impl Iterator<Item = [f64; 2]>) -> [f64; 4] {
    let mut result = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for [x, y] in points {
        result[0] = result[0].min(x);
        result[1] = result[1].max(x);
        result[2] = result[2].min(y);
        result[3] = result[3].max(y);
    }
    for i in [0, 2] {
        let pad = ((result[i + 1] - result[i]) * 0.1)
            .max(result[i].abs() * 0.05)
            .max(0.0001);
        result[i] -= pad;
        result[i + 1] += pad;
    }
    result
}

fn limits(scene: &mut Scene, report: &ace_physics::CgReport, frame: &Plot, evidence: &str) {
    for stations in report.envelope.windows(2) {
        let (a, b) = (&stations[0], &stations[1]);
        let start = frame.domain[2].max(a.mass_kg);
        let end = frame.domain[3].min(b.mass_kg);
        if end < start {
            continue;
        }
        for (v0, v1) in [(a.minimum_m, b.minimum_m), (a.maximum_m, b.maximum_m)] {
            let pts = [start, end].map(|m| {
                frame.point([
                    v0 + (v1 - v0) * (m - a.mass_kg) / (b.mass_kg - a.mass_kg),
                    m,
                ])
            });
            scene.path("cg-limit", pts.to_vec(), false, Stroke::Envelope, evidence);
        }
    }
}

fn samples(
    scene: &mut Scene,
    study: &ace_study::StudyArchive,
    metric: &str,
    divisor: f64,
) -> Vec<([f64; 2], String, ace_engine::Feasibility)> {
    let mut samples = Vec::new();
    let unit = if metric == "economics.casm" {
        ace_model::Unit::DollarPerNauticalMile
    } else {
        ace_model::Unit::Kilogram
    };
    for c in &study.candidates {
        let q = |id: &str, unit| {
            c.state
                .values
                .get(id)
                .filter(|v| v.usable())
                .and_then(|v| v.quantity)
                .and_then(|q| q.to(unit).ok())
                .map(|q| q.value)
        };
        if let (Some(x), Some(y)) = (
            q("mission.distance", ace_model::Unit::NauticalMile),
            q(metric, unit),
        ) {
            let key = format!("candidate:{}:{metric}", c.id);
            if let Some(base) = scene.evidence.get(&format!("candidate:{}", c.id)).cloned() {
                scene.evidence.insert(
                    key.clone(),
                    Evidence {
                        source: metric.into(),
                        run_id: c.state.values.get(metric).and_then(|v| v.run_id.clone()),
                        ..base
                    },
                );
            }
            samples.push(([x / 1000.0, y / divisor], key, c.feasibility));
        }
    }
    samples
}
