use super::{
    arrangement::{Page, heading},
    imported,
};
use crate::{DrawingInput, DrawingResult, Scene, Stroke};
use ace_engine::MarginStatus;
use ace_model::{Priority, Unit};
use ace_sheet::Sheet;

pub(super) fn draw(
    scene: &mut Scene,
    sheet: &Sheet,
    input: &DrawingInput,
    page: &Page,
) -> DrawingResult<()> {
    mass(scene, input, page)?;
    requirements(scene, sheet, page);
    configuration(scene, sheet, input, page);
    supplied(scene, input, page)?;
    notes(scene, sheet, input, page);
    Ok(())
}

fn mass(scene: &mut Scene, input: &DrawingInput, page: &Page) -> DrawingResult<()> {
    let [x, w] = page.left;
    heading(scene, [x, 32.], w, "MASS / LOADED STATE", "layout");
    let mut total = (!input.model.mass_items.is_empty()).then_some(0.);
    for (i, (id, item)) in input.model.mass_items.iter().enumerate() {
        let value = crate::data::resolve(&item.mass, Unit::Kilogram, input)?;
        total = total.zip(value).map(|(a, b)| a + b);
        if i >= 8 {
            continue;
        }
        let key = format!("mass:{id}");
        scene.text([x, 40. + i as f64 * 5.], super::short(id, 24), &key);
        scene.text(
            [x + 41., 40. + i as f64 * 5.],
            value
                .map(|v| format!("{v:.1} kg"))
                .unwrap_or_else(|| "N/C".into()),
            &key,
        );
    }
    let y = 43. + input.model.mass_items.len().min(8) as f64 * 5.;
    scene.line([x, y - 2.], [x + w, y - 2.], Stroke::Annotation, "layout");
    scene.text([x, y + 2.], "Total loaded", "mass:total");
    scene.text(
        [x + 41., y + 2.],
        total
            .map(|v| format!("{v:.1} kg"))
            .unwrap_or_else(|| "N/C".into()),
        "mass:total",
    );
    if input.model.mass_items.len() > 8 {
        scene.text(
            [x, y + 8.],
            format!(
                "8/{} items shown; full mass register retained",
                input.model.mass_items.len()
            ),
            "layout",
        );
    }
    if input.model.mass_items.is_empty() {
        scene.text([x, 49.], "Mass statement N/C", "layout");
    }
    scene.text([x, 105.], "Lines: outline / hidden / centerline", "layout");
    scene.text(
        [x, 110.],
        "Heavy: supplied structure; hatch: material",
        "layout",
    );
    scene.text(
        [x, 115.],
        "Dashed: occupancy or declared CG limit",
        "layout",
    );
    Ok(())
}

fn requirements(scene: &mut Scene, sheet: &Sheet, page: &Page) {
    let [x, w] = page.right;
    heading(scene, [x, 32.], w, "REQUIREMENTS / MARGINS", "layout");
    for (i, id) in sheet.attention.iter().take(7).enumerate() {
        let Some(margin) = sheet.margins.get(id) else {
            continue;
        };
        let key = format!("requirement:{id}");
        let priority = sheet.model.requirements.get(id).map(|r| r.priority);
        let class = match priority {
            Some(Priority::Hard) => "H",
            Some(Priority::Soft) => "S",
            _ => "R",
        };
        let status = match margin.status {
            MarginStatus::Pass => "PASS",
            MarginStatus::Warn => "WARN",
            MarginStatus::Fail => "FAIL",
            MarginStatus::Unknown => "UNKNOWN",
            MarginStatus::NotApplicable => "N/A",
        };
        let color = if margin.status == MarginStatus::Fail && priority == Some(Priority::Hard) {
            Stroke::Failure
        } else {
            Stroke::Annotation
        };
        let value = margin
            .absolute
            .map(|q| format!("{:+.2} {}", q.value, imported::unit(q.unit)))
            .unwrap_or_else(|| "N/C".into());
        let y = 40. + i as f64 * 8.;
        scene.label(
            [x, y],
            format!("{} [{class}]", super::short(id, 35)),
            2.5,
            false,
            color,
            &key,
        );
        scene.label(
            [x, y + 3.8],
            format!(
                "{status}{} {value}",
                if margin.provisional { " [PROV]" } else { "" }
            ),
            2.5,
            true,
            color,
            &key,
        );
    }
    scene.text(
        [x, 100.],
        format!(
            "{}/{}; H hard / S soft / R report",
            sheet.attention.len().min(7),
            sheet.attention.len()
        ),
        "layout",
    );
}

fn configuration(scene: &mut Scene, sheet: &Sheet, input: &DrawingInput, page: &Page) {
    let [x, w] = page.right;
    heading(scene, [x, 110.], w, "CONFIGURATION / SOURCE", "layout");
    let selected = input
        .data
        .as_ref()
        .filter(|d| !d.configuration_parameters.is_empty())
        .map(|d| d.configuration_parameters.clone())
        .unwrap_or_else(|| {
            sheet
                .model
                .parameters
                .iter()
                .filter(|(_, p)| {
                    p.value.is_some()
                        && matches!(p.unit, Unit::Meter | Unit::SquareMeter | Unit::CubicMeter)
                })
                .map(|(id, _)| id.clone())
                .collect()
        });
    for (i, id) in selected.iter().take(5).enumerate() {
        let key = format!("parameter:{id}");
        let value = sheet
            .snapshot
            .values
            .get(id)
            .filter(|v| v.usable())
            .and_then(|v| v.quantity)
            .map(|q| format!("{:.2} {}", q.value, imported::unit(q.unit)))
            .unwrap_or_else(|| "N/C".into());
        scene.text([x, 118. + i as f64 * 8.], super::short(id, 38), &key);
        scene.text([x + 8., 122. + i as f64 * 8.], value, &key);
    }
    if selected.len() > 5 {
        scene.text(
            [x, 160.],
            format!("5/{}; full parameter register retained", selected.len()),
            "layout",
        );
    }
}

fn supplied(scene: &mut Scene, input: &DrawingInput, page: &Page) -> DrawingResult<()> {
    let [x, w] = page.right;
    heading(scene, [x, 171.], w, "SUPPLIED RESULTS / BASIS", "layout");
    let values = input
        .data
        .as_ref()
        .map(|d| d.results.statements.as_slice())
        .unwrap_or(&[]);
    for (i, value) in values.iter().take(2).enumerate() {
        let key = format!("statement:{i}");
        let y = 179. + i as f64 * 18.;
        scene.text([x, y], super::short(&value.label, 42), &key);
        let display = imported::value(value, input)?;
        let (display, margin) = display.split_once("; ").unwrap_or((&display, ""));
        scene.text([x, y + 4.], super::short(display, 46), &key);
        scene.text([x, y + 8.], super::short(margin, 46), &key);
        scene.text(
            [x, y + 12.],
            super::short(&imported::source(input, &value.evidence), 42),
            &key,
        );
    }
    if values.is_empty() {
        scene.text([x, 181.], "External results: N/C", "layout");
        if let Some(operations) = &input.model.operations
            && let Some(basis) = &operations.basis
        {
            scene.text(
                [x, 188.],
                format!("Basis: {} / {}", basis.currency, basis.price_year),
                "operations:basis",
            );
            scene.text(
                [x, 194.],
                super::short(&basis.cost_scope, 42),
                "operations:basis",
            );
        }
    }
    if !values.is_empty() {
        scene.text(
            [x, 220.],
            format!(
                "{}/{}; full results/equations retained",
                values.len().min(2),
                values.len()
            ),
            "layout",
        );
    }
    Ok(())
}

fn notes(scene: &mut Scene, sheet: &Sheet, input: &DrawingInput, page: &Page) {
    let [x, _] = page.right;
    let y = page.footer - 25.;
    let source = input
        .data
        .as_ref()
        .map(|d| d.document_reference.as_str())
        .unwrap_or("");
    scene.text(
        [x, y],
        if source.is_empty() {
            "Reference: N/C".into()
        } else {
            super::short(source, 34)
        },
        "layout",
    );
    scene.text(
        [x, y + 5.],
        "Calculated != measured; maturity declared",
        "layout",
    );
    let evidence = input.data.as_ref().map_or(0, |d| d.evidence.len());
    scene.text(
        [x, y + 10.],
        format!("{evidence} sources; no implied compliance"),
        "layout",
    );
    let closure = if sheet.snapshot.closures.is_empty() {
        "N/C"
    } else if sheet.snapshot.closures.values().all(|c| c.converged) {
        "closed"
    } else {
        "NOT CLOSED"
    };
    scene.text(
        [x, y + 15.],
        format!("Closure: {closure}; calibration separate"),
        "layout",
    );
    if let Some(study) = &input.study {
        scene.text(
            [page.left[0], page.footer - 8.],
            format!(
                "{} study candidates retained; selected design shown",
                study.candidates.len()
            ),
            "study",
        );
    }
}
