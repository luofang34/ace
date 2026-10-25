use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ace_engine::{Assessment, Margin};
use ace_model::{ModelEdit, Source, VehicleModel};
use serde::Serialize;

use crate::domain::diagnostic::{AexError, AexResult};
use crate::services::vehicle_document::{DocumentFile, create_blocking};

/// Changes to one vehicle file, applied as one validated revision.
pub(crate) struct VehicleEdit<'a> {
    /// Structural declarations, applied before parameter values.
    pub edits: &'a [ModelEdit],
    /// Parameter assignments as `"value unit"` strings.
    pub updates: &'a BTreeMap<String, String>,
    /// Refuse the edit unless the file is at this model revision.
    pub expected_revision: Option<u64>,
    /// Write the edited vehicle to this new file and leave the source unchanged.
    pub save_as: Option<&'a Path>,
}

/// The saved revision and the synthesized verdict of the edited vehicle.
#[derive(Debug, Serialize)]
pub(crate) struct EditOutcome {
    pub path: PathBuf,
    pub revision: u64,
    pub model_digest: String,
    /// Graph nodes whose results the edit invalidated.
    pub affected: Vec<String>,
    pub completed: bool,
    pub assessment: Assessment,
    pub margins: BTreeMap<String, Margin>,
}

/// Apply `edit` to the vehicle at `path`, synthesize it, and save it only when it changed.
pub(crate) fn edit_blocking(path: &Path, edit: &VehicleEdit<'_>) -> AexResult<EditOutcome> {
    let mut file = DocumentFile::open_blocking(path.to_path_buf())?;
    if let Some(expected) = edit.expected_revision
        && expected != file.model().revision
    {
        return Err(AexError::validation(
            "REVISION_CONFLICT",
            "expected_revision",
            format!(
                "{} is at revision {}, not {expected}",
                path.display(),
                file.model().revision
            ),
        ));
    }
    let transaction = transaction(file.model(), edit)?;
    let mut session =
        super::synthesis::create_session_blocking(file.model().clone(), &BTreeMap::new())?;
    let affected = session.transact(&transaction)?;
    let snapshot = session.run()?;
    let margins = ace_engine::evaluate_requirements(session.model(), &snapshot)?;
    let assessment = ace_engine::assess(session.model(), &snapshot)?;
    let saved = match edit.save_as {
        Some(target) => {
            create_blocking(target, session.model())?;
            target.to_path_buf()
        }
        None => {
            if session.model() != file.model() {
                file.save_blocking(session.model())?;
            }
            path.to_path_buf()
        }
    };
    Ok(EditOutcome {
        path: saved,
        revision: session.model().revision,
        model_digest: snapshot.model_digest.clone(),
        affected,
        completed: snapshot.complete(),
        assessment,
        margins,
    })
}

/// Typed edits followed by assignments rewritten as parameter declarations, so the whole
/// change is one transaction and one model revision.
fn transaction(model: &VehicleModel, edit: &VehicleEdit<'_>) -> AexResult<Vec<ModelEdit>> {
    let mut edited = model.clone();
    for change in edit.edits {
        change
            .apply(&mut edited)
            .map_err(ace_engine::EngineError::from)?;
    }
    let mut transaction = edit.edits.to_vec();
    for (name, value) in super::synthesis::parse_updates(edit.updates)? {
        let computed = edited
            .analyses
            .values()
            .any(|analysis| analysis.outputs.values().any(|output| output == &name));
        let mut parameter = edited
            .parameter(&name)
            .map_err(ace_engine::EngineError::from)?
            .clone();
        if computed || parameter.expression.is_some() {
            return Err(AexError::validation(
                "COMPUTED_PARAMETER",
                name,
                "computed outputs cannot be assigned",
            ));
        }
        parameter.value = Some(
            parameter
                .check_value(value)
                .map_err(ace_engine::EngineError::from)?,
        );
        parameter.source = Source {
            kind: "user".into(),
            reference: "vehicle edit".into(),
            revision: None,
        };
        transaction.push(ModelEdit::Parameter {
            id: name,
            value: Some(parameter),
        });
    }
    Ok(transaction)
}
