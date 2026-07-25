use std::collections::BTreeMap;

use crate::domain::diagnostic::AexError;
use crate::domain::schema::RawWing;

use super::{AREA_PATH, ASPECT_RATIO_PATH, SPAN_PATH, complete_planform_overrides, resolve_wing};

fn raw_wing(area: Option<f64>, span: Option<f64>, aspect_ratio: Option<f64>) -> RawWing {
    RawWing {
        area: area.map(|value| format!("{value} m^2")),
        span: span.map(|value| format!("{value} m")),
        aspect_ratio,
        sweep_quarter_chord: "0 deg".to_owned(),
        center_body_edge_sweep: None,
    }
}

fn assert_closed(wing: &crate::domain::schema::Wing) {
    assert!((wing.span_m.powi(2) / wing.area_m2 - wing.aspect_ratio).abs() < 1.0e-12);
}

#[test]
fn every_pair_derives_a_closed_planform() -> Result<(), Box<dyn std::error::Error>> {
    let from_area_span = resolve_wing(&raw_wing(Some(2.0), Some(4.0), None))?;
    let from_area_ratio = resolve_wing(&raw_wing(Some(2.0), None, Some(8.0)))?;
    let from_span_ratio = resolve_wing(&raw_wing(None, Some(4.0), Some(8.0)))?;

    for wing in [&from_area_span, &from_area_ratio, &from_span_ratio] {
        assert_closed(wing);
        assert!((wing.area_m2 - 2.0).abs() < 1.0e-12);
        assert!((wing.span_m - 4.0).abs() < 1.0e-12);
        assert!((wing.aspect_ratio - 8.0).abs() < 1.0e-12);
    }
    Ok(())
}

#[test]
fn rounded_triple_tolerance_is_inclusive_and_normalized() -> Result<(), Box<dyn std::error::Error>>
{
    let boundary = resolve_wing(&raw_wing(Some(100.0), Some(10.0), Some(1.005)))?;

    assert_closed(&boundary);
    assert!((boundary.aspect_ratio - 1.0).abs() < 1.0e-12);
    assert!(matches!(
        resolve_wing(&raw_wing(Some(100.0), Some(10.0), Some(1.005_001))),
        Err(AexError::Validation {
            code: "INCONSISTENT_WING_PLANFORM",
            ..
        })
    ));
    Ok(())
}

#[test]
fn fewer_than_two_values_is_rejected() {
    for wing in [
        raw_wing(None, None, None),
        raw_wing(Some(16.0), None, None),
        raw_wing(None, Some(11.0), None),
        raw_wing(None, None, Some(7.5)),
    ] {
        assert!(matches!(
            resolve_wing(&wing),
            Err(AexError::Validation {
                code: "INCOMPLETE_WING_PLANFORM",
                ..
            })
        ));
    }
}

#[test]
fn generated_planforms_close_for_all_input_pairs() -> Result<(), Box<dyn std::error::Error>> {
    for area in [8.0_f64, 16.17, 84.0, 436.8] {
        for aspect_ratio in [1.69_f64, 7.49, 9.61, 10.8] {
            let span = (area * aspect_ratio).sqrt();
            for wing in [
                raw_wing(Some(area), Some(span), None),
                raw_wing(Some(area), None, Some(aspect_ratio)),
                raw_wing(None, Some(span), Some(aspect_ratio)),
            ] {
                assert_closed(&resolve_wing(&wing)?);
            }
        }
    }
    Ok(())
}

#[test]
fn sweep_completion_closes_every_planform_variable_combination()
-> Result<(), Box<dyn std::error::Error>> {
    let baseline = resolve_wing(&raw_wing(Some(16.0), Some(8.0), Some(4.0)))?;
    let cases = [
        BTreeMap::from([(AREA_PATH.to_owned(), "20 m^2".to_owned())]),
        BTreeMap::from([(SPAN_PATH.to_owned(), "10 m".to_owned())]),
        BTreeMap::from([(ASPECT_RATIO_PATH.to_owned(), "5".to_owned())]),
        BTreeMap::from([
            (AREA_PATH.to_owned(), "20 m^2".to_owned()),
            (SPAN_PATH.to_owned(), "10 m".to_owned()),
        ]),
        BTreeMap::from([
            (AREA_PATH.to_owned(), "20 m^2".to_owned()),
            (ASPECT_RATIO_PATH.to_owned(), "5".to_owned()),
        ]),
        BTreeMap::from([
            (SPAN_PATH.to_owned(), "10 m".to_owned()),
            (ASPECT_RATIO_PATH.to_owned(), "5".to_owned()),
        ]),
    ];

    for overrides in cases {
        let completed = complete_planform_overrides(&overrides, &baseline)?;
        let wing = resolve_wing(&RawWing {
            area: completed.get(AREA_PATH).cloned(),
            span: completed.get(SPAN_PATH).cloned(),
            aspect_ratio: completed
                .get(ASPECT_RATIO_PATH)
                .map(|value| value.parse::<f64>())
                .transpose()?,
            sweep_quarter_chord: "0 deg".to_owned(),
            center_body_edge_sweep: None,
        })?;
        assert_closed(&wing);
    }
    Ok(())
}
