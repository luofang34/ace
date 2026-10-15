use super::{panel, short};
use crate::{DrawingInput, DrawingResult, Scene, Stroke};
use ace_engine::MarginStatus;
use ace_model::Unit;
use ace_sheet::Sheet;

pub(super) fn draw(scene: &mut Scene, sheet: &Sheet, input: &DrawingInput) -> DrawingResult<()> {
    requirements(scene, sheet);
    weights(scene, sheet, &input.scenario)?;
    parameters(scene, sheet);
    mission(scene, sheet, &input.scenario)?;
    economics(scene, sheet, &input.scenario)?;
    notes(scene, sheet);
    Ok(())
}

fn requirements(scene: &mut Scene, sheet: &Sheet) {
    scene.panel(
        [8.0, 28.0, 80.0, 77.0],
        "01 / REQUIREMENTS + CLOSURE",
        "layout",
    );
    for (i, id) in sheet.attention.iter().take(10).enumerate() {
        let Some(margin) = sheet.margins.get(id) else {
            continue;
        };
        let y = 39.0 + i as f64 * 5.2;
        let reference = format!("requirement:{id}");
        let value = margin
            .absolute
            .map(|q| {
                format!(
                    "{:+.2} {}",
                    q.value,
                    match q.unit {
                        Unit::Meter => "m",
                        Unit::Kilogram => "kg",
                        Unit::DollarPerNauticalMile => "$/nmi",
                        _ => "",
                    }
                )
            })
            .unwrap_or_else(|| "N/C".into());
        let status = match margin.status {
            MarginStatus::Fail => "FAIL",
            MarginStatus::Unknown => "N/C",
            MarginStatus::NotApplicable => "N/A",
            _ if margin.provisional => "PROV",
            _ => "PASS",
        };
        let stroke = if margin.status == MarginStatus::Fail {
            Stroke::Failure
        } else {
            Stroke::Annotation
        };
        scene.label([10.0, y], short(id, 21), 2.5, false, stroke, &reference);
        scene.label(
            [48.0, y],
            format!("{status} {value}"),
            2.5,
            true,
            stroke,
            &reference,
        );
    }
    let closed = sheet.snapshot.closures.values().all(|c| c.converged);
    let iterations = sheet
        .snapshot
        .closures
        .values()
        .map(|c| c.iterations)
        .max()
        .unwrap_or(0);
    scene.text(
        [10.0, 95.0],
        format!(
            "Closure: {} / {iterations} iterations",
            if closed { "closed" } else { "NOT CLOSED" }
        ),
        "layout",
    );
    scene.text(
        [10.0, 101.0],
        format!(
            "Top {} of {} constraints; PROV = approximate",
            sheet.attention.len().min(10),
            sheet.attention.len()
        ),
        "layout",
    );
}

fn weights(scene: &mut Scene, sheet: &Sheet, scenario: &str) -> DrawingResult<()> {
    scene.panel(
        [8.0, 108.0, 80.0, 50.0],
        "02 / MASS, FUEL + USABLE VOLUME",
        "layout",
    );
    let rows = [
        ("Operating empty", "weight.oew", Unit::Kilogram, "kg"),
        ("Payload", "payload.mass", Unit::Kilogram, "kg"),
        ("MTOW", "weight.mtow", Unit::Kilogram, "kg"),
        ("Required fuel", "fuel.required", Unit::Kilogram, "kg"),
        ("Fuel loaded", "fuel.loaded", Unit::Kilogram, "kg"),
        ("Fuel capacity", "fuel.capacity", Unit::Kilogram, "kg"),
        (
            "Normal trip burn",
            "mission.block_fuel",
            Unit::Kilogram,
            "kg",
        ),
        (
            "Reserve branch burn",
            "mission.reserve_fuel",
            Unit::Kilogram,
            "kg",
        ),
        (
            "Usable tank volume",
            "fuel.available_volume",
            Unit::CubicMeter,
            "m3",
        ),
    ];
    let mission = panel(sheet, "vehicle.mission_sequence", Some(scenario));
    let report = mission
        .map(|p| serde_json::from_value::<ace_physics::MissionReport>(p.details.clone()))
        .transpose()?;
    for (i, (label, id, unit, suffix)) in rows.iter().enumerate() {
        let at = [10.0, 119.0 + i as f64 * 4.25];
        if ["mission.block_fuel", "mission.reserve_fuel"].contains(id) {
            let evidence = mission
                .map(|p| format!("analysis:{}", p.analysis))
                .unwrap_or_else(|| "layout".into());
            let amount = report.as_ref().map(|r| {
                if *id == "mission.block_fuel" {
                    r.block_fuel_kg
                } else {
                    r.reserve_fuel_kg
                }
            });
            scene.text(at, *label, &evidence);
            scene.text(
                [53.0, at[1]],
                amount
                    .map(|n| format!("{n:.0} kg"))
                    .unwrap_or_else(|| "NOT CALCULATED".into()),
                &evidence,
            );
        } else {
            row(scene, sheet, at, label, id, (*unit, suffix, 0));
        }
    }
    Ok(())
}

fn parameters(scene: &mut Scene, sheet: &Sheet) {
    scene.panel(
        [324.0, 28.0, 88.0, 48.0],
        "03 / CANONICAL CONFIGURATION",
        "layout",
    );
    for (i, (label, id, unit, suffix)) in [
        ("Span", "wing.span", Unit::Meter, "m"),
        ("Wing area", "wing.area", Unit::SquareMeter, "m2"),
        ("Aspect ratio", "wing.aspect_ratio", Unit::Ratio, ""),
        ("Length", "fuselage.length", Unit::Meter, "m"),
        ("Diameter", "fuselage.diameter", Unit::Meter, "m"),
        ("Seats available", "cabin.seats", Unit::Ratio, ""),
        (
            "Design range",
            "mission.distance",
            Unit::NauticalMile,
            "nmi",
        ),
        ("Tank translation", "tank.offset_x", Unit::Meter, "m"),
    ]
    .iter()
    .enumerate()
    {
        row(
            scene,
            sheet,
            [326.0, 39.0 + i as f64 * 4.7],
            label,
            id,
            (*unit, suffix, 2),
        );
    }
}

fn mission(scene: &mut Scene, sheet: &Sheet, scenario: &str) -> DrawingResult<()> {
    scene.panel(
        [324.0, 79.0, 88.0, 57.0],
        "04 / E2 MISSION - NORMAL TRIP",
        "layout",
    );
    let Some(panel) = panel(sheet, "vehicle.mission_sequence", Some(scenario)) else {
        scene.text([326.0, 92.0], "Mission NOT CALCULATED", "layout");
        return Ok(());
    };
    let report: ace_physics::MissionReport = serde_json::from_value(panel.details.clone())?;
    let evidence = format!("analysis:{}", panel.analysis);
    scene.text([326.0, 90.0], "Stage", &evidence);
    scene.text([361.0, 90.0], "burn kg", &evidence);
    scene.text([389.0, 90.0], "min", &evidence);
    for (i, stage) in report.stages.iter().take(7).enumerate() {
        let y = 96.0 + i as f64 * 4.5;
        scene.text([326.0, y], short(&stage.id, 15), &evidence);
        scene.text([361.0, y], format!("{:.0}", stage.fuel_burn_kg), &evidence);
        scene.text(
            [389.0, y],
            format!(
                "{:.1}",
                (stage.end.state.time.value - stage.start.state.time.value) / 60.0
            ),
            &evidence,
        );
    }
    scene.text(
        [326.0, 129.0],
        format!(
            "Reserve branches: {}; excluded from trip cost",
            report.branches.len()
        ),
        &evidence,
    );
    scene.text(
        [326.0, 134.0],
        format!(
            "Scenario {} | {}",
            short(scenario, 16),
            if report.completed {
                "complete"
            } else {
                "FAILED"
            }
        ),
        &evidence,
    );
    Ok(())
}

fn economics(scene: &mut Scene, sheet: &Sheet, scenario: &str) -> DrawingResult<()> {
    scene.panel(
        [324.0, 139.0, 88.0, 51.0],
        "05 / E4 OPERATIONS + COST BASIS",
        "layout",
    );
    let Some(panel) = panel(sheet, "vehicle.operations", None) else {
        scene.text([326.0, 152.0], "Operations NOT CALCULATED", "layout");
        return Ok(());
    };
    let report: ace_physics::OperationsReport = serde_json::from_value(panel.details.clone())?;
    let Some(selected) = report
        .scenarios
        .get(scenario)
        .or_else(|| (report.scenario.as_deref() == Some(scenario)).then_some(&report))
    else {
        scene.text(
            [326.0, 152.0],
            "Operating scenario NOT CALCULATED",
            "layout",
        );
        return Ok(());
    };
    let evidence = format!("analysis:{}", panel.analysis);
    let rows = economic_rows(sheet, selected, report.network_compatible);
    for (i, text) in rows.into_iter().enumerate() {
        scene.text([326.0, 150.0 + i as f64 * 4.4], text, &evidence);
    }
    Ok(())
}

fn notes(scene: &mut Scene, sheet: &Sheet) {
    scene.panel(
        [8.0, 222.0, 80.0, 57.0],
        "07 / ASSUMPTIONS + EVIDENCE",
        "layout",
    );
    let flight = if sheet
        .model
        .analyses
        .values()
        .any(|a| a.plugin == "aircraft.energy_path_phase")
    {
        "Energy paths declared; ground estimated."
    } else {
        "Flight phases: inspect solver evidence."
    };
    let notes = [
        "Twinjet concept family; supplied assumptions.",
        "Demand does not uniquely specify shape.",
        "Native body / diamond wing sections.",
        "No pylons, structure or hidden-line solution.",
        "Cruise: mass-driven polar + fixed TSFC.",
        flight,
        "Field length, loads, noise: NOT CALCULATED.",
        "No validated airfoils or uncertainty propagation.",
    ];
    for (i, text) in notes.iter().enumerate() {
        scene.text([10.0, 233.0 + i as f64 * 5.2], *text, "layout");
    }
    if sheet.snapshot.closures.is_empty() {
        scene.text([10.0, 276.0], "No sizing closure declared", "layout");
    }
}

pub(super) fn value(sheet: &Sheet, id: &str, unit: Unit) -> Option<f64> {
    sheet
        .snapshot
        .values
        .get(id)
        .filter(|v| v.usable())
        .and_then(|v| v.quantity)
        .and_then(|q| q.to(unit).ok())
        .map(|q| q.value)
}

fn row(
    scene: &mut Scene,
    sheet: &Sheet,
    at: [f64; 2],
    label: &str,
    id: &str,
    (unit, suffix, decimals): (Unit, &str, usize),
) {
    let evidence = if scene.evidence.contains_key(&format!("parameter:{id}")) {
        format!("parameter:{id}")
    } else {
        "layout".into()
    };
    scene.text(at, label, &evidence);
    let text = value(sheet, id, unit)
        .map(|v| format!("{v:.decimals$} {suffix}"))
        .unwrap_or_else(|| "NOT CALCULATED".into());
    scene.text([at[0] + 43.0, at[1]], text, &evidence);
}

fn economic_rows(
    sheet: &Sheet,
    selected: &ace_physics::OperationsReport,
    network: bool,
) -> [String; 9] {
    let basis = selected
        .basis
        .as_ref()
        .map(|b| format!("{} {} | declared cost scope", b.currency, b.price_year))
        .unwrap_or_else(|| "Basis unavailable".into());
    let rate = |id, unit, precision| {
        value(sheet, id, unit)
            .map(|v| format!("{v:.precision$}"))
            .unwrap_or_else(|| "N/C".into())
    };
    [
        basis,
        format!(
            "Trip {:.0} USD | CASM {:.5}",
            selected.trip_cost_usd, selected.casm_usd_per_nmi
        ),
        format!("USD / (available seat nmi); {} seats", selected.seats),
        format!(
            "Fuel {}/kg; crew {}/FH",
            rate("economics.fuel_price", Unit::DollarPerKilogram, 2),
            rate("economics.crew_rate", Unit::DollarPerHour, 0)
        ),
        format!(
            "Maintenance {}/FH + {}/FC",
            rate("economics.maintenance_rate", Unit::DollarPerHour, 0),
            rate("economics.maintenance_cycle", Unit::Dollar, 0)
        ),
        format!(
            "Ground {}/h; turnaround {} h",
            rate("economics.turnaround_rate", Unit::DollarPerHour, 0),
            rate("economics.turnaround", Unit::Hour, 2)
        ),
        format!(
            "Ownership {} USD/year",
            rate("economics.annual_ownership", Unit::Dollar, 0)
        ),
        format!(
            "Utilization: {} flights/year",
            selected
                .annual_flights
                .map(|n| format!("{n:.0}"))
                .unwrap_or_else(|| "N/C".into())
        ),
        format!(
            "Network compatibility: {}",
            if network { "YES" } else { "FAIL" }
        ),
    ]
}
