use super::AppState;
use crate::domain::diagnostic::{AexError, AexResult};
use ace_study::StudyArchive;
use axum::{Json, extract::State};

pub(super) async fn import(
    State(state): State<AppState>,
    Json(archive): Json<StudyArchive>,
) -> Result<Json<StudyArchive>, super::api::ApiError> {
    let current = state.current.borrow().clone();
    let baseline = current.sheet.model.clone();
    let build = current.sheet.snapshot.build.digest.clone();
    let result = tokio::task::spawn_blocking(move || {
        validate(&archive, &baseline, &build)?;
        Ok::<_, AexError>(archive)
    })
    .await
    .map_err(|source| super::service_error("verify study import", source))??;
    if state.current.borrow().sheet.snapshot.model_digest != current.sheet.snapshot.model_digest {
        return Err(AexError::validation(
            "REVISION_CONFLICT",
            "study",
            "design changed during import",
        )
        .into());
    }
    Ok(Json(result))
}

fn validate(
    archive: &StudyArchive,
    baseline: &ace_model::VehicleModel,
    build: &str,
) -> AexResult<()> {
    if archive.candidates.len() > 64 {
        return Err(AexError::validation(
            "STUDY_LIMIT",
            "study",
            "Sheet accepts at most 64 candidates per archive",
        ));
    }
    archive.verify()?;
    if &archive.baseline != baseline
        || archive
            .candidates
            .iter()
            .any(|candidate| candidate.state.build.digest != build)
    {
        return Err(AexError::validation(
            "REVISION_CONFLICT",
            "study",
            "study must use this exact model revision and implementation build",
        ));
    }
    for candidate in &archive.candidates {
        ace_sheet::project(&candidate.model, &archive.snapshot(&candidate.id)?)?;
    }
    Ok(())
}
