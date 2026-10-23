use super::*;
use ace_engine::{
    AnalysisContext, AnalysisPlugin, PluginOutput, PluginSpec, Registry, SolverError,
};
use ace_model::{Unit, VehicleModel};
use tokio::sync::Notify;

struct Gated {
    spec: PluginSpec,
    started: Arc<Notify>,
    release: Arc<Notify>,
}

impl AnalysisPlugin for Gated {
    fn spec(&self) -> &PluginSpec {
        &self.spec
    }
    fn evaluate(
        &self,
        inputs: &BTreeMap<String, Quantity>,
        _: &AnalysisContext<'_>,
    ) -> Result<PluginOutput, SolverError> {
        let input = inputs["input"];
        if input.value == 1.0 {
            self.started.notify_one();
            runtime().block_on(self.release.notified());
        }
        Ok(PluginOutput {
            values: BTreeMap::from([("output".into(), input)]),
            approximate: false,
            artifacts: Vec::new(),
            details: serde_json::Value::Null,
            failure: None,
        })
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn fixture_blocking() -> (Worker, Arc<Notify>, Arc<Notify>, tempfile::TempDir) {
    let model: VehicleModel = serde_json::from_value(serde_json::json!({
        "schema_version":1,"id":"jobs","name":"Job isolation","domain":"generic",
        "parameters":{
            "x":{"unit":"kg","value":{"value":1,"unit":"kg"},"owner":"test","source":{"kind":"test","reference":"fixture"}},
            "y":{"unit":"kg","owner":"test","source":{"kind":"test","reference":"fixture"}}
        },
        "analyses":{"scale":{"plugin":"scale","version":"1","inputs":{"input":"x"},"outputs":{"output":"y"}}}
    })).unwrap();
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut registry = Registry::default();
    registry
        .register(Arc::new(Gated {
            spec: PluginSpec {
                mode: ace_model::AnalysisMode::Scalar,
                domain: Default::default(),
                identity: ace_engine::PluginIdentity {
                    id: "scale".into(),
                    version: "1".into(),
                    fidelity: ace_engine::Fidelity::Conceptual,
                    code_revision: "fixture".into(),
                },
                inputs: BTreeMap::from([("input".into(), Unit::Kilogram.into())]),
                outputs: BTreeMap::from([("output".into(), Unit::Kilogram.into())]),
            },
            started: started.clone(),
            release: release.clone(),
        }))
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vehicle.json");
    std::fs::write(&path, serde_json::to_vec(&model).unwrap()).unwrap();
    let file = DocumentFile::open_blocking(path).unwrap();
    let session = Session::new(model, registry).unwrap();
    let publication = Publication {
        job: None,
        activity: "idle".into(),
        error: None,
        saved_digest: file.model_digest().unwrap(),
        sheet: Arc::new(ace_sheet::project(session.model(), &session.snapshot()).unwrap()),
    };
    let (publish, _) = watch::channel(Arc::new(publication));
    (
        Worker {
            session,
            file,
            publish,
            jobs: crate::services::sheet::jobs::Jobs::new(Default::default()),
            formal: None,
            preview: None,
        },
        started,
        release,
        directory,
    )
}

async fn finish(worker: &mut Worker) {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        worker.jobs.tasks.join_next(),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    worker.complete(result).unwrap();
}

#[test]
fn a_delayed_old_job_cannot_overwrite_the_new_design() {
    runtime().block_on(async {
        let (mut worker, started, release, _directory) = fixture_blocking();
        worker.start_formal().unwrap();
        started.notified().await;
        let old = worker.formal.as_ref().unwrap().id.clone();
        let (reply, response) = oneshot::channel();
        worker
            .command(Command::Edit(
                Edit {
                    expected_revision: "0".into(),
                    updates: BTreeMap::from([(
                        "x".into(),
                        Quantity::new(2.0, Unit::Kilogram).unwrap(),
                    )]),
                    edits: Vec::new(),
                },
                reply,
            ))
            .await
            .unwrap();
        response.await.unwrap().unwrap();
        finish(&mut worker).await;
        let published = worker.publish.borrow().clone();
        assert_eq!(published.sheet.revision, "1");
        assert_eq!(
            published.sheet.snapshot.values["y"].quantity.unwrap().value,
            2.0
        );
        assert_ne!(published.job.as_ref().unwrap().id, old);
        release.notify_one();
        finish(&mut worker).await;
        assert!(Arc::ptr_eq(&published, &worker.publish.borrow()));
        worker.jobs.shutdown().await.unwrap();
    });
}

#[test]
fn canceled_work_cannot_publish_successful_values() {
    runtime().block_on(async {
        let (mut worker, started, release, _directory) = fixture_blocking();
        worker.start_formal().unwrap();
        started.notified().await;
        let id = worker.formal.as_ref().unwrap().id.clone();
        let (reply, response) = oneshot::channel();
        worker.command(Command::Cancel(id, reply)).await.unwrap();
        response.await.unwrap().unwrap();
        release.notify_one();
        finish(&mut worker).await;
        let published = worker.publish.borrow().clone();
        assert_eq!(published.job.as_ref().unwrap().state, "canceled");
        assert!(published.sheet.snapshot.values["y"].quantity.is_none());
        worker.jobs.shutdown().await.unwrap();
    });
}
