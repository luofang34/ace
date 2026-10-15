use crate::domain::diagnostic::{AexError, AexResult};
use axum::Router;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

mod api;
mod assets;
mod jobs;
mod persistence;
mod study;
mod worker;

#[derive(Debug, Clone, serde::Serialize)]
struct Publication {
    job: Option<jobs::JobStatus>,
    activity: String,
    sheet: Arc<ace_sheet::Sheet>,
    error: Option<String>,
    saved_digest: String,
}

#[derive(Clone)]
struct AppState {
    commands: mpsc::Sender<worker::Command>,
    current: watch::Receiver<Arc<Publication>>,
    authority: String,
}

pub(crate) struct SheetServer {
    pub(crate) address: SocketAddr,
    stop: watch::Sender<bool>,
    tasks: Vec<(&'static str, JoinHandle<AexResult<()>>)>,
}

impl SheetServer {
    pub(crate) async fn shutdown(self) -> AexResult<()> {
        self.stop.send_replace(true);
        let mut failure = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        for (name, mut task) in self.tasks {
            match tokio::time::timeout_at(deadline, &mut task).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(error))) => {
                    tracing::error!(task=name, %error, "sheet task failed");
                    failure = Some(error);
                }
                Ok(Err(error)) => {
                    failure = Some(service_error(name, error));
                }
                Err(error) => {
                    tracing::error!(
                        task = name,
                        "sheet task did not exit before shutdown deadline; aborting"
                    );
                    task.abort();
                    failure = Some(service_error(name, error));
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

pub(crate) async fn start(
    path: &Path,
    port: u16,
    overrides: &BTreeMap<String, String>,
) -> AexResult<SheetServer> {
    let path = path.to_path_buf();
    let overrides = overrides.clone();
    let (session, file) = tokio::task::spawn_blocking(move || {
        let file = persistence::DocumentFile::open_blocking(path)?;
        let session = super::synthesis::create_session_blocking(file.model().clone(), &overrides)?;
        Ok::<_, AexError>((session, file))
    })
    .await
    .map_err(|error| service_error("open sheet model", error))??;
    let sheet = ace_sheet::project(session.model(), &session.snapshot())?;
    let publication = Arc::new(Publication {
        job: None,
        activity: "computing".into(),
        sheet: Arc::new(sheet),
        error: None,
        saved_digest: file.model_digest()?,
    });
    let (publish, current) = watch::channel(publication);
    let (commands, receiver) = mpsc::channel(16);
    let (stop, stopped) = watch::channel(false);
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
        .await
        .map_err(|error| service_error("bind synthesis sheet", error))?;
    let address = listener
        .local_addr()
        .map_err(|error| service_error("read sheet listener address", error))?;
    let state = AppState {
        commands,
        current,
        authority: address.to_string(),
    };
    let routes = api::routes(state).merge(assets::routes());
    let compute = tokio::spawn(worker::run(
        session,
        file,
        receiver,
        publish,
        stopped.clone(),
    ));
    let server = tokio::spawn(serve(listener, routes, stopped));
    Ok(SheetServer {
        address,
        stop,
        tasks: vec![("design-worker", compute), ("sheet-http", server)],
    })
}

async fn serve(
    listener: TcpListener,
    routes: Router,
    mut stopped: watch::Receiver<bool>,
) -> AexResult<()> {
    axum::serve(listener, routes)
        .with_graceful_shutdown(async move {
            if !*stopped.borrow() {
                stopped.changed().await.ok();
            }
        })
        .await
        .map_err(|error| service_error("serve synthesis sheet", error))
}

fn service_error(
    operation: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> AexError {
    AexError::Service {
        operation,
        source: Box::new(source),
    }
}

#[cfg(test)]
mod tests;
