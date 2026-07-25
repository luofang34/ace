use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::domain::diagnostic::AexError;
use crate::domain::evidence::StudyArchive;
use crate::domain::evidence::archive::StudyArchiveDraft;
use crate::domain::quantity::{Dimension, parse_quantity};
use crate::services::analysis::ApplicationService;

use super::{evaluation, loading};

fn example_study(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .join("study.yaml")
}

fn candidate_ids(result: &crate::domain::evidence::StudyRunResult) -> Vec<&str> {
    result
        .selected_candidates
        .iter()
        .map(|candidate| candidate.candidate.candidate_id.as_str())
        .collect()
}

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}

fn assert_no_candidate_directories_blocking(root: &Path) -> io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            assert_ne!(
                entry.file_name().to_string_lossy().to_ascii_lowercase(),
                "candidates"
            );
            assert_no_candidate_directories_blocking(&entry.path())?;
        }
    }
    Ok(())
}

fn assert_study_result_planforms(
    service: &ApplicationService,
    result: &crate::domain::evidence::StudyRunResult,
) -> Result<(), Box<dyn std::error::Error>> {
    for selected in &result.selected_candidates {
        let evidence = service.get_study_evidence_blocking(
            &result.study_id,
            Some(&result.archive_id),
            &selected.candidate.candidate_id,
        )?;
        let metric = |id: &str| {
            evidence
                .results
                .metrics
                .get(id)
                .map(|value| value.value)
                .ok_or_else(|| io::Error::other(format!("missing study metric {id}")))
        };
        let area = metric("geometry.wing_area")?;
        let span = metric("geometry.wing_span")?;
        let aspect_ratio = metric("geometry.aspect_ratio")?;
        assert!((span.powi(2) / area - aspect_ratio).abs() < 1.0e-12);
    }
    Ok(())
}

fn assert_reference_candidate_round_trips(
    service: &ApplicationService,
    study: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let prepared = loading::prepare_study_blocking(service, study)?;
    let descriptor = evaluation::candidate_descriptor(
        &prepared,
        &BTreeMap::from([
            ("aspect_ratio".to_owned(), "7.49".to_owned()),
            ("fuel_capacity".to_owned(), "144 kg".to_owned()),
            ("propulsion_sizing".to_owned(), "1.4".to_owned()),
            ("wing_area".to_owned(), "16.17 m^2".to_owned()),
        ]),
    )?;
    let operating_empty = descriptor
        .parameters
        .get("aircraft.mass.operating_empty_mass")
        .ok_or_else(|| io::Error::other("mass closure omitted operating empty mass"))?;
    let maximum_takeoff = descriptor
        .parameters
        .get("aircraft.mass.maximum_takeoff_mass")
        .ok_or_else(|| io::Error::other("mass closure omitted maximum takeoff mass"))?;
    let operating_empty = parse_quantity(operating_empty, Dimension::Mass)?;
    let maximum_takeoff = parse_quantity(maximum_takeoff, Dimension::Mass)?;
    let baseline = &prepared.scenario.aircraft.mass;
    assert!(operating_empty > baseline.operating_empty_mass_kg);
    assert!(
        ((maximum_takeoff - baseline.maximum_takeoff_mass_kg)
            - (operating_empty - baseline.operating_empty_mass_kg))
            .abs()
            < 1.0e-9
    );
    let resolved = service.resolve_blocking(prepared.scenario_path(), &descriptor.parameters)?;
    assert!(
        (resolved.aircraft.wing.span_m.powi(2) / resolved.aircraft.wing.area_m2
            - resolved.aircraft.wing.aspect_ratio)
            .abs()
            < 1.0e-12
    );
    let evaluated = evaluation::evaluate_candidate_blocking(service, &prepared, descriptor, 0)?;
    let decoded = serde_json::from_slice(&serde_json::to_vec(&evaluated.evidence)?)?;
    assert_eq!(evaluated.evidence, decoded);
    assert!(evaluated.outcome.feasible);
    Ok(())
}

fn discard_progress_after_blocking(
    service: &ApplicationService,
    study_id: &str,
    progress: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut found_checkpoint = false;
    for archive in service.studies.list_archives_blocking()? {
        if archive.study_id != study_id {
            continue;
        }
        let evaluations = archive.workflow.outcomes.len();
        found_checkpoint |= evaluations == progress && !archive.complete;
        if evaluations > progress || archive.complete {
            fs::remove_file(service.studies.archive_path(&archive.archive_id)?)?;
        }
    }
    assert!(found_checkpoint);
    Ok(())
}

#[test]
fn c172_study_reuses_evidence_and_promotes_one_design() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let study = example_study("c172");
    assert_reference_candidate_round_trips(&service, &study)?;

    let first = service.run_study_blocking(&study)?;
    let second = service.run_study_blocking(&study)?;
    let queried = service.query_study_blocking(&first.study_id, Some(&first.archive_id), 10)?;
    let diagnostic_candidate = first
        .selected_candidates
        .first()
        .ok_or_else(|| io::Error::other("C172 study selected no candidate"))?;
    let diagnostic_evidence = service.get_study_evidence_blocking(
        &first.study_id,
        Some(&first.archive_id),
        &diagnostic_candidate.candidate.candidate_id,
    )?;

    assert!(first.complete);
    assert_eq!(first.evaluated_candidates, 81);
    assert!(
        first.feasible_candidates > 0,
        "{:?} {:?}",
        diagnostic_candidate.candidate.parameters,
        diagnostic_evidence
            .results
            .metrics
            .get("mission.minimum_power_reserve_margin")
    );
    assert_eq!(first.archive_id, second.archive_id);
    assert_eq!(candidate_ids(&first), candidate_ids(&second));
    assert_eq!(candidate_ids(&first), candidate_ids(&queried));
    assert_eq!(second.reused_evaluations, first.evaluated_candidates);
    assert_study_result_planforms(&service, &first)?;

    let selected = first
        .selected_candidates
        .first()
        .ok_or_else(|| io::Error::other("C172 study selected no candidate"))?;
    let evidence = service.get_study_evidence_blocking(
        &first.study_id,
        Some(&first.archive_id),
        &selected.candidate.candidate_id,
    )?;
    assert_eq!(evidence.evaluation_id, selected.evidence_id);

    let designs = temporary.path().join("designs");
    let design = service.promote_study_candidate_blocking(
        &study,
        &selected.candidate.candidate_id,
        "c172-study-selection",
        "C172 Study Selection",
        &designs,
    )?;
    assert!(design.scenario_path.is_file());
    assert_eq!(fs::read_dir(&designs)?.count(), 1);

    discard_progress_after_blocking(&service, &first.study_id, 40)?;
    let resumed = service.run_study_blocking(&study)?;
    assert_eq!(resumed.reused_evaluations, 40);
    assert_eq!(resumed.archive_id, first.archive_id);
    assert_eq!(candidate_ids(&resumed), candidate_ids(&first));
    assert_no_candidate_directories_blocking(temporary.path())?;
    Ok(())
}

#[test]
fn b777_evolutionary_study_is_seeded_and_reusable() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let study = example_study("b777");

    let first = service.run_study_blocking(&study)?;
    let second = service.run_study_blocking(&study)?;

    assert!(first.complete);
    assert!(first.evaluated_candidates <= 120);
    assert!(first.evaluated_candidates > 0);
    assert_eq!(first.archive_id, second.archive_id);
    assert_eq!(candidate_ids(&first), candidate_ids(&second));
    assert_eq!(second.reused_evaluations, first.evaluated_candidates);
    assert_study_result_planforms(&service, &first)?;
    assert_no_candidate_directories_blocking(temporary.path())?;
    Ok(())
}

#[test]
fn reused_study_id_requires_an_archive_discriminator() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let service = ApplicationService::filesystem(temporary.path().join("runs"));
    let mut archive_ids = Vec::new();
    for study_digest in [digest('a'), digest('b')] {
        let archive = StudyArchive::from_draft(StudyArchiveDraft {
            study_id: "reused-study".to_owned(),
            study_digest,
            baseline_digest: digest('c'),
            evaluator_signature: "native-test".to_owned(),
            complete: true,
            candidates: Vec::new(),
            evaluation_ids: Vec::new(),
            selected_candidate_ids: Vec::new(),
            workflow: Default::default(),
        })?;
        archive_ids.push(archive.archive_id.clone());
        service.studies.save_archive_blocking(&archive)?;
    }

    assert!(matches!(
        service.query_study_blocking("reused-study", None, 1),
        Err(AexError::Validation {
            code: "AMBIGUOUS_STUDY_ARCHIVE",
            ..
        })
    ));
    let selected = service.query_study_blocking("reused-study", Some(&archive_ids[0]), 1)?;
    assert_eq!(selected.archive_id, archive_ids[0]);
    Ok(())
}
