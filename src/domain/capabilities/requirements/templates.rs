use super::{
    EngineCountValueCapability, RequirementProvenanceCapability, RequirementTemplateCapability,
    RequirementTemplateItemCapability, RequirementTemplateValue, TemplateParameterCapability,
};

const DESIGNER_DEFAULT: RequirementProvenanceCapability = RequirementProvenanceCapability {
    kind: "designer_default",
    source: "ACE conceptual-design default",
    citation: None,
    non_regulatory: true,
};

const TRANSPORT_OEI_SOURCE: RequirementProvenanceCapability = RequirementProvenanceCapability {
    kind: "regulatory_derived",
    source: "14 CFR 25.121(b), second-segment one-engine-inoperative climb",
    citation: Some("https://www.ecfr.gov/current/title-14/section-25.121"),
    non_regulatory: false,
};

const OEI_GRADIENTS: [EngineCountValueCapability; 3] = [
    EngineCountValueCapability {
        engine_count: 2,
        value: 0.024,
    },
    EngineCountValueCapability {
        engine_count: 3,
        value: 0.027,
    },
    EngineCountValueCapability {
        engine_count: 4,
        value: 0.030,
    },
];

const LIGHT_ITEMS: [RequirementTemplateItemCapability; 5] = [
    item(
        "stall_speed_landing",
        "performance.stall_speed_landing",
        "le",
        RequirementTemplateValue::Quantity("61 kt"),
    ),
    item(
        "takeoff_field_length",
        "performance.takeoff_field_length",
        "le",
        RequirementTemplateValue::Quantity("2500 ft"),
    ),
    item(
        "landing_field_length",
        "performance.landing_field_length",
        "le",
        RequirementTemplateValue::Quantity("2500 ft"),
    ),
    item(
        "all_engine_climb_gradient",
        "performance.all_engine_climb_gradient",
        "ge",
        RequirementTemplateValue::Scalar(0.05),
    ),
    item(
        "reserve_duration",
        "mission.reserve_duration",
        "ge",
        RequirementTemplateValue::Quantity("45 min"),
    ),
];

const TRANSPORT_ITEMS: [RequirementTemplateItemCapability; 5] = [
    item(
        "stall_speed_landing",
        "performance.stall_speed_landing",
        "le",
        RequirementTemplateValue::Quantity("150 kt"),
    ),
    item(
        "takeoff_field_length",
        "performance.takeoff_field_length",
        "le",
        RequirementTemplateValue::Quantity("11000 ft"),
    ),
    item(
        "landing_field_length",
        "performance.landing_field_length",
        "le",
        RequirementTemplateValue::Quantity("8000 ft"),
    ),
    item(
        "reserve_duration",
        "mission.reserve_duration",
        "ge",
        RequirementTemplateValue::Quantity("30 min"),
    ),
    RequirementTemplateItemCapability {
        id: "oei_second_segment_climb_gradient",
        metric: "performance.oei_second_segment_climb_gradient",
        operator: "ge",
        value: None,
        values_by_engine_count: &OEI_GRADIENTS,
        severity: "hard",
        weight: None,
        provenance: TRANSPORT_OEI_SOURCE,
    },
];

const TEMPLATES: [RequirementTemplateCapability; 2] = [
    RequirementTemplateCapability {
        id: "light_aircraft_conceptual",
        version: 1,
        display_name: "Light-aircraft conceptual defaults",
        category: "normal-category light aircraft",
        certification_use: "conceptual_screen_only",
        parameter: None,
        items: &LIGHT_ITEMS,
    },
    RequirementTemplateCapability {
        id: "transport_conceptual",
        version: 1,
        display_name: "Transport-aircraft conceptual defaults",
        category: "transport",
        certification_use: "conceptual_screen_only",
        parameter: Some(TemplateParameterCapability {
            name: "engine_count",
            required: true,
            allowed_values: &[2, 3, 4],
        }),
        items: &TRANSPORT_ITEMS,
    },
];

pub(crate) const fn requirement_templates() -> &'static [RequirementTemplateCapability] {
    &TEMPLATES
}

pub(crate) fn requirement_template(
    id: &str,
    version: u32,
) -> Option<RequirementTemplateCapability> {
    TEMPLATES
        .iter()
        .copied()
        .find(|template| template.id == id && template.version == version)
}

const fn item(
    id: &'static str,
    metric_id: &'static str,
    operator: &'static str,
    value: RequirementTemplateValue,
) -> RequirementTemplateItemCapability {
    RequirementTemplateItemCapability {
        id,
        metric: metric_id,
        operator,
        value: Some(value),
        values_by_engine_count: &[],
        severity: "soft",
        weight: None,
        provenance: DESIGNER_DEFAULT,
    }
}
