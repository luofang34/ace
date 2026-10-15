use super::{AppState, worker};
use crate::domain::diagnostic::{AexError, AexResult};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use std::convert::Infallible;
use tokio::sync::oneshot;
use tokio_stream::{Stream, StreamExt, wrappers::WatchStream};

pub(super) fn routes(state: AppState) -> Router {
    Router::new()
        .route("/api/state", get(current))
        .route("/api/model", get(model))
        .route("/api/events", get(events))
        .route("/api/edit", post(edit))
        .route("/api/preview", post(preview))
        .route("/api/save", post(save))
        .route("/api/run", post(run))
        .route("/api/cancel", post(cancel))
        .route(
            "/api/study/import",
            post(super::study::import).layer(DefaultBodyLimit::max(128 * 1024 * 1024)),
        )
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), boundary))
        .with_state(state)
}

async fn current(State(state): State<AppState>) -> Json<std::sync::Arc<super::Publication>> {
    Json(state.current.borrow().clone())
}

async fn model(State(state): State<AppState>) -> Response {
    let model = state.current.borrow().sheet.model.clone();
    let mut response = Json(model).into_response();
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_static("attachment; filename=vehicle.json"),
    );
    response
}

async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = WatchStream::new(state.current)
        .take_while(|publication| publication.activity != "stopped")
        .map(|publication| {
            Ok(Event::default()
                .json_data(publication.as_ref())
                .unwrap_or_else(|error| {
                    tracing::error!(%error,"failed to serialize sheet event");
                    Event::default().event("error").data(error.to_string())
                }))
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn edit(
    State(state): State<AppState>,
    Json(edit): Json<worker::Edit>,
) -> Result<StatusCode, ApiError> {
    let (reply, response) = oneshot::channel();
    enqueue(&state, worker::Command::Edit(edit, reply))?;
    receive(response).await?;
    Ok(StatusCode::ACCEPTED)
}

async fn preview(
    State(state): State<AppState>,
    Json(edit): Json<worker::Edit>,
) -> Result<Json<worker::Preview>, ApiError> {
    let (reply, response) = oneshot::channel();
    enqueue(&state, worker::Command::Preview(edit, reply))?;
    Ok(Json(receive(response).await?))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    expected_revision: String,
}

async fn run(
    State(state): State<AppState>,
    Json(request): Json<Save>,
) -> Result<StatusCode, ApiError> {
    let (reply, response) = oneshot::channel();
    enqueue(
        &state,
        worker::Command::Run(request.expected_revision, reply),
    )?;
    receive(response).await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Cancel {
    job_id: String,
}

async fn cancel(
    State(state): State<AppState>,
    Json(request): Json<Cancel>,
) -> Result<StatusCode, ApiError> {
    let (reply, response) = oneshot::channel();
    enqueue(&state, worker::Command::Cancel(request.job_id, reply))?;
    receive(response).await?;
    Ok(StatusCode::ACCEPTED)
}

async fn save(
    State(state): State<AppState>,
    Json(save): Json<Save>,
) -> Result<StatusCode, ApiError> {
    let (reply, response) = oneshot::channel();
    enqueue(&state, worker::Command::Save(save.expected_revision, reply))?;
    receive(response).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn enqueue(state: &AppState, command: worker::Command) -> Result<(), ApiError> {
    state.commands.try_send(command).map_err(|_| {
        ApiError(AexError::validation(
            "WORKER_UNAVAILABLE",
            "sheet",
            "design worker is busy or stopped; retry after the current update",
        ))
    })
}

async fn receive<T>(response: oneshot::Receiver<AexResult<T>>) -> Result<T, ApiError> {
    response
        .await
        .map_err(|error| ApiError(super::service_error("receive sheet command", error)))?
        .map_err(ApiError)
}

pub(super) struct ApiError(AexError);

impl From<AexError> for ApiError {
    fn from(error: AexError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let detail = self.0.detail();
        let status = match detail.code.as_str() {
            "REVISION_CONFLICT" | "EXTERNAL_EDIT" => StatusCode::CONFLICT,
            "WORKER_UNAVAILABLE" => StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_ERROR" => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        };
        (status, Json(detail)).into_response()
    }
}

async fn boundary(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    let expected_origin = format!("http://{}", state.authority);
    if host != Some(state.authority.as_str())
        || origin.is_some_and(|origin| origin != expected_origin)
    {
        return (
            StatusCode::FORBIDDEN,
            "sheet requests must use the announced local origin",
        )
            .into_response();
    }
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}
