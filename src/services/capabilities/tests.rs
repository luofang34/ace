use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use crate::backends::contracts::{BackendDescriptor, BackendTopologyCapabilities};
use crate::domain::capabilities::{
    SegmentFieldCapability, SegmentFieldRequirement, aero_configurations, document_types,
    energy_schedule_fields, mission_initial_state_fields, mission_segments, profile_types,
    requirement_metrics, requirement_templates,
};

use super::{manifest, reference_markdown};

fn test_backends() -> Vec<BackendDescriptor> {
    vec![
        backend("native", true, Some("built-in"), None),
        backend(
            "openvsp",
            false,
            None,
            Some("not configured for the golden manifest"),
        ),
    ]
}

fn backend(
    id: &str,
    available: bool,
    version: Option<&str>,
    unavailable_reason: Option<&str>,
) -> BackendDescriptor {
    BackendDescriptor {
        id: id.to_owned(),
        display_name: format!("{id} test backend"),
        available,
        version: version.map(str::to_owned),
        capabilities: vec!["geometry".to_owned()],
        disciplines: vec!["geometry".to_owned()],
        fidelity_levels: vec![1],
        topology: BackendTopologyCapabilities {
            component_kinds: vec!["wing".to_owned()],
            relationship_kinds: vec!["attached_to".to_owned()],
            delegated_relationship_kinds: Vec::new(),
        },
        unavailable_reason: unavailable_reason.map(str::to_owned),
    }
}

#[test]
fn registries_are_unique_and_lookup_complete() {
    assert_unique(document_types().iter().map(|item| item.id));
    assert_unique(profile_types().iter().map(|item| item.id));
    assert_unique(aero_configurations().iter().map(|item| item.id));
    assert_unique(mission_segments().iter().map(|item| item.segment_type));
    assert_field_groups(mission_initial_state_fields());
    assert_field_groups(energy_schedule_fields());
    assert_segment_field_groups();
    assert_unique(requirement_metrics().iter().map(|item| item.id));
    assert_unique(requirement_templates().iter().map(|item| item.id));
    for template in requirement_templates() {
        assert_unique(template.items.iter().map(|item| item.id));
        assert!(template.items.iter().all(|item| {
            requirement_metrics()
                .iter()
                .any(|metric| metric.id == item.metric)
        }));
    }
    assert!(
        requirement_metrics()
            .iter()
            .filter(|item| !item.bindable)
            .all(|item| item.replacement.is_some())
    );
}

fn assert_segment_field_groups() {
    for segment in mission_segments() {
        assert_field_groups(segment.fields);
    }
}

fn assert_field_groups(fields: &[SegmentFieldCapability]) {
    assert_unique(fields.iter().map(|field| field.name));
    for field in fields {
        match field.requirement {
            SegmentFieldRequirement::Required | SegmentFieldRequirement::Optional => {
                assert!(field.alternative_group.is_none());
            }
            SegmentFieldRequirement::ExactlyOne | SegmentFieldRequirement::AtMostOne => {
                let group = field.alternative_group;
                assert!(group.is_some());
                assert!(fields.iter().all(|peer| {
                    peer.alternative_group != group || peer.requirement == field.requirement
                }));
                assert!(
                    fields
                        .iter()
                        .filter(|peer| peer.alternative_group == group)
                        .count()
                        >= 2
                );
            }
        }
    }
}

#[test]
fn manifest_matches_golden_file() -> Result<(), Box<dyn std::error::Error>> {
    let value = serde_json::to_vec(&manifest(test_backends())?)?;
    let digest = hex::encode(Sha256::digest(value));
    assert_eq!(
        digest,
        include_str!("../../../tests/golden/capabilities-manifest.sha256").trim()
    );
    Ok(())
}

#[test]
fn generated_reference_matches_manifest() -> Result<(), Box<dyn std::error::Error>> {
    let value = reference_markdown(&manifest(test_backends())?);
    assert_eq!(value, include_str!("../../../docs/capabilities.md"));
    Ok(())
}

fn assert_unique<'a>(items: impl Iterator<Item = &'a str>) {
    let items = items.collect::<Vec<_>>();
    let unique = items.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(items.len(), unique.len());
}
