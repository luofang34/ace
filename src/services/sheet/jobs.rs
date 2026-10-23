use super::{service_error, worker::Preview};
use crate::domain::diagnostic::{AexError, AexResult};
use ace_engine::{RunControl, Session, Snapshot};
use serde::Serialize;
use std::{collections::BTreeMap, time::Duration};
use tokio::{sync::oneshot, task::JoinSet};

#[derive(Debug, Clone, Serialize)]
pub(super) struct JobStatus {
    pub id: String,
    pub revision: String,
    pub kind: String,
    pub state: String,
    pub timeout_seconds: u64,
}

pub(super) struct Completion {
    pub status: JobStatus,
    pub result: AexResult<(Session, Snapshot)>,
    pub preview: Option<oneshot::Sender<AexResult<Preview>>>,
}

/// Wall-clock limits that keep interactive previews responsive and bound formal runs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct JobLimits {
    /// Isolated preview of an unsaved edit.
    pub preview: Duration,
    /// Authoritative recomputation of the current design.
    pub formal: Duration,
}

impl Default for JobLimits {
    fn default() -> Self {
        Self {
            preview: Duration::from_secs(15),
            formal: Duration::from_secs(60),
        }
    }
}

pub(super) struct Jobs {
    pub tasks: JoinSet<Completion>,
    controls: BTreeMap<String, RunControl>,
    sequence: u64,
    limits: JobLimits,
}

impl Jobs {
    pub fn new(limits: JobLimits) -> Self {
        Self {
            tasks: JoinSet::new(),
            controls: BTreeMap::new(),
            sequence: 0,
            limits,
        }
    }

    pub fn start(
        &mut self,
        session: Session,
        preview: Option<oneshot::Sender<AexResult<Preview>>>,
        base_revision: String,
    ) -> JobStatus {
        self.sequence = self.sequence.wrapping_add(1);
        let kind = if preview.is_some() {
            "preview"
        } else {
            "formal"
        };
        let limit = if preview.is_some() {
            self.limits.preview
        } else {
            self.limits.formal
        };
        let timeout_seconds = limit.as_secs().max(1);
        let status = JobStatus {
            id: format!("{kind}-{}-r{base_revision}", self.sequence),
            revision: base_revision,
            kind: kind.into(),
            state: "running".into(),
            timeout_seconds,
        };
        let control = RunControl::bounded(limit);
        self.controls.insert(status.id.clone(), control.clone());
        self.tasks
            .spawn(execute(session, status.clone(), control, preview));
        status
    }

    pub fn cancel(&self, id: &str) {
        if let Some(control) = self.controls.get(id) {
            control.cancel();
        }
    }
    pub fn finished(&mut self, id: &str) {
        self.controls.remove(id);
    }

    pub async fn shutdown(&mut self) -> AexResult<()> {
        for control in self.controls.values() {
            control.cancel();
        }
        let drained = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(completion) = self.tasks.join_next().await {
                match completion {
                    Ok(completion) => {
                        self.controls.remove(&completion.status.id);
                    }
                    Err(error) => {
                        tracing::error!(%error,"execution monitor failed during shutdown");
                    }
                }
            }
        })
        .await;
        if let Err(source) = drained {
            let names: Vec<_> = self.controls.keys().cloned().collect();
            tracing::error!(tasks=?names,"execution tasks did not stop; aborting monitors");
            self.tasks.abort_all();
            return Err(service_error("execution shutdown", source));
        }
        Ok(())
    }
}

async fn execute(
    mut session: Session,
    mut status: JobStatus,
    control: RunControl,
    preview: Option<oneshot::Sender<AexResult<Preview>>>,
) -> Completion {
    let token = control.clone();
    let mut task = tokio::task::spawn_blocking(move || {
        let result = session.run_controlled(&token);
        result
            .map(|snapshot| (session, snapshot))
            .map_err(AexError::from)
    });
    let result =
        match tokio::time::timeout(Duration::from_secs(status.timeout_seconds), &mut task).await {
            Ok(result) => result
                .map_err(|source| service_error("synthesis job", source))
                .and_then(|result| result),
            Err(_) => {
                control.cancel();
                status.state = "timed_out".into();
                // Cooperative native adapters and supervised child processes release their resources before this monitor exits.
                task.await.ok();
                Err(ace_engine::EngineError::from(ace_engine::Interruption::TimedOut).into())
            }
        };
    if status.state != "timed_out" {
        status.state = if result.is_ok() {
            "completed"
        } else {
            match control.checkpoint() {
                Err(ace_engine::Interruption::Canceled) => "canceled",
                Err(ace_engine::Interruption::TimedOut) => "timed_out",
                Ok(()) => "failed",
            }
        }
        .into();
    }
    Completion {
        status,
        result,
        preview,
    }
}
