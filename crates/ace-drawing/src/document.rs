use crate::{Branding, DrawingError, DrawingResult, Scene};
use ace_engine::Snapshot;
use ace_model::VehicleModel;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Supported landscape engineering sheet sizes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Paper {
    /// 420 by 297 mm.
    A3,
    /// 841 by 594 mm, with the same content at twice the linear size.
    A1,
    /// 480 by 270 mm, independently composed for 16:9 presentation.
    #[serde(rename = "16:9")]
    Widescreen,
}

/// Frozen authoritative input, including optional bounded-search evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawingInput {
    /// Canonical selected vehicle design.
    pub model: VehicleModel,
    /// Computed state of exactly this design revision.
    pub snapshot: Snapshot,
    /// Mission and operating scenario shown in the drawing.
    pub scenario: String,
    /// Full verified study if a search candidate was selected.
    pub study: Option<ace_study::StudyArchive>,
    /// Selected candidate content address; absent for a directly supplied design.
    pub candidate: Option<String>,
    /// Optional imported geometry/results and configurable drawing content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<crate::DrawingData>,
}

/// Portable single-page drawing and its exact scientific inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawingDocument {
    /// Drawing schema contract.
    pub schema: String,
    /// Content address of the whole document with this field empty.
    pub digest: String,
    /// Original input and calculation evidence.
    pub input: DrawingInput,
    /// Selected sheet size.
    pub paper: Paper,
    /// Renderer build identity.
    pub generator: ace_engine::BuildIdentity,
    /// Frozen renderer-independent scene.
    pub scene: Scene,
    /// Frozen presentation identity; absent preserves the original drawing format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branding: Option<Branding>,
    /// Caller-defined figure identity; absent for the complete synthesis sheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure: Option<crate::FigureSpec>,
}

/// Compose a readable engineering drawing without changing or recomputing its source design.
pub fn compose(input: DrawingInput, paper: Paper) -> DrawingResult<DrawingDocument> {
    compose_branded(input, paper, Branding::default())
}

/// Compose a customer-branded drawing while preserving source model and run identities.
pub fn compose_branded(
    input: DrawingInput,
    paper: Paper,
    branding: Branding,
) -> DrawingResult<DrawingDocument> {
    branding.validate()?;
    verify_input(&input)?;
    let sheet = ace_sheet::project(&input.model, &input.snapshot)?;
    let scene = crate::layout::compose(&sheet, &input, paper, &branding)?;
    let mut document = DrawingDocument {
        schema: "ace.synthesis-drawing/1".into(),
        digest: String::new(),
        input,
        paper,
        generator: ace_engine::implementation_identity(),
        scene,
        branding: Some(branding),
        figure: None,
    };
    document.digest = digest(&document)?;
    Ok(document)
}

pub(crate) fn figure(
    document: &DrawingDocument,
    spec: &crate::FigureSpec,
) -> DrawingResult<DrawingDocument> {
    let sheet = ace_sheet::project(&document.input.model, &document.input.snapshot)?;
    let mut result = document.clone();
    result.figure = Some(spec.clone());
    result.scene = crate::layout::figure(&sheet, &document.input, spec)?;
    result.digest.clear();
    result.digest = digest(&result)?;
    Ok(result)
}

impl DrawingDocument {
    /// Verify frozen content, calculation hashes, geometry artifacts and revision correspondence.
    pub fn verify(&self) -> DrawingResult<()> {
        let mut check = self.clone();
        check.digest.clear();
        if self.schema != "ace.synthesis-drawing/1" || digest(&check)? != self.digest {
            return Err(DrawingError::Invalid(
                "document content address mismatch".into(),
            ));
        }
        if let Some(branding) = &self.branding {
            branding.validate()?;
        }
        verify_input(&self.input)?;
        crate::layout::validate(&self.scene)
    }

    /// Serialize vector SVG from the shared scene; text retains evidence keys and object IDs.
    pub fn svg(&self) -> DrawingResult<String> {
        self.verify()?;
        Ok(crate::render::svg(
            &self.scene,
            &self.digest,
            self.branding.as_ref(),
        ))
    }

    /// Serialize one vector PDF page using the same millimetre scene and standard PDF fonts.
    pub fn pdf(&self) -> DrawingResult<Vec<u8>> {
        self.verify()?;
        Ok(crate::render::pdf(
            &self.scene,
            &self.digest,
            self.branding.as_ref(),
        ))
    }
}

fn verify_input(input: &DrawingInput) -> DrawingResult<()> {
    let snapshot = &input.snapshot;
    input.model.validate()?;
    if let Some(data) = &input.data {
        crate::data::validate(data, input)?;
    }
    if snapshot.revision != input.model.revision
        || snapshot.vehicle_id != input.model.id
        || digest(&input.model)? != snapshot.model_digest
    {
        return Err(DrawingError::Invalid(
            "model and snapshot revisions differ".into(),
        ));
    }
    for run in &snapshot.runs {
        let mut check = run.clone();
        check.id.clear();
        if digest(&check)? != run.id {
            return Err(DrawingError::Invalid(format!("altered run {}", run.id)));
        }
        for artifact in &run.artifacts {
            artifact.validate()?;
        }
    }
    verify_values(input)?;
    match (&input.study, &input.candidate) {
        (Some(study), Some(id)) => {
            study.verify()?;
            let candidate = study.snapshot(id)?;
            if candidate.model_digest != snapshot.model_digest
                || candidate.values != snapshot.values
                || candidate.build != snapshot.build
            {
                return Err(DrawingError::Invalid(
                    "selected study candidate differs from drawing".into(),
                ));
            }
        }
        (None, None) => (),
        _ => {
            return Err(DrawingError::Invalid(
                "study and selected candidate must be supplied together".into(),
            ));
        }
    }
    Ok(())
}

fn verify_values(input: &DrawingInput) -> DrawingResult<()> {
    for (id, value) in input.snapshot.values.iter().filter(|(_, v)| v.usable()) {
        let expected = if let Some(run_id) = &value.run_id {
            input
                .snapshot
                .runs
                .iter()
                .find(|r| &r.id == run_id && r.status == value.status)
                .and_then(|run| run.outputs.get(id))
                .copied()
        } else if value.status == ace_engine::ValueStatus::Input {
            input.model.parameters.get(id).and_then(|p| p.value)
        } else {
            None
        };
        let matches = expected
            .zip(value.quantity)
            .is_some_and(|(expected, actual)| {
                expected.to(actual.unit).is_ok_and(|converted| {
                    // Canonical unit normalization can round even an unchanged angle by one ULP.
                    (converted.value - actual.value).abs()
                        <= 8. * f64::EPSILON * actual.value.abs().max(1.)
                })
            });
        if value.parameter != *id || !matches {
            return Err(DrawingError::Invalid(format!(
                "value {id} does not match its canonical input or producing run"
            )));
        }
    }
    Ok(())
}

fn digest(value: &impl Serialize) -> DrawingResult<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}
