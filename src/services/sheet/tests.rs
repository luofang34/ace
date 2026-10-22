use super::*;
use serde_json::{Value, json};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
mod accounting;

#[test]
fn active_constraints_project_current_loading_and_volume_section_witnesses() {
    let model: ace_model::VehicleModel =
        serde_json::from_str(include_str!("../../../examples/synthesis/transport.json")).unwrap();
    let mut session =
        super::super::synthesis::create_session_blocking(model, &BTreeMap::new()).unwrap();
    let snapshot = session.run().unwrap();
    let sheet = ace_sheet::project(session.model(), &snapshot).unwrap();
    let loading = sheet.focus["aft_cg"].loading.as_ref().unwrap();
    assert_eq!(loading.analysis, "loading");
    assert_eq!(loading.case.as_deref(), Some("empty_tanks"));
    let run = sheet
        .snapshot
        .runs
        .iter()
        .find(|run| run.id == loading.run_id)
        .unwrap();
    assert_eq!(
        run.details["loadings"]["empty_tanks"]["state"]["cg_m"][0],
        run.details["maximum_cg_x_m"]
    );
    let sections = &sheet.focus["fuel_volume"].sections;
    assert!(
        sections
            .iter()
            .any(|section| section.section == "volume.tank")
    );
    assert!(
        sections
            .iter()
            .all(|section| section.section != "clearance.gear_spar")
    );
    for section in sections {
        assert!(
            sheet
                .snapshot
                .runs
                .iter()
                .any(|run| run.id == section.run_id)
        );
    }
    session
        .edit(
            &BTreeMap::from([(
                "wing.span".into(),
                ace_model::Quantity::new(37.0, ace_model::Unit::Meter).unwrap(),
            )]),
            ace_model::Source {
                kind: "test".into(),
                reference: "edit span".into(),
                revision: None,
            },
        )
        .unwrap();
    let stale = ace_sheet::project(session.model(), &session.snapshot()).unwrap();
    assert!(!stale.focus.contains_key("aft_cg"));
    assert!(!stale.focus.contains_key("fuel_volume"));
    let snapshot = session.run().unwrap();
    let updated = ace_sheet::project(session.model(), &snapshot).unwrap();
    assert_ne!(
        updated.focus["aft_cg"].loading.as_ref().unwrap().run_id,
        loading.run_id
    );
}

#[test]
fn loading_focus_distinguishes_current_state_and_inapplicable_constraints() {
    let mut model: ace_model::VehicleModel =
        serde_json::from_str(include_str!("../../../examples/synthesis/transport.json")).unwrap();
    model
        .analyses
        .get_mut("loading")
        .unwrap()
        .resources
        .loading_cases
        .clear();
    model
        .requirements
        .get_mut("fuel_volume")
        .unwrap()
        .condition
        .insert(
            "wing.span".into(),
            ace_model::Quantity::new(1.0, ace_model::Unit::Meter).unwrap(),
        );
    let mut session =
        super::super::synthesis::create_session_blocking(model, &BTreeMap::new()).unwrap();
    let snapshot = session.run().unwrap();
    let sheet = ace_sheet::project(session.model(), &snapshot).unwrap();
    assert!(
        sheet.focus["aft_cg"]
            .loading
            .as_ref()
            .unwrap()
            .case
            .is_none()
    );
    assert!(!sheet.focus.contains_key("fuel_volume"));
}

fn fixture_blocking() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vehicle.json");
    std::fs::write(
        &path,
        include_str!("../../../examples/synthesis/transport.json"),
    )
    .unwrap();
    (directory, path)
}

/// Detects a hung exchange; generous enough that a loaded host running unoptimized
/// synthesis never reaches it, so it never decides a behavioral outcome.
const HANG_GUARD: Duration = Duration::from_secs(600);

/// Behavioral tests must not race the interactive deadlines in unoptimized builds.
async fn start_unhurried(path: &std::path::Path) -> AexResult<SheetServer> {
    let limits = jobs::JobLimits {
        preview: HANG_GUARD,
        formal: HANG_GUARD,
    };
    start_with_limits(path, 0, &BTreeMap::new(), limits).await
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

async fn request(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: Option<Value>,
    origin: Option<&str>,
) -> (u16, Value) {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let body = body.map(|value| value.to_string()).unwrap_or_default();
    let origin = origin
        .map(|value| format!("Origin: {value}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{origin}Connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    tokio::time::timeout(HANG_GUARD, stream.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    let response = String::from_utf8(bytes).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (
        status,
        serde_json::from_str(body).unwrap_or_else(|_| Value::String(body.into())),
    )
}

async fn events(address: SocketAddr) -> BufReader<TcpStream> {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(format!("GET /api/events HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    BufReader::new(stream)
}

async fn next_state(events: &mut BufReader<TcpStream>, revision: &str) -> Value {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let mut line = String::new();
            assert!(
                events.read_line(&mut line).await.unwrap() > 0,
                "event stream ended"
            );
            if let Some(data) = line
                .strip_prefix("data: ")
                .or_else(|| line.strip_prefix("data:"))
            {
                let value: Value = serde_json::from_str(data.trim()).unwrap();
                if value["activity"] == "ready" && value["sheet"]["revision"] == revision {
                    return value;
                }
            }
        }
    })
    .await
    .unwrap()
}

async fn shutdown_with_events(server: SheetServer, events: &mut BufReader<TcpStream>) {
    // Continue consuming the response while closing, as an event-stream client does.
    // An unread publication can fill the socket and intentionally reach the shutdown deadline.
    let drain = async {
        let mut tail = Vec::new();
        tokio::time::timeout(HANG_GUARD, events.read_to_end(&mut tail))
            .await
            .unwrap()
            .unwrap();
    };
    let (shutdown, ()) = tokio::join!(server.shutdown(), drain);
    shutdown.unwrap();
}

#[test]
fn previews_past_their_limit_fail_without_changing_the_design() {
    let defaults = jobs::JobLimits::default();
    assert_eq!(defaults.preview, Duration::from_secs(15));
    assert_eq!(defaults.formal, Duration::from_secs(60));
    let (_directory, path) = fixture_blocking();
    runtime().block_on(async {
        let limits = jobs::JobLimits {
            preview: Duration::from_millis(1),
            formal: Duration::from_secs(600),
        };
        let server = start_with_limits(&path, 0, &BTreeMap::new(), limits)
            .await
            .unwrap();
        let address = server.address;
        let mut stream = events(address).await;
        next_state(&mut stream, "0").await;
        let edit = json!({"expected_revision":"0","updates":{"wing.span":{"value":37,"unit":"m"}}});
        let (status, _) = request(address, "POST", "/api/preview", Some(edit), None).await;
        assert_ne!(status, 200);
        assert_eq!(
            request(address, "GET", "/api/state", None, None).await.1["sheet"]["revision"],
            "0"
        );
        shutdown_with_events(server, &mut stream).await;
    });
}

#[test]
fn live_sheet_edits_preview_revision_conflicts_and_saves_use_one_authoritative_session() {
    let (_directory, path) = fixture_blocking();
    runtime().block_on(async {
        let server = start_unhurried(&path).await.unwrap();
        let address = server.address;
        let mut stream = events(address).await;
        let initial = next_state(&mut stream, "0").await;
        let edit = json!({"expected_revision":"0","updates":{"wing.span":{"value":37,"unit":"m"}}});
        let (status, preview) =
            request(address, "POST", "/api/preview", Some(edit.clone()), None).await;
        assert_eq!(status, 200);
        assert_eq!(preview["mode"], "preview");
        assert_eq!(preview["sheet"]["margins"]["gate_span"]["status"], "FAIL");
        assert_eq!(
            request(address, "GET", "/api/state", None, None).await.1["sheet"]["revision"],
            "0"
        );
        assert_eq!(
            request(address, "POST", "/api/edit", Some(edit.clone()), None)
                .await
                .0,
            202
        );
        let changed = next_state(&mut stream, "1").await;
        assert_eq!(
            changed["sheet"]["margins"]["remaining_fuel"]["status"],
            "WARN"
        );
        assert_eq!(changed["sheet"]["assessment"]["execution"], "complete");
        assert_eq!(changed["sheet"]["assessment"]["feasibility"], "violated");
        assert_eq!(
            changed["sheet"]["margins"]["remaining_fuel"]["provisional"],
            true
        );
        assert!(
            changed["sheet"]["snapshot"]["executed"]
                .as_array()
                .unwrap()
                .len()
                < initial["sheet"]["snapshot"]["executed"]
                    .as_array()
                    .unwrap()
                    .len()
        );
        assert_eq!(
            request(address, "POST", "/api/edit", Some(edit), None)
                .await
                .0,
            409
        );
        assert_eq!(
            request(
                address,
                "POST",
                "/api/save",
                Some(json!({"expected_revision":"1"})),
                None
            )
            .await
            .0,
            204
        );
        let stored: ace_model::VehicleModel =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(stored.parameters["wing.span"].value.unwrap().value, 37.0);
        shutdown_with_events(server, &mut stream).await;
    });
}

#[test]
fn outside_origins_and_external_file_changes_cannot_mutate_the_design() {
    let (_directory, path) = fixture_blocking();
    runtime().block_on(async {
        let server = start_unhurried(&path).await.unwrap();
        let address = server.address;
        let mut stream = events(address).await;
        next_state(&mut stream, "0").await;
        let edit = json!({"expected_revision":"0","updates":{"wing.span":{"value":37,"unit":"m"}}});
        assert_eq!(
            request(
                address,
                "POST",
                "/api/edit",
                Some(edit),
                Some("http://unrelated.invalid")
            )
            .await
            .0,
            403
        );
        let mut external: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        external["name"] = json!("External edit");
        let bytes = serde_json::to_vec(&external).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let (status, error) = request(
            address,
            "POST",
            "/api/save",
            Some(json!({"expected_revision":"0"})),
            None,
        )
        .await;
        assert_eq!(status, 409);
        assert_eq!(error["code"], "EXTERNAL_EDIT");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        shutdown_with_events(server, &mut stream).await;
    });
}

#[test]
fn trace_contains_every_resource_and_pending_panels_retain_identified_history() {
    let model: ace_model::VehicleModel =
        serde_json::from_str(include_str!("../../../examples/synthesis/transport.json")).unwrap();
    let mut session =
        super::super::synthesis::create_session_blocking(model, &BTreeMap::new()).unwrap();
    let snapshot = session.run().unwrap();
    let sheet = ace_sheet::project(session.model(), &snapshot).unwrap();
    for link in &sheet.trace.links {
        assert!(sheet.trace.nodes.contains_key(&link.from), "{}", link.from);
        assert!(sheet.trace.nodes.contains_key(&link.to), "{}", link.to);
    }
    for section in session.model().sections.keys() {
        assert!(
            sheet
                .trace
                .nodes
                .contains_key(&format!("section:{section}"))
        );
    }
    for closure in session.model().closures.keys() {
        assert!(
            sheet
                .trace
                .nodes
                .contains_key(&format!("closure:{closure}"))
        );
    }
    assert!(
        sheet
            .trace
            .links
            .iter()
            .any(|link| { link.from == "parameter:wing.span" && link.to == "operations:case" })
    );
    session
        .edit(
            &BTreeMap::from([(
                "wing.span".into(),
                ace_model::Quantity::new(37.0, ace_model::Unit::Meter).unwrap(),
            )]),
            ace_model::Source {
                kind: "test".into(),
                reference: "sheet edit".into(),
                revision: None,
            },
        )
        .unwrap();
    let pending = ace_sheet::project(session.model(), &session.snapshot()).unwrap();
    let panel = pending
        .panels
        .iter()
        .find(|panel| panel.plugin == "vehicle.geometry_envelopes")
        .unwrap();
    assert!(panel.historical);
    assert_eq!(panel.status, ace_engine::ValueStatus::Stale);
    assert!(
        pending
            .snapshot
            .runs
            .iter()
            .any(|run| run.id == panel.run_id)
    );
    assert_eq!(
        pending.margins["remaining_fuel"].status,
        ace_engine::MarginStatus::Unknown
    );
    let snapshot = session.run().unwrap();
    let current = ace_sheet::project(session.model(), &snapshot).unwrap();
    assert!(current.panels.iter().all(|panel| !panel.historical));
}

#[test]
fn model_download_preserves_integer_revisions_without_browser_round_trip() {
    let (_directory, path) = fixture_blocking();
    let mut model: ace_model::VehicleModel =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    model.revision = u64::MAX;
    std::fs::write(&path, serde_json::to_vec(&model).unwrap()).unwrap();
    runtime().block_on(async {
        let server = start_unhurried(&path).await.unwrap();
        let (status, exported) = request(server.address, "GET", "/api/model", None, None).await;
        assert_eq!(status, 200);
        let exported: ace_model::VehicleModel = serde_json::from_value(exported).unwrap();
        assert_eq!(exported, model);
        server.shutdown().await.unwrap();
    });
}

#[test]
fn agent_tool_module_is_served_from_the_local_origin() {
    let (_directory, path) = fixture_blocking();
    runtime().block_on(async {
        let server = start(&path, 0, &BTreeMap::new()).await.unwrap();
        let (status, body) = request(server.address, "GET", "/agent.js", None, None).await;
        assert_eq!(status, 200);
        let source = body.as_str().unwrap();
        assert!(source.contains("document?.modelContext"));
        assert!(source.contains("ace.edit_parameters"));
        let (status, page) = request(server.address, "GET", "/app.js", None, None).await;
        assert_eq!(status, 200);
        assert!(page.as_str().unwrap().contains("registerAgentTools"));
        server.shutdown().await.unwrap();
    });
}

#[test]
fn kernel_viewer_module_and_card_are_served_from_the_local_origin() {
    let (_directory, path) = fixture_blocking();
    runtime().block_on(async {
        let server = start(&path, 0, &BTreeMap::new()).await.unwrap();
        let (status, body) = request(server.address, "GET", "/viewer.js", None, None).await;
        assert_eq!(status, 200);
        assert!(body.as_str().unwrap().contains("export function parseObj"));
        let (status, page) = request(server.address, "GET", "/", None, None).await;
        assert_eq!(status, 200);
        assert!(page.as_str().unwrap().contains("id=\"kernel\""));
        let (status, view) = request(server.address, "GET", "/view.js", None, None).await;
        assert_eq!(status, 200);
        assert!(view.as_str().unwrap().contains("vehicle.openvsp_geometry"));
        server.shutdown().await.unwrap();
    });
}
