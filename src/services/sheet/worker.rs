use super::{Publication, persistence::DocumentFile, service_error};
use crate::domain::diagnostic::{AexError, AexResult};
use ace_engine::Session;
use ace_model::{Quantity, Source};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, watch};

#[cfg(test)]
mod tests;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Edit {
    pub expected_revision: String,
    #[serde(default)]
    pub updates: BTreeMap<String, Quantity>,
    #[serde(default)]
    pub edits: Vec<ace_model::ModelEdit>,
}

#[derive(Serialize)]
pub(super) struct Preview {
    pub(super) base_revision: String,
    pub(super) mode: &'static str,
    pub(super) sheet: ace_sheet::Sheet,
}

pub(super) enum Command {
    Edit(Edit, oneshot::Sender<AexResult<()>>),
    Preview(Edit, oneshot::Sender<AexResult<Preview>>),
    Save(String, oneshot::Sender<AexResult<()>>),
    Cancel(String, oneshot::Sender<AexResult<()>>),
    Run(String, oneshot::Sender<AexResult<()>>),
}

struct Worker {
    session: Session,
    file: DocumentFile,
    publish: watch::Sender<Arc<Publication>>,
    jobs: super::jobs::Jobs,
    formal: Option<super::jobs::JobStatus>,
    preview: Option<String>,
}

pub(super) async fn run(
    session: Session,
    file: DocumentFile,
    mut commands: mpsc::Receiver<Command>,
    publish: watch::Sender<Arc<Publication>>,
    mut stopped: watch::Receiver<bool>,
    limits: super::jobs::JobLimits,
) -> AexResult<()> {
    let mut worker = Worker {
        session,
        file,
        publish,
        jobs: super::jobs::Jobs::new(limits),
        formal: None,
        preview: None,
    };
    let result = worker.drive(&mut commands, &mut stopped).await;
    let cleanup = worker.jobs.shutdown().await;
    let mut final_state = worker.publish.borrow().as_ref().clone();
    final_state.activity = if result.is_err() { "error" } else { "stopped" }.into();
    final_state.error = result.as_ref().err().map(ToString::to_string);
    worker.publish.send_replace(Arc::new(final_state));
    result.and(cleanup)
}

impl Worker {
    async fn drive(
        &mut self,
        commands: &mut mpsc::Receiver<Command>,
        stopped: &mut watch::Receiver<bool>,
    ) -> AexResult<()> {
        self.start_formal()?;
        loop {
            tokio::select! {
                changed = stopped.changed() => { changed.ok(); break; },
                command = commands.recv() => {
                    match command { Some(command) => self.command(command).await?, None => break }
                },
                result = self.jobs.tasks.join_next(), if !self.jobs.tasks.is_empty() => {
                    if let Some(result) = result {
                        self.complete(result.map_err(|source| service_error("synthesis monitor",source))?)?;
                    }
                },
            }
        }
        Ok(())
    }

    fn start_formal(&mut self) -> AexResult<()> {
        if let Some(job) = &self.formal {
            self.jobs.cancel(&job.id);
        }
        self.formal = Some(self.jobs.start(
            self.session.fork(),
            None,
            self.session.model().revision.to_string(),
        ));
        self.publish_state("computing", None)
    }

    async fn command(&mut self, command: Command) -> AexResult<()> {
        match command {
            Command::Edit(edit, reply) => {
                let result = edit_session(&mut self.session, &edit, "sheet editor");
                let accepted = result.is_ok();
                reply.send(result).ok();
                if accepted {
                    self.start_formal()?;
                }
            }
            Command::Preview(edit, reply) => self.start_preview(edit, reply),
            Command::Save(revision, reply) => {
                let result = self.save(&revision).await;
                reply.send(result).ok();
            }
            Command::Run(revision, reply) => {
                let result =
                    check_revision(&self.session, &revision).and_then(|()| self.start_formal());
                reply.send(result).ok();
            }
            Command::Cancel(id, reply) => {
                let result = if self
                    .formal
                    .as_ref()
                    .is_some_and(|job| job.id == id && job.state == "running")
                {
                    self.jobs.cancel(&id);
                    self.publish_state("canceling", None)
                } else {
                    Err(AexError::validation(
                        "REVISION_CONFLICT",
                        "job_id",
                        "job is not the current running design",
                    ))
                };
                reply.send(result).ok();
            }
        }
        Ok(())
    }

    fn start_preview(&mut self, edit: Edit, reply: oneshot::Sender<AexResult<Preview>>) {
        let branch = check_revision(&self.session, &edit.expected_revision)
            .and_then(|()| Ok(self.session.preview_branch()?))
            .and_then(|mut branch| {
                edit_session(&mut branch, &edit, "isolated preview")?;
                Ok(branch)
            });
        match branch {
            Ok(branch) => {
                if let Some(id) = &self.preview {
                    self.jobs.cancel(id);
                }
                let job = self.jobs.start(branch, Some(reply), edit.expected_revision);
                self.preview = Some(job.id);
            }
            Err(error) => {
                reply.send(Err(error)).ok();
            }
        }
    }

    fn complete(&mut self, completion: super::jobs::Completion) -> AexResult<()> {
        self.jobs.finished(&completion.status.id);
        if let Some(reply) = completion.preview {
            let result = completion.result.and_then(|(session, snapshot)| {
                Ok(Preview {
                    base_revision: completion.status.revision,
                    mode: "preview",
                    sheet: ace_sheet::project(session.model(), &snapshot)?,
                })
            });
            reply.send(result).ok();
        } else if self
            .formal
            .as_ref()
            .is_some_and(|job| job.id == completion.status.id)
            && self.session.model().revision.to_string() == completion.status.revision
        {
            self.formal = Some(completion.status.clone());
            match completion.result {
                Ok((session, snapshot)) => {
                    self.session = session;
                    self.publish_snapshot(&snapshot, "ready", None)?;
                }
                Err(error) => {
                    self.publish_state(&completion.status.state, Some(error.to_string()))?;
                }
            }
        }
        Ok(())
    }

    async fn save(&mut self, revision: &str) -> AexResult<()> {
        check_revision(&self.session, revision)?;
        let model = self.session.model().clone();
        let mut file = self.file.clone();
        let file = tokio::task::spawn_blocking(move || {
            file.save_blocking(&model)?;
            Ok::<_, AexError>(file)
        })
        .await
        .map_err(|source| service_error("sheet-save", source))??;
        self.file = file;
        let mut publication = self.publish.borrow().as_ref().clone();
        publication.saved_digest = self.file.model_digest()?;
        self.publish.send_replace(Arc::new(publication));
        Ok(())
    }

    fn publish_state(&self, activity: &str, error: Option<String>) -> AexResult<()> {
        self.publish_snapshot(&self.session.snapshot(), activity, error)
    }

    fn publish_snapshot(
        &self,
        snapshot: &ace_engine::Snapshot,
        activity: &str,
        error: Option<String>,
    ) -> AexResult<()> {
        let sheet = ace_sheet::project(self.session.model(), snapshot)?;
        self.publish.send_replace(Arc::new(Publication {
            activity: activity.into(),
            sheet: Arc::new(sheet),
            error,
            saved_digest: self.file.model_digest()?,
            job: self.formal.clone(),
        }));
        Ok(())
    }
}

fn edit_session(session: &mut Session, edit: &Edit, reference: &str) -> AexResult<()> {
    check_revision(session, &edit.expected_revision)?;
    if edit.updates.is_empty() && edit.edits.is_empty() {
        return Err(AexError::validation(
            "EMPTY_EDIT",
            "updates",
            "provide parameter updates or typed structural edits",
        ));
    }
    if !edit.updates.is_empty() && !edit.edits.is_empty() {
        return Err(AexError::validation(
            "MIXED_EDIT",
            "edit",
            "use one edit representation per transaction",
        ));
    }
    if !edit.edits.is_empty() {
        session.transact(&edit.edits)?;
        return Ok(());
    }
    session.edit(
        &edit.updates,
        Source {
            kind: "user".into(),
            reference: reference.into(),
            revision: None,
        },
    )?;
    Ok(())
}

fn check_revision(session: &Session, expected: &str) -> AexResult<()> {
    if expected != session.model().revision.to_string() {
        return Err(AexError::validation(
            "REVISION_CONFLICT",
            "expected_revision",
            "design changed; refresh before applying this edit",
        ));
    }
    Ok(())
}
